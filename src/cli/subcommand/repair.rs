use std::io::IsTerminal as _;
use std::process::ExitCode;
use std::time::SystemTime;

use clap::{ArgAction, Parser, Subcommand};
use eyre::Context as _;
use serde::{Deserialize, Serialize};
use target_lexicon::OperatingSystem;
use tokio::process::Command;

use crate::action::base::{AddUserToGroup, CreateGroup, CreateUser};
use crate::action::common::{ConfigureShellProfile, CreateUsersAndGroups};
use crate::action::{Action, ActionState, StatefulAction};
use crate::cli::interaction::PromptChoice;
use crate::cli::{ensure_root, CommandExecute};
use crate::plan::RECEIPT_LOCATION;
use crate::planner::{PlannerError, ShellProfileLocations};
use crate::{execute_command, InstallPlan};

/// The base UID that we temporarily move build users to while migrating macOS to the new range.
const TEMP_USER_ID_BASE: u32 = 31000;

/**
Various actions to repair Nix installations.

The default is to repair shell hooks.
*/
#[derive(Debug, Parser)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Repair {
    #[clap(
        long,
        env = "NIX_INSTALLER_NO_CONFIRM",
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub no_confirm: bool,

    #[command(subcommand)]
    command: Option<RepairKind>,
}

#[derive(Clone, Debug, Subcommand, serde::Deserialize, serde::Serialize)]
pub enum RepairKind {
    /// Update the shell profiles to make Nix usable after system upgrades.
    Hooks,
    /// Recover from the macOS 15 Sequoia update taking over _nixbld users.
    ///
    /// Default functionality is to only attempt the fix if _nixbld users are missing.
    ///
    /// Can be run before taking a macOS 15 Sequoia update by passing the `--move-existing-users`
    /// flag (which will move the Nix build users to the new UID range even if they all currently
    /// exist).
    Sequoia {
        /// The Nix build user prefix (user numbers will be postfixed)
        #[cfg_attr(
            feature = "cli",
            clap(long, env = "NIX_INSTALLER_NIX_BUILD_USER_PREFIX", global = true)
        )]
        #[cfg_attr(
            all(target_os = "macos", feature = "cli"),
            clap(default_value = "_nixbld")
        )]
        #[cfg_attr(
            all(target_os = "linux", feature = "cli"),
            clap(default_value = "nixbld")
        )]
        nix_build_user_prefix: String,

        /// The number of build users to ensure exist
        #[cfg_attr(
            feature = "cli",
            clap(
                long,
                alias = "daemon-user-count",
                env = "NIX_INSTALLER_NIX_BUILD_USER_COUNT",
                default_value = "32",
                global = true
            )
        )]
        nix_build_user_count: u32,

        /// The Nix build group name
        #[cfg_attr(
            feature = "cli",
            clap(
                long,
                default_value = "nixbld",
                env = "NIX_INSTALLER_NIX_BUILD_GROUP_NAME",
                global = true
            )
        )]
        nix_build_group_name: String,

        /// If `nix-installer` should move the build users to a Sequoia-compatible range, even when
        /// they all currently exist
        #[cfg_attr(
            feature = "cli",
            clap(
                long,
                action(ArgAction::SetTrue),
                default_value = "false",
                global = true,
                env = "NIX_INSTALLER_MOVE_EXISTING_USERS"
            )
        )]
        move_existing_users: bool,
    },
}

impl Repair {
    pub fn command(&self) -> RepairKind {
        self.command.to_owned().unwrap_or(RepairKind::Hooks)
    }
}

