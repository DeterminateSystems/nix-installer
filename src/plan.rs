use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::{
    action::{Action, ActionDescription, StatefulAction},
    planner::{BuiltinPlanner, Planner},
    NixInstallerError,
};
use owo_colors::OwoColorize;
use semver::{Version, VersionReq};
use tokio::sync::broadcast::Receiver;

pub const RECEIPT_LOCATION: &str = "/nix/receipt.json";

/**
A set of [`Action`]s, along with some metadata, which can be carried out to drive an install or
revert
*/
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct InstallPlan {
    pub(crate) version: Version,

    pub(crate) actions: Vec<StatefulAction<Box<dyn Action>>>,

    pub(crate) planner: Box<dyn Planner>,

    #[cfg(feature = "diagnostics")]
    pub(crate) diagnostic_data: Option<crate::diagnostics::DiagnosticData>,
}

impl InstallPlan {
    pub async fn default() -> Result<Self, NixInstallerError> {
        let planner = BuiltinPlanner::default().await?;

        #[cfg(feature = "diagnostics")]
        let diagnostic_data = Some(planner.diagnostic_data().await?);

        let planner = planner.boxed();
        let actions = planner.plan().await?;

        Ok(Self {
            planner,
            actions,
            version: current_version()?,
            #[cfg(feature = "diagnostics")]
            diagnostic_data,
        })
    }

    pub async fn plan<P>(planner: P) -> Result<Self, NixInstallerError>
    where
        P: Planner + 'static,
    {
        planner.platform_check().await?;

        #[cfg(feature = "diagnostics")]
        let diagnostic_data = Some(planner.diagnostic_data().await?);

        // Some Action `plan` calls may fail if we don't do these checks
        planner.pre_install_check().await?;

        let actions = planner.plan().await?;
        Ok(Self {
            planner: planner.boxed(),
            actions,
            version: current_version()?,
            #[cfg(feature = "diagnostics")]
            diagnostic_data,
        })
    }

    pub async fn pre_uninstall_check(&self) -> Result<(), NixInstallerError> {
        self.planner.platform_check().await?;
        self.planner.pre_uninstall_check().await?;
        Ok(())
    }

