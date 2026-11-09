use std::{path::PathBuf, process::ExitCode, time::SystemTime};

use crate::{
    action::{Action, ActionState, StatefulAction},
    cli::{ensure_root, interaction::PromptChoice},
    plan::RECEIPT_LOCATION,
    InstallPlan,
};
use clap::{ArgAction, Parser};
use color_eyre::eyre::WrapErr;
use eyre::ContextCompat as _;
use owo_colors::OwoColorize;

use crate::cli::CommandExecute;

pub(crate) const PHASE1_RECEIPT_LOCATION: &str = "/nix/uninstall-phase1.json";
pub(crate) const PHASE2_RECEIPT_LOCATION: &str = "/nix/uninstall-phase2.json";

/// Split an existing receipt into two phases, one that cleans up the Nix store (phase 2), and
/// one that does everything else (phase 1).
///
/// This will produce two modified receipts -- a phase 1 receipt and a phase 2 receipt. If you
/// run `/nix/nix-installer uninstall /nix/uninstall-phase1.json` (the default phase 1
/// location), it will clean up everything but the Nix store and allow you to reinstall with a
/// newer version. If you run `/nix/nix-installer uninstall /nix/uninstall-phase2.json`, then it
/// will complete the uninstall by cleaning up the Nix store.
#[derive(Debug, Parser)]
pub struct SplitReceipt {
    #[clap(
        long,
        env = "NIX_INSTALLER_NO_CONFIRM",
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub no_confirm: bool,
    #[clap(default_value = RECEIPT_LOCATION)]
    pub receipt: PathBuf,
    #[clap(long, default_value = PHASE1_RECEIPT_LOCATION)]
    pub phase1_output: PathBuf,
    #[clap(long, default_value = PHASE2_RECEIPT_LOCATION)]
    pub phase2_output: PathBuf,
    // NOTE(cole-h): an escape hatch in case we somehow run into a case where the "receipt is
    // valid and we can actually parse it into structs" step does the wrong thing; hidden so
    // that users aren't tempted to use it themselves, but we can suggest it as a break-glass
    // measure
    #[clap(long, hide = true)]
    pub force_naive_json_method: bool,
}

#[async_trait::async_trait]
impl CommandExecute for SplitReceipt {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute<T>(self, _feedback: T) -> eyre::Result<ExitCode>
    where
        T: crate::feedback::Feedback,
    {
        ensure_root()?;

        let timestamp_millis = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis();

        let original_receipt_location = PathBuf::from(RECEIPT_LOCATION);
        let backed_up_receipt_location = original_receipt_location
            .with_file_name(format!(".original-receipt.{timestamp_millis}.json"));

        let brief_summary = format!("\n\
               This will split your existing receipt at {receipt} into two phases (phase 1: {phase1}, phase 2: {phase2}) \
               for uninstallation purposes, and move the existing receipt to a backup location at {backup_location} afterwards.\n\
               Phase 1 will clean up everything {except} for the root of the Nix store.\n\
               Phase 2 will then clean up the root of the Nix store and all its contents.\n\
               If you are wanting to install with a newer version of nix-installer, \
               you do not need to run phase 2 of the uninstallation.\n\
               If you want a clean uninstallation, you should run phase 2 after phase 1.\
               ",
           receipt = self.receipt.display().bold(),
           phase1 = self.phase1_output.display().bold(),
           phase2 = self.phase2_output.display().bold(),
           backup_location = backed_up_receipt_location.display().bold(),
           except = "except".italic(),
        );

        if !self.no_confirm {
            loop {
                match crate::cli::interaction::prompt(&brief_summary, PromptChoice::Yes, true)
                    .await?
                {
                    PromptChoice::Yes => break,
                    PromptChoice::No => {
                        crate::cli::interaction::clean_exit_with_message(
                            "Okay, didn't do anything! Bye!",
                        )
                        .await
                    },
                    PromptChoice::Explain => (),
                }
            }
        } else {
            tracing::info!("{}", brief_summary);
        }

        let install_receipt_string = tokio::fs::read_to_string(&self.receipt)
            .await
            .wrap_err("Reading receipt")?;

        if self.force_naive_json_method {
            two_phased_cannot_parse_receipt_perfectly(&self, &install_receipt_string).await?;
        } else {
            let maybe_compatible_plan =
                serde_json::from_str::<InstallPlan>(&install_receipt_string)
                    .ok()
                    .and_then(|plan| {
                        if plan.check_compatible().is_ok() {
                            Some(plan)
                        } else {
                            None
                        }
                    });
            match maybe_compatible_plan {
                Some(plan) => {
                    two_phased_can_parse_receipt_perfectly(&self, plan).await?;
                },
                None => {
                    two_phased_cannot_parse_receipt_perfectly(&self, &install_receipt_string)
                        .await?;
                },
            }
        }

        tokio::fs::rename(original_receipt_location, &backed_up_receipt_location).await?;
        tracing::info!(
            "Backed up original, untouched receipt to {}",
            backed_up_receipt_location.display()
        );

        println!(
            "\
            {success}\n\
            ",
            success = format!(
                "Phase 1 and 2 uninstall receipts successfully written:\n\
                Phase 1: {phase1}\n\
                Phase 2: {phase2}\n\
                You can now uninstall starting with:\n\
                /nix/nix-installer uninstall {phase1}",
                phase1 = self.phase1_output.display(),
                phase2 = self.phase2_output.display()
            )
            .green()
            .bold(),
        );

        Ok(ExitCode::SUCCESS)
    }
}