#[async_trait::async_trait]
impl CommandExecute for Repair {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn execute<T>(self, _feedback: T) -> eyre::Result<ExitCode>
    where
        T: crate::feedback::Feedback,
    {
        let command = self.command();

        ensure_root()?;

        let mut repair_actions = Vec::new();
        let (prompt_before_repairing, brief_repair_summary) = match command {
            RepairKind::Hooks => (
                false,
                String::from("Will ensure the Nix shell profiles are still being sourced"),
            ),
            RepairKind::Sequoia {
                ref nix_build_user_prefix,
                nix_build_user_count,
                ref nix_build_group_name,
                ..
            } => {
                let maybe_users_and_groups_from_receipt = maybe_users_and_groups_from_receipt(
                    nix_build_user_prefix,
                    nix_build_user_count,
                    nix_build_group_name,
                )
                .await?;

                let user_base = crate::settings::default_nix_build_user_id_base();
                let brief_summary = format!(
                    "Will move the {nix_build_user_prefix} users to the Sequoia-compatible \
                    {user_base}+ UID range and {maybe_update_receipt} update the receipt",
                    maybe_update_receipt = if maybe_users_and_groups_from_receipt
                        .receipt_action_idx_create_group
                        .is_some()
                    {
                        "WILL"
                    } else {
                        "WILL NOT"
                    }
                );
                (!self.no_confirm, brief_summary)
            },
        };

        if prompt_before_repairing {
            loop {
                match crate::cli::interaction::prompt(
                    &brief_repair_summary,
                    PromptChoice::Yes,
                    true,
                )
                .await?
                {
                    PromptChoice::Yes => break,
                    PromptChoice::No => {
                        crate::cli::interaction::clean_exit_with_message(
                            "Okay, not continuing with the repair. Bye!",
                        )
                        .await
                    },
                    PromptChoice::Explain => (),
                }
            }
        } else {
            tracing::info!("{}", brief_repair_summary);
        }

        // TODO(cole-h): if we add another repair command, make this whole thing more generic
        let updated_receipt = match command.clone() {
            RepairKind::Hooks => {
                let reconfigure = ConfigureShellProfile::plan(ShellProfileLocations::default())
                    .await
                    .map_err(PlannerError::Action)?
                    .boxed();
                repair_actions.push(reconfigure);

                match OperatingSystem::host() {
                    OperatingSystem::MacOSX { .. } | OperatingSystem::Darwin => {
                        let reconfigure = crate::action::macos::ConfigureRemoteBuilding::plan()
                            .await
                            .map_err(PlannerError::Action)?
                            .boxed();
                        repair_actions.push(reconfigure);
                    },
                    _ => {
                        // Linux-specific hook repair actions, once we have them
                    },
                }

                None
            },
            RepairKind::Sequoia {
                nix_build_user_prefix,
                nix_build_user_count,
                nix_build_group_name,
                move_existing_users,
            } => {
                if !matches!(
                    OperatingSystem::host(),
                    OperatingSystem::MacOSX { .. } | OperatingSystem::Darwin
                ) {
                    return Err(color_eyre::eyre::eyre!(
                        "The `sequoia` repair command is only available on macOS"
                    ));
                }

                if !std::io::stdin().is_terminal() && !self.no_confirm {
                    return Err(color_eyre::eyre::eyre!(
                        "The `sequoia` repair command should be run in an interactive terminal. If \
                        you accept the risks of an unattended repair, pass `--no-confirm`."
                    ));
                }

                let user_base = crate::settings::default_nix_build_user_id_base();

                let maybe_users_and_groups_from_receipt = maybe_users_and_groups_from_receipt(
                    &nix_build_user_prefix,
                    nix_build_user_count,
                    &nix_build_group_name,
                )
                .await?;

                let user_prefix = maybe_users_and_groups_from_receipt.user_prefix;
                let user_count = maybe_users_and_groups_from_receipt.user_count;
                let group_name = maybe_users_and_groups_from_receipt.group_name;
                let group_gid = maybe_users_and_groups_from_receipt.group_gid;
                let receipt_action_idx_create_group =
                    maybe_users_and_groups_from_receipt.receipt_action_idx_create_group;

                if receipt_action_idx_create_group.is_none() {
                    tracing::warn!(
                        "Unable to find {} in receipt (receipt didn't exist or is unable to be \
                        parsed by this version of the installer). Your receipt at {RECEIPT_LOCATION} \
                        will not reflect the changed UIDs, but the users will still be relocated \
                        to the new Sequoia-compatible UID range, starting at {user_base}, and \
                        uninstallation will continue to work as normal, even if the UIDs do not match.",
                        CreateUsersAndGroups::action_tag()
                    );
                }

                let group_plist = {
                    let buf = execute_command(
                        Command::new("/usr/bin/dscl")
                            .process_group(0)
                            .args(["-plist", ".", "-read", &format!("/Groups/{group_name}")])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await?
                    .stdout;

                    let group_plist: GroupPlist = plist::from_bytes(&buf)?;
                    group_plist
                };

                let expected_users = group_plist
                    .group_membership
                    .into_iter()
                    .enumerate()
                    .map(|(idx, name)| ((idx + 1) as u32, name))
                    .collect::<Vec<_>>();

                let mut missing_users = Vec::new();
                for (user_idx, user_name) in &expected_users {
                    let ret = execute_command(
                        Command::new("/usr/bin/dscl")
                            .process_group(0)
                            .args([".", "-read", &format!("/Users/{user_name}")])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await;

                    if let Err(e) = ret {
                        tracing::debug!(%e, user_name, "Couldn't read user, assuming it's missing");
                        missing_users.push((user_idx, user_name));
                    }
                }

                if missing_users.is_empty() && !move_existing_users {
                    tracing::info!("Nothing to do! All users appear to be in place!");
                    return Ok(ExitCode::SUCCESS);
                }

                let mut existing_users = expected_users.clone();
                existing_users.retain(|(idx, _name)| {
                    !missing_users.iter().any(|(idx2, _name2)| idx == *idx2)
                });

                // NOTE(coleh-h): We move all existing build users into a temp UID range in case a
                // user customized the number of users they created and the UIDs would overlap in
                // this new range, i.e. with 128 build users, _nixbld81 prior to migration would
                // have the same ID as  _nixbld31 after the migration and would likely fail.
                for (user_idx, user_name) in existing_users {
                    let temp_user_id = TEMP_USER_ID_BASE + user_idx;

                    execute_command(
                        Command::new("/usr/bin/dscl")
                            .process_group(0)
                            // NOTE(cole-h): even though it says "create" it's really "create-or-update"
                            .args([".", "-create", &format!("/Users/{user_name}"), "UniqueID"])
                            .arg(temp_user_id.to_string())
                            .stdin(std::process::Stdio::null()),
                    )
                    .await?;
                }

                let mut create_users = Vec::with_capacity(user_count as usize);
                let group_gid = group_gid.unwrap_or(group_plist.gid);

                for (idx, name) in expected_users {
                    let create_user = CreateUser::plan(
                        name,
                        user_base + idx,
                        group_name.clone(),
                        group_gid,
                        format!("Nix build user {idx}"),
                        false,
                    )
                    .await?;
                    create_users.push(create_user);
                }

                let mut maybe_updated_receipt = None;
                if let Some((mut receipt, action_idx, create_group)) =
                    receipt_action_idx_create_group
                {
                    // NOTE(cole-h): Once we write the updated receipt, these steps will have been
                    // completed, so manually setting them to completed with
                    // StatefulAction::completed is fine.

                    let (add_users_to_groups, create_users): (
                        Vec<StatefulAction<AddUserToGroup>>,
                        Vec<StatefulAction<CreateUser>>,
                    ) = create_users
                        .iter()
                        .cloned()
                        .map(|create_user| {
                            let action = create_user.action;
                            (
                                StatefulAction::completed(AddUserToGroup {
                                    name: action.name.clone(),
                                    uid: action.uid,
                                    groupname: action.groupname.clone(),
                                    gid: action.gid,
                                }),
                                StatefulAction::completed(action),
                            )
                        })
                        .unzip();

                    let create_users_and_groups = StatefulAction::completed(CreateUsersAndGroups {
                        nix_build_group_name: group_name.clone(),
                        nix_build_group_id: group_gid,
                        nix_build_user_count: user_count,
                        nix_build_user_prefix: user_prefix.clone(),
                        nix_build_user_id_base: user_base,
                        create_group,
                        create_users: create_users.clone(),
                        add_users_to_groups,
                    });

                    let _replaced = std::mem::replace(
                        &mut receipt.actions[action_idx],
                        create_users_and_groups.boxed(),
                    );

                    maybe_updated_receipt = Some(receipt);
                }

                let create_users = create_users
                    .into_iter()
                    .map(|create_user| create_user.boxed())
                    .collect::<Vec<_>>();
                repair_actions.extend(create_users);

                maybe_updated_receipt
            },
        };

        for mut action in repair_actions {
            if let Err(err) = action.try_execute().await {
                println!("{:#?}", err);
                return Ok(ExitCode::FAILURE);
            }
            action.state = ActionState::Completed;
        }

        if let Some(updated_receipt) = updated_receipt {
            let timestamp_millis = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_millis();

            let mut old_receipt = std::path::PathBuf::from(RECEIPT_LOCATION);
            old_receipt.set_extension(format!("pre-repair.{timestamp_millis}.json"));
            tokio::fs::copy(RECEIPT_LOCATION, &old_receipt).await?;
            tracing::info!("Backed up pre-repair receipt to {}", old_receipt.display());

            updated_receipt.write_receipt().await?;
            tracing::info!("Wrote updated receipt");
        }

        tracing::info!("Finished repairing successfully!");

        Ok(ExitCode::SUCCESS)
    }
}

#[derive(Serialize, Deserialize)]
/// Structured output of `dscl -plist . -read /Groups/{name}`
struct GroupPlist {
    #[serde(rename = "dsAttrTypeStandard:GroupMembership")]
    group_membership: Vec<String>,
    #[serde(
        rename = "dsAttrTypeStandard:PrimaryGroupID",
        deserialize_with = "deserialize_gid"
    )]
    gid: u32,
}