    pub async fn pre_install_check(&self) -> Result<(), NixInstallerError> {
        self.planner.platform_check().await?;
        self.planner.pre_install_check().await?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn describe_install(&self, explain: bool) -> Result<String, NixInstallerError> {
        let Self {
            planner,
            actions,
            version,
            ..
        } = self;

        let plan_settings = if explain {
            // List all settings when explaining
            planner.settings()?
        } else {
            // Otherwise, only list user-configured settings
            planner.configured_settings().await?
        };
        let mut plan_settings = plan_settings
            .into_iter()
            .map(|(k, v)| format!("* {k}: {v}", k = k.bold()))
            .collect::<Vec<_>>();
        // Stabilize output order
        plan_settings.sort();

        let buf = format!(
            "\
            Nix install plan (v{version})\n\
            Planner: {planner}{maybe_default_setting_note}\n\
            \n\
            {maybe_plan_settings}\
            Planned actions:\n\
            {actions}\n\
        ",
            planner = planner.typetag_name(),
            maybe_default_setting_note = if plan_settings.is_empty() {
                String::from(" (with default settings)")
            } else {
                String::new()
            },
            maybe_plan_settings = if plan_settings.is_empty() {
                String::new()
            } else {
                format!(
                    "\
                    Configured settings:\n\
                    {plan_settings}\n\
                    \n\
                ",
                    plan_settings = plan_settings.join("\n")
                )
            },
            actions = actions
                .iter()
                .flat_map(|v| v.describe_execute())
                .map(|desc| {
                    let ActionDescription {
                        description,
                        explanation,
                    } = desc;

                    let mut buf = String::default();
                    buf.push_str(&format!("* {description}"));
                    if explain {
                        for line in explanation {
                            buf.push_str(&format!("\n  {line}"));
                        }
                    }
                    buf
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
        Ok(buf)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn install(
        &mut self,
        cancel_channel: impl Into<Option<Receiver<()>>>,
    ) -> Result<(), NixInstallerError> {
        self.check_compatible()?;
        self.pre_install_check().await?;

        let Self { actions, .. } = self;
        let mut cancel_channel = cancel_channel.into();

        // This is **deliberately sequential**.
        // Actions which are parallelizable are represented by "group actions" like CreateUsers
        // The plan itself represents the concept of the sequence of stages.
        for action in actions {
            if let Some(ref mut cancel_channel) = cancel_channel {
                if cancel_channel.try_recv()
                    != Err(tokio::sync::broadcast::error::TryRecvError::Empty)
                {
                    if let Err(err) = self.write_receipt().await {
                        tracing::error!("Error saving receipt: {:?}", err);
                    }

                    #[cfg(feature = "diagnostics")]
                    if let Some(diagnostic_data) = &self.diagnostic_data {
                        diagnostic_data
                            .clone()
                            .send(
                                crate::diagnostics::DiagnosticAction::Install,
                                crate::diagnostics::DiagnosticStatus::Cancelled,
                            )
                            .await;
                    }

                    return Err(NixInstallerError::Cancelled);
                }
            }

            tracing::info!("Step: {}", action.tracing_synopsis());
            if let Err(err) = action.try_execute().await {
                if let Err(err) = self.write_receipt().await {
                    tracing::error!("Error saving receipt: {:?}", err);
                }
                let err = NixInstallerError::Action(err);
                #[cfg(feature = "diagnostics")]
                if let Some(diagnostic_data) = &self.diagnostic_data {
                    diagnostic_data
                        .clone()
                        .failure(&err)
                        .send(
                            crate::diagnostics::DiagnosticAction::Install,
                            crate::diagnostics::DiagnosticStatus::Failure,
                        )
                        .await;
                }

                return Err(err);
            }
        }

        self.write_receipt().await?;

        if let Err(err) = crate::self_test::self_test()
            .await
            .map_err(NixInstallerError::SelfTest)
        {
            #[cfg(feature = "diagnostics")]
            if let Some(diagnostic_data) = &self.diagnostic_data {
                diagnostic_data
                    .clone()
                    .failure(&err)
                    .send(
                        crate::diagnostics::DiagnosticAction::Install,
                        crate::diagnostics::DiagnosticStatus::Failure,
                    )
                    .await;
            }

            tracing::warn!("{err:?}")
        } else {
            #[cfg(feature = "diagnostics")]
            if let Some(diagnostic_data) = &self.diagnostic_data {
                diagnostic_data
                    .clone()
                    .send(
                        crate::diagnostics::DiagnosticAction::Install,
                        crate::diagnostics::DiagnosticStatus::Success,
                    )
                    .await;
            }
        }

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn describe_uninstall(&self, explain: bool) -> Result<String, NixInstallerError> {
        let Self {
            version,
            planner,
            actions,
            ..
        } = self;

        let plan_settings = if explain {
            // List all settings when explaining
            planner.settings()?
        } else {
            // Otherwise, only list user-configured settings
            planner.configured_settings().await?
        };
        let mut plan_settings = plan_settings
            .into_iter()
            .map(|(k, v)| format!("* {k}: {v}", k = k.bold()))
            .collect::<Vec<_>>();
        // Stabilize output order
        plan_settings.sort();

        let buf = format!(
            "\
            Nix uninstall plan (v{version})\n\
            \n\
            Planner: {planner}{maybe_default_setting_note}\n\
            \n\
            {maybe_plan_settings}\
            Planned actions:\n\
            {actions}\n\
        ",
            planner = planner.typetag_name(),
            maybe_default_setting_note = if plan_settings.is_empty() {
                String::from(" (with default settings)")
            } else {
                String::new()
            },
            maybe_plan_settings = if plan_settings.is_empty() {
                String::new()
            } else {
                format!(
                    "\
                Configured settings:\n\
                {plan_settings}\n\
                \n\
            ",
                    plan_settings = plan_settings.join("\n")
                )
            },
            actions = actions
                .iter()
                .rev()
                .flat_map(|v| v.describe_revert())
                .map(|desc| {
                    let ActionDescription {
                        description,
                        explanation,
                    } = desc;

                    let mut buf = String::default();
                    buf.push_str(&format!("* {description}"));
                    if explain {
                        for line in explanation {
                            buf.push_str(&format!("\n  {line}"));
                        }
                    }
                    buf
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
        Ok(buf)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn uninstall(
        &mut self,
        cancel_channel: impl Into<Option<Receiver<()>>>,
    ) -> Result<(), NixInstallerError> {
        self.check_compatible()?;
        self.pre_uninstall_check().await?;

        let Self { actions, .. } = self;
        let mut cancel_channel = cancel_channel.into();
        let mut errors = vec![];

        // This is **deliberately sequential**.
        // Actions which are parallelizable are represented by "group actions" like CreateUsers
        // The plan itself represents the concept of the sequence of stages.
        for action in actions.iter_mut().rev() {
            if let Some(ref mut cancel_channel) = cancel_channel {
                if cancel_channel.try_recv()
                    != Err(tokio::sync::broadcast::error::TryRecvError::Empty)
                {
                    if let Err(err) = self.write_receipt().await {
                        tracing::error!("Error saving receipt: {:?}", err);
                    }

                    #[cfg(feature = "diagnostics")]
                    if let Some(diagnostic_data) = &self.diagnostic_data {
                        diagnostic_data
                            .clone()
                            .send(
                                crate::diagnostics::DiagnosticAction::Uninstall,
                                crate::diagnostics::DiagnosticStatus::Cancelled,
                            )
                            .await;
                    }
                    return Err(NixInstallerError::Cancelled);
                }
            }

            tracing::info!("Revert: {}", action.tracing_synopsis());
            if let Err(errs) = action.try_revert().await {
                errors.push(errs);
            }
        }

        if errors.is_empty() {
            #[cfg(feature = "diagnostics")]
            if let Some(diagnostic_data) = &self.diagnostic_data {
                diagnostic_data
                    .clone()
                    .send(
                        crate::diagnostics::DiagnosticAction::Uninstall,
                        crate::diagnostics::DiagnosticStatus::Success,
                    )
                    .await;
            }

            Ok(())
        } else {
            let error = NixInstallerError::ActionRevert(errors);
            #[cfg(feature = "diagnostics")]
            if let Some(diagnostic_data) = &self.diagnostic_data {
                diagnostic_data
                    .clone()
                    .failure(&error)
                    .send(
                        crate::diagnostics::DiagnosticAction::Uninstall,
                        crate::diagnostics::DiagnosticStatus::Failure,
                    )
                    .await;
            }

            Err(error)
        }
    }

    pub fn check_compatible(&self) -> Result<(), NixInstallerError> {
        let self_version_string = self.version.to_string();
        let req = VersionReq::parse(&self_version_string)
            .map_err(|e| NixInstallerError::InvalidVersionRequirement(self_version_string, e))?;
        let nix_installer_version = current_version()?;
        if req.matches(&nix_installer_version) {
            Ok(())
        } else {
            Err(NixInstallerError::IncompatibleVersion {
                binary: nix_installer_version,
                plan: self.version.clone(),
            })
        }
    }

    pub(crate) async fn write_receipt(&self) -> Result<(), NixInstallerError> {
        let install_receipt_path = PathBuf::from(RECEIPT_LOCATION);
        write_receipt(self, &install_receipt_path).await?;

        Ok(())
    }
}

pub(crate) async fn write_receipt(
    plan: &impl serde::Serialize,
    install_receipt_path: &Path,
) -> Result<(), NixInstallerError> {
    let install_receipt_path_tmp = {
        let mut install_receipt_path_tmp = install_receipt_path.to_path_buf();
        install_receipt_path_tmp.set_extension("tmp");
        install_receipt_path_tmp
    };
    let self_json =
        serde_json::to_string_pretty(plan).map_err(NixInstallerError::SerializingReceipt)?;

    tokio::fs::create_dir_all("/nix")
        .await
        .map_err(|e| NixInstallerError::RecordingReceipt(PathBuf::from("/nix"), e))?;
    tokio::fs::write(&install_receipt_path_tmp, format!("{self_json}\n"))
        .await
        .map_err(|e| NixInstallerError::RecordingReceipt(install_receipt_path_tmp.clone(), e))?;
    tokio::fs::rename(&install_receipt_path_tmp, &install_receipt_path)
        .await
        .map_err(|e| NixInstallerError::RecordingReceipt(install_receipt_path.to_path_buf(), e))?;

    Ok(())
}

pub fn current_version() -> Result<Version, NixInstallerError> {
    let nix_installer_version_str = env!("CARGO_PKG_VERSION");
    Version::from_str(nix_installer_version_str).map_err(|e| {
        NixInstallerError::InvalidCurrentVersion(nix_installer_version_str.to_string(), e)
    })
}

#[cfg(test)]
mod test {
    use semver::Version;

    use crate::{planner::BuiltinPlanner, InstallPlan, NixInstallerError};

    #[tokio::test]
    async fn ensure_version_allows_compatible() -> Result<(), NixInstallerError> {
        let planner = BuiltinPlanner::default().await?;
        let good_version = Version::parse(env!("CARGO_PKG_VERSION"))?;
        let value = serde_json::json!({
            "planner": planner.boxed(),
            "version": good_version,
            "actions": [],
        });
        let maybe_plan: InstallPlan = serde_json::from_value(value)?;
        maybe_plan.check_compatible()?;
        Ok(())
    }

    #[tokio::test]
    async fn ensure_version_denies_incompatible() -> Result<(), NixInstallerError> {
        let planner = BuiltinPlanner::default().await?;
        let bad_version = Version::parse("9999999999999.9999999999.99999999")?;
        let value = serde_json::json!({
            "planner": planner.boxed(),
            "version": bad_version,
            "actions": [],
        });
        let maybe_plan: InstallPlan = serde_json::from_value(value)?;
        assert!(maybe_plan.check_compatible().is_err());
        Ok(())
    }
}