/// If the receipt can be parsed by this version of the installer, then we can use the actual
/// types as they will have the same fields.
async fn two_phased_can_parse_receipt_perfectly(
    uninstall_args: &SplitReceipt,
    plan: InstallPlan,
) -> eyre::Result<()> {
    tracing::debug!("Using the 'can actually parse receipt perfectly' method to split the receipt");

    let mut phase1_plan = plan;
    let mut phase2_plan = InstallPlan {
        version: phase1_plan.version.clone(),
        actions: Vec::new(),
        planner: phase1_plan.planner.clone(),
    };

    for action in phase1_plan.actions.iter_mut() {
        let inner_typetag_name = action.inner_typetag_name();
        match inner_typetag_name {
            action_tag if action_tag == crate::action::common::ProvisionNix::action_tag().0 => {
                let action_unjson =
                    roundtrip_to_extract_type::<crate::action::common::ProvisionNix>(action)?;

                tracing::debug!(
                    "Marking provision_nix as skipped so we don't undo it until phase 2"
                );

                {
                    let action_unjson = action_unjson.clone();
                    phase2_plan.actions.push(action_unjson.boxed());
                }

                // NOTE(cole-h): it's OK to skip the entire ProvisionNix thing here, since we
                // know its only job is to setup /nix and all that stuff (since that's all it
                // does in this version)
                {
                    let mut action_unjson = action_unjson;
                    action_unjson.state = ActionState::Skipped;
                    let _ = std::mem::replace(action, action_unjson.boxed());
                }
            },
            action_tag if action_tag == crate::action::base::CreateDirectory::action_tag().0 => {
                let action_unjson =
                    roundtrip_to_extract_type::<crate::action::base::CreateDirectory>(action)?;

                // NOTE(cole-h): we check if it stars with /nix, in case we start creating more
                // directories in the "toplevel" actions
                let path = &action_unjson.action.path;
                if path.starts_with("/nix") {
                    tracing::debug!(
                        "Marking create_directory for {path} as skipped so we don't undo it until phase 2", path = path.display()
                    );

                    {
                        let action_unjson = action_unjson.clone();
                        phase2_plan.actions.push(action_unjson.boxed());
                    }

                    {
                        let mut action_unjson = action_unjson;
                        action_unjson.state = ActionState::Skipped;
                        let _ = std::mem::replace(action, action_unjson.boxed());
                    }
                }
            },
            action_tag if action_tag == crate::action::macos::CreateNixVolume::action_tag().0 => {
                let action_unjson =
                    roundtrip_to_extract_type::<crate::action::macos::CreateNixVolume>(action)?;

                tracing::debug!("Marking create_volume, encrypt_volume (if it happened), unmount_volume as skipped so we don't undo it until phase 2");

                {
                    let action_unjson = action_unjson.clone();
                    phase2_plan
                        .actions
                        .push(action_unjson.action.create_volume.boxed());
                    phase2_plan
                        .actions
                        .push(action_unjson.action.unmount_volume.boxed());
                    if let Some(encrypt_volume) = action_unjson.action.encrypt_volume {
                        phase2_plan.actions.push(encrypt_volume.boxed());
                    }
                }

                {
                    let mut action_unjson = action_unjson;
                    action_unjson.action.create_volume.state = ActionState::Skipped;
                    if let Some(action) = action_unjson.action.encrypt_volume.as_mut() {
                        action.state = ActionState::Skipped;
                    };
                    action_unjson.action.unmount_volume.state = ActionState::Skipped;
                    let _ = std::mem::replace(action, action_unjson.boxed());
                }
            },
            action_tag
                if action_tag
                    == crate::action::macos::CreateDeterminateNixVolume::action_tag().0 =>
            {
                let action_unjson = roundtrip_to_extract_type::<
                    crate::action::macos::CreateDeterminateNixVolume,
                >(action)?;

                tracing::debug!("Marking create_volume, encrypt_volume, unmount_volume as skipped so we don't undo it until phase 2");

                {
                    let action_unjson = action_unjson.clone();
                    phase2_plan
                        .actions
                        .push(action_unjson.action.create_volume.boxed());
                    phase2_plan
                        .actions
                        .push(action_unjson.action.unmount_volume.boxed());
                    phase2_plan
                        .actions
                        .push(action_unjson.action.encrypt_volume.boxed());
                }

                {
                    let mut action_unjson = action_unjson;
                    action_unjson.action.create_volume.state = ActionState::Skipped;
                    action_unjson.action.encrypt_volume.state = ActionState::Skipped;
                    action_unjson.action.unmount_volume.state = ActionState::Skipped;
                    let _ = std::mem::replace(action, action_unjson.boxed());
                }
            },
            action_tag if action_tag == crate::action::common::ConfigureNix::action_tag().0 => {
                let mut action_unjson =
                    roundtrip_to_extract_type::<crate::action::common::ConfigureNix>(action)?;

                if let Some(custom_nix_config) =
                    skip_custom_nix_config_until_phase2(&mut action_unjson.action)
                {
                    phase2_plan.actions.push(custom_nix_config.boxed());
                    let _ = std::mem::replace(action, action_unjson.boxed());
                }
            },
            _ => {},
        }
    }

    crate::plan::write_receipt(&phase1_plan, &uninstall_args.phase1_output).await?;
    crate::plan::write_receipt(&phase2_plan, &uninstall_args.phase2_output).await?;

    Ok(())
}