pub fn deserialize_gid<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let s: Vec<String> = serde::Deserialize::deserialize(deserializer)?;

    let gid_str = s
        .first()
        .ok_or_else(|| serde::de::Error::invalid_length(0, &"a gid entry"))?;

    let gid: u32 = gid_str.parse().map_err(serde::de::Error::custom)?;

    Ok(gid)
}

#[tracing::instrument]
async fn get_existing_receipt() -> Option<InstallPlan> {
    match std::path::Path::new(RECEIPT_LOCATION).exists() {
        true => {
            tracing::debug!("Reading existing receipt");
            let install_plan_string = tokio::fs::read_to_string(RECEIPT_LOCATION).await.ok();

            match install_plan_string {
                Some(s) => match serde_json::from_str::<InstallPlan>(s.as_str()) {
                    Ok(plan) => {
                        tracing::debug!(plan_version = %plan.version, "Able to parse receipt");
                        Some(plan)
                    },
                    Err(e) => {
                        tracing::debug!(?e);
                        tracing::warn!("Could not parse receipt. Your receipt will not be updated to account for the new UIDs");
                        None
                    },
                },
                _ => None,
            }
        },
        false => None,
    }
}

#[tracing::instrument(skip_all)]
fn find_users_and_groups(
    existing_receipt: Option<InstallPlan>,
) -> color_eyre::Result<Option<(InstallPlan, usize, CreateUsersAndGroups)>> {
    let ret = match existing_receipt {
        Some(receipt) => {
            tracing::debug!("Got existing receipt");

            let mut maybe_create_users_and_groups_idx_action = None;
            for (idx, stateful_action) in receipt.actions.iter().enumerate() {
                let action_tag = stateful_action.inner_typetag_name();
                tracing::trace!("Found {action_tag} in receipt");

                if action_tag == CreateUsersAndGroups::action_tag().0 {
                    tracing::debug!(
                        "Found {} in receipt, preparing to roundtrip to extract the real type",
                        CreateUsersAndGroups::action_tag().0
                    );
                    // NOTE(cole-h): this round-trip is kinda jank... but Action is not
                    // object-safe, and I can't think of any other way to get the
                    // concrete `CreateUsersAndGroups` type out of a `Box<dyn Action>`.
                    let action = &stateful_action.action;
                    let create_users_and_groups_json =
                        serde_json::to_string(action).with_context(|| {
                            format!("round-tripping {action_tag} json to extract real type")
                        })?;
                    let create_users_and_groups: CreateUsersAndGroups =
                        serde_json::from_str(&create_users_and_groups_json).with_context(|| {
                            format!("round-tripping {action_tag} json to extract real type")
                        })?;

                    maybe_create_users_and_groups_idx_action =
                        Some((receipt, idx, create_users_and_groups));

                    break;
                }
            }

            maybe_create_users_and_groups_idx_action
        },
        None => {
            tracing::debug!(
                "Receipt didn't exist or is unable to be parsed by this version of the installer"
            );
            None
        },
    };

    Ok(ret)
}