/// If the receipt cannot be parsed or is not compatible with this version of the installer, we
/// fall back to naive JSON poking. Since the structure is version-specific, we have to be
/// careful that we account for this.
async fn two_phased_cannot_parse_receipt_perfectly(
    uninstall_args: &SplitReceipt,
    receipt_str: &str,
) -> eyre::Result<()> {
    tracing::debug!("Using the 'cannot parse receipt perfectly' method to split the receipt");

    #[derive(Debug, serde::Deserialize, serde::Serialize)]
    struct OpaquePlan {
        version: semver::Version,
        actions: Vec<serde_json::Value>,
        planner: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        diagnostic_data: Option<serde_json::Value>,
    }

    let mut phase1_plan: OpaquePlan =
        serde_json::from_str(receipt_str).context("Receipt was not opaquely parseable")?;
    let mut phase2_plan = OpaquePlan {
        version: phase1_plan.version.clone(),
        actions: Vec::new(),
        planner: phase1_plan.planner.clone(),
        diagnostic_data: phase1_plan.diagnostic_data.clone(),
    };

    let receipt_version = &phase1_plan.version;
    let skipped_json_value = serde_json::to_value(ActionState::Skipped)
        .context("ActionState::Skipped should be trivially serializable to JSON")?;

    for entry in phase1_plan.actions.iter_mut() {
        let Some(entry) = entry.as_object_mut() else {
            return Err(eyre::eyre!("Receipt entry was not an object! {entry:?}"));
        };

        let action_obj = &mut entry["action"];

        let Some(action) = action_obj.as_object_mut() else {
            return Err(eyre::eyre!(
                "Receipt action was not an object! {action_obj:?}"
            ));
        };

        // NOTE(cole-h): Older versions of nix-installer stored the action name as `action`
        // (under an object) -- i.e. `{"action":{"action":"create_apfs_volume", ...}}`
        // (versions prior to 0.21.0). These older versions also didn't note the action name
        // in sub-actions, only "root" actions. Newer versions store it as `action_name` --
        // i.e. `{"action":{"action_name":"create_apfs_volume", ...}}` (versions after
        // 0.21.0).
        let action_name = if *receipt_version >= semver::Version::new(0, 21, 0) {
            action["action_name"]
                .as_str()
                .context("Action name was not a string!")?
        } else {
            action["action"]
                .as_str()
                .context("Action name was not a string!")?
        };

        match action_name {
            // ProvisionNix
            "provision_nix" => {
                tracing::debug!(
                    "Marking provision_nix as skipped so we don't undo it until phase 2"
                );

                {
                    phase2_plan.actions.push(action_obj["fetch_nix"].clone());
                    phase2_plan
                        .actions
                        .push(action_obj["create_nix_tree"].clone());
                    phase2_plan
                        .actions
                        .push(action_obj["move_unpacked_nix"].clone());
                }

                // NOTE(cole-h): it's _NOT_ OK to skip the entire provision_nix thing here,
                // since older versions of the receipt (0.0.1 at least) used to have other
                // things in this step like "create nix build group and users" and similar
                {
                    action_obj["fetch_nix"]["state"] = skipped_json_value.clone();
                    action_obj["create_nix_tree"]["state"] = skipped_json_value.clone();
                    action_obj["move_unpacked_nix"]["state"] = skipped_json_value.clone();
                }
            },
            // CreateDirectory; Linux-only
            "create_directory" => {
                // NOTE(cole-h): we check if it stars with /nix, in case we created more
                // directories in the "toplevel" actions in the past
                let path = action_obj["path"]
                    .as_str()
                    .context("create_directory path field should be string!")?;
                if path.starts_with("/nix") {
                    tracing::debug!(
                            "Marking create_directory for {path} as skipped so we don't undo it until phase 2"
                        );

                    {
                        phase2_plan.actions.push(action_obj.clone());
                    }

                    {
                        entry["state"] = skipped_json_value.clone();
                    }
                }
            },
            s if (
                // CreateNixVolume on >= 0.28.0; macOS-only
                s == "create_nix_volume" && *receipt_version >= semver::Version::new(0, 28, 0)
            ) || (
                // CreateNixVolume on < 0.28.0; macOS-only
                s == "create_apfs_volume" && *receipt_version < semver::Version::new(0, 28, 0)
            ) =>
            {
                tracing::debug!("Marking create_volume, encrypt_volume (if it happened), unmount_volume as skipped so we don't undo it until phase 2");

                {
                    phase2_plan
                        .actions
                        .push(action_obj["create_volume"].clone());
                    if !action_obj["encrypt_volume"].is_null() {
                        phase2_plan
                            .actions
                            .push(action_obj["encrypt_volume"].clone());
                    }
                    phase2_plan
                        .actions
                        .push(action_obj["unmount_volume"].clone());
                }

                {
                    action_obj["create_volume"]["state"] = skipped_json_value.clone();
                    if !action_obj["encrypt_volume"].is_null() {
                        action_obj["encrypt_volume"]["state"] = skipped_json_value.clone();
                    }
                    action_obj["unmount_volume"]["state"] = skipped_json_value.clone();
                }
            },
            // CreateDeterminateNixVolume; macOS-only
            "create_determinate_nix_volume" => {
                tracing::debug!("Marking create_volume, encrypt_volume, unmount_volume as skipped so we don't undo it until phase 2");

                {
                    phase2_plan
                        .actions
                        .push(action_obj["create_volume"].clone());
                    phase2_plan
                        .actions
                        .push(action_obj["encrypt_volume"].clone());
                    // NOTE(cole-h): this action will ~always be at the "Progress" phase because we
                    // expect it to fail, and once it fails, it exits early and doesn't set the
                    // completed status
                    phase2_plan
                        .actions
                        .push(action_obj["unmount_volume"].clone());
                }

                {
                    action_obj["create_volume"]["state"] = skipped_json_value.clone();
                    action_obj["encrypt_volume"]["state"] = skipped_json_value.clone();
                    action_obj["unmount_volume"]["state"] = skipped_json_value.clone();
                }
            },
            // ConfigureNix; receipts from before `nix.custom.conf` existed won't have the
            // nested action, in which case this does nothing
            "configure_nix" => {
                if let Some(custom_nix_config) =
                    skip_custom_nix_config_until_phase2_json(action_obj, &skipped_json_value)
                {
                    phase2_plan.actions.push(custom_nix_config);
                }
            },
            _s => {},
        }
    }
    crate::plan::write_receipt(&phase1_plan, &uninstall_args.phase1_output).await?;
    crate::plan::write_receipt(&phase2_plan, &uninstall_args.phase2_output).await?;

    Ok(())
}

/// `/etc/nix/nix.custom.conf` is where users are told to put their own Nix configuration, so
/// it must survive a phase 1 uninstall + reinstall (e.g. an upgrade via the macOS package).
/// Reverting it deletes the whole file, so defer that until phase 2 (a full uninstall).
///
/// Returns the deferred action (with its original state) for the phase 2 plan, if there was one.
fn skip_custom_nix_config_until_phase2(
    configure_nix: &mut crate::action::common::ConfigureNix,
) -> Option<StatefulAction<crate::action::base::CreateOrMergeNixConfig>> {
    let place_nix_configuration = configure_nix.place_nix_configuration.as_mut()?;
    let custom_nix_config = &mut place_nix_configuration
        .action
        .create_or_merge_custom_nix_config;

    tracing::debug!(
        "Marking create_or_merge_nix_config for {path} as skipped so we don't undo it until phase 2",
        path = custom_nix_config.action.path.display()
    );

    let phase2_action = custom_nix_config.clone();
    custom_nix_config.state = ActionState::Skipped;

    Some(phase2_action)
}

/// The naive JSON equivalent of [`skip_custom_nix_config_until_phase2`].
fn skip_custom_nix_config_until_phase2_json(
    configure_nix: &mut serde_json::Value,
    skipped_json_value: &serde_json::Value,
) -> Option<serde_json::Value> {
    // NOTE: use `pointer_mut` rather than indexing, since indexing a missing key on a mutable
    // `Value` inserts it
    let custom_nix_config = configure_nix
        .pointer_mut("/place_nix_configuration/action/create_or_merge_custom_nix_config")?
        .as_object_mut()?;

    tracing::debug!(
        "Marking create_or_merge_nix_config for {path} as skipped so we don't undo it until phase 2",
        path = custom_nix_config
            .get("action")
            .and_then(|action| action.get("path"))
            .and_then(|path| path.as_str())
            .unwrap_or("nix.custom.conf")
    );

    let phase2_action = serde_json::Value::Object(custom_nix_config.clone());
    custom_nix_config.insert("state".to_string(), skipped_json_value.clone());

    Some(phase2_action)
}