struct UsersAndGroupsMeta {
    user_prefix: String,
    user_count: u32,
    group_name: String,
    group_gid: Option<u32>,
    receipt_action_idx_create_group: Option<(InstallPlan, usize, StatefulAction<CreateGroup>)>,
}

async fn maybe_users_and_groups_from_receipt(
    nix_build_user_prefix: &str,
    nix_build_user_count: u32,
    nix_build_group_name: &str,
) -> eyre::Result<UsersAndGroupsMeta> {
    let existing_receipt = get_existing_receipt().await;
    let maybe_create_users_and_groups_idx_action = find_users_and_groups(existing_receipt)?;

    match maybe_create_users_and_groups_idx_action {
        Some((receipt, create_users_and_groups_idx, action)) => {
            tracing::debug!("Found {} in receipt", CreateUsersAndGroups::action_tag());

            let user_prefix = action.nix_build_user_prefix;
            let user_count = action.nix_build_user_count;
            let group_gid = action.nix_build_group_id;
            let group_name = action.nix_build_group_name;

            Ok(UsersAndGroupsMeta {
                user_prefix,
                user_count,
                group_name,
                group_gid: Some(group_gid),
                receipt_action_idx_create_group: Some((
                    receipt,
                    create_users_and_groups_idx,
                    action.create_group,
                )),
            })
        },
        None => Ok(UsersAndGroupsMeta {
            user_prefix: nix_build_user_prefix.to_string(),
            user_count: nix_build_user_count,
            group_name: nix_build_group_name.to_string(),
            group_gid: None,
            receipt_action_idx_create_group: None,
        }),
    }
}