fn roundtrip_to_extract_type<T: serde::de::DeserializeOwned>(
    action: &StatefulAction<Box<dyn Action>>,
) -> eyre::Result<StatefulAction<T>> {
    let action_name = std::any::type_name::<T>();
    let action_json = serde_json::to_string(action).with_context(|| {
        format!("serde_json::to_string'ing {action_name} json to extract real type")
    })?;
    let action_unjson: StatefulAction<T> =
        serde_json::from_str(&action_json).with_context(|| {
            format!("serde_json::from_str'ing {action_name} json to extract real type")
        })?;

    Ok(action_unjson)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINUX: &str = include_str!("../../../tests/fixtures/linux/linux.json");
    const MACOS: &str = include_str!("../../../tests/fixtures/macos/macos.json");

    fn configure_nix_json(receipt: &str) -> serde_json::Value {
        let receipt: serde_json::Value = serde_json::from_str(receipt).unwrap();
        receipt["actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["action"]["action_name"] == "configure_nix")
            .expect("fixture should have a configure_nix action")["action"]
            .clone()
    }

    #[test]
    fn custom_nix_config_deferred_to_phase2() {
        for receipt in [LINUX, MACOS] {
            let mut configure_nix: crate::action::common::ConfigureNix =
                serde_json::from_value(configure_nix_json(receipt)).unwrap();

            let phase2_action = skip_custom_nix_config_until_phase2(&mut configure_nix)
                .expect("custom nix config should be deferred");

            assert_eq!(
                phase2_action.action.path,
                PathBuf::from("/etc/nix/nix.custom.conf")
            );
            assert_eq!(phase2_action.state, ActionState::Completed);

            let place_nix_configuration = configure_nix.place_nix_configuration.unwrap();
            assert_eq!(
                place_nix_configuration
                    .action
                    .create_or_merge_custom_nix_config
                    .state,
                ActionState::Skipped
            );
            // The rest of the configuration is still reverted in phase 1
            assert_eq!(place_nix_configuration.state, ActionState::Completed);
        }
    }

    #[test]
    fn custom_nix_config_deferred_to_phase2_json() {
        let skipped = serde_json::to_value(ActionState::Skipped).unwrap();

        for receipt in [LINUX, MACOS] {
            let mut configure_nix = configure_nix_json(receipt);

            let phase2_action =
                skip_custom_nix_config_until_phase2_json(&mut configure_nix, &skipped)
                    .expect("custom nix config should be deferred");

            assert_eq!(phase2_action["action"]["path"], "/etc/nix/nix.custom.conf");
            assert_eq!(phase2_action["state"], "Completed");
            assert_eq!(
                configure_nix["place_nix_configuration"]["action"]
                    ["create_or_merge_custom_nix_config"]["state"],
                skipped
            );
            assert_eq!(
                configure_nix["place_nix_configuration"]["state"],
                "Completed"
            );
        }
    }

    #[test]
    fn custom_nix_config_json_missing_is_untouched() {
        let skipped = serde_json::to_value(ActionState::Skipped).unwrap();

        // Older receipts predate `nix.custom.conf`
        let mut configure_nix = configure_nix_json(MACOS);
        configure_nix["place_nix_configuration"]["action"]
            .as_object_mut()
            .unwrap()
            .remove("create_or_merge_custom_nix_config");
        let expected = configure_nix.clone();

        assert!(skip_custom_nix_config_until_phase2_json(&mut configure_nix, &skipped).is_none());
        assert_eq!(configure_nix, expected);

        // ...or `place_nix_configuration` entirely
        let mut configure_nix = serde_json::json!({ "action_name": "configure_nix" });
        let expected = configure_nix.clone();

        assert!(skip_custom_nix_config_until_phase2_json(&mut configure_nix, &skipped).is_none());
        assert_eq!(configure_nix, expected);
    }
}
