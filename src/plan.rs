use std::{path::PathBuf, str::FromStr};

use crate::{
    action::{Action, ActionDescription, StatefulAction},
    planner::{BuiltinPlanner, Planner},
    HarmonicError,
};
use owo_colors::OwoColorize;
use semver::{Version, VersionReq};
use serde::{de::Error, Deserialize, Deserializer};
use tokio::sync::broadcast::Receiver;

pub const RECEIPT_LOCATION: &str = "/nix/receipt.json";

/**
A set of [`Action`]s, along with some metadata, which can be carried out to drive an install or
revert
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct InstallPlan {
    #[serde(deserialize_with = "ensure_version")]
    pub(crate) version: Version,

    pub(crate) actions: Vec<StatefulAction<Box<dyn Action>>>,

    pub(crate) planner: Box<dyn Planner>,
}

impl InstallPlan {
    pub async fn default() -> Result<Self, HarmonicError> {
        let planner = BuiltinPlanner::default().await?.boxed();
        let actions = planner.plan().await?;

        Ok(Self {
            planner,
            actions,
            version: current_version()?,
        })
    }

    pub async fn plan<P>(planner: P) -> Result<Self, HarmonicError>
    where
        P: Planner + 'static,
    {
        let actions = planner.plan().await?;
        Ok(Self {
            planner: planner.boxed(),
            actions,
            version: current_version()?,
        })
    }
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn describe_install(&self, explain: bool) -> Result<String, HarmonicError> {
        let Self {
            planner,
            actions,
            version,
        } = self;
        let buf = format!(
            "\
            Nix install plan (v{version})\n\
            \n\
            Planner: {planner}\n\
            \n\
            Planner settings:\n\
            \n\
            {plan_settings}\n\
            \n\
            The following actions will be taken{maybe_explain}:\n\
            \n\
            {actions}\n\
        ",
            maybe_explain = if !explain {
                " (`--explain` for more context)"
            } else {
                ""
            },
            planner = planner.typetag_name(),
            plan_settings = planner
                .settings()?
                .into_iter()
                .map(|(k, v)| format!("* {k}: {v}", k = k.bold().white()))
                .collect::<Vec<_>>()
                .join("\n"),
            actions = actions
                .iter()
                .map(|v| v.describe_execute())
                .flatten()
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
    ) -> Result<(), HarmonicError> {
        let Self {
            version: _,
            actions,
            planner: _,
        } = self;
        let mut cancel_channel = cancel_channel.into();

        // This is **deliberately sequential**.
        // Actions which are parallelizable are represented by "group actions" like CreateUsers
        // The plan itself represents the concept of the sequence of stages.
        for action in actions {
            if let Some(ref mut cancel_channel) = cancel_channel {
                if cancel_channel.try_recv()
                    != Err(tokio::sync::broadcast::error::TryRecvError::Empty)
                {
                    if let Err(err) = write_receipt(self.clone()).await {
                        tracing::error!("Error saving receipt: {:?}", err);
                    }
                    return Err(HarmonicError::Cancelled);
                }
            }

            tracing::info!("Step: {}", action.tracing_synopsis());
            if let Err(err) = action.try_execute().await {
                if let Err(err) = write_receipt(self.clone()).await {
                    tracing::error!("Error saving receipt: {:?}", err);
                }
                return Err(HarmonicError::Action(err));
            }
        }

        write_receipt(self.clone()).await?;
        copy_self_to_nix_store()
            .await
            .map_err(|e| HarmonicError::CopyingSelf(e))?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn describe_uninstall(&self, explain: bool) -> Result<String, HarmonicError> {
        let Self {
            version: _,
            planner,
            actions,
        } = self;
        let buf = format!(
            "\
            Nix uninstall plan\n\
            \n\
            Planner: {planner}\n\
            \n\
            Planner settings:\n\
            \n\
            {plan_settings}\n\
            \n\
            The following actions will be taken{maybe_explain}:\n\
            \n\
            {actions}\n\
        ",
            maybe_explain = if !explain {
                " (`--explain` for more context)"
            } else {
                ""
            },
            planner = planner.typetag_name(),
            plan_settings = planner
                .settings()?
                .into_iter()
                .map(|(k, v)| format!("* {k}: {v}", k = k.bold().white()))
                .collect::<Vec<_>>()
                .join("\n"),
            actions = actions
                .iter()
                .rev()
                .map(|v| v.describe_revert())
                .flatten()
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
    ) -> Result<(), HarmonicError> {
        let Self {
            version: _,
            actions,
            planner: _,
        } = self;
        let mut cancel_channel = cancel_channel.into();

        // This is **deliberately sequential**.
        // Actions which are parallelizable are represented by "group actions" like CreateUsers
        // The plan itself represents the concept of the sequence of stages.
        for action in actions.iter_mut().rev() {
            if let Some(ref mut cancel_channel) = cancel_channel {
                if cancel_channel.try_recv()
                    != Err(tokio::sync::broadcast::error::TryRecvError::Empty)
                {
                    if let Err(err) = write_receipt(self.clone()).await {
                        tracing::error!("Error saving receipt: {:?}", err);
                    }
                    return Err(HarmonicError::Cancelled);
                }
            }

            tracing::info!("Step: {}", action.tracing_synopsis());
            if let Err(err) = action.try_revert().await {
                if let Err(err) = write_receipt(self.clone()).await {
                    tracing::error!("Error saving receipt: {:?}", err);
                }
                return Err(HarmonicError::Action(err));
            }
        }

        Ok(())
    }
}

async fn write_receipt(plan: InstallPlan) -> Result<(), HarmonicError> {
    tokio::fs::create_dir_all("/nix")
        .await
        .map_err(|e| HarmonicError::RecordingReceipt(PathBuf::from("/nix"), e))?;
    let install_receipt_path = PathBuf::from(RECEIPT_LOCATION);
    let self_json =
        serde_json::to_string_pretty(&plan).map_err(HarmonicError::SerializingReceipt)?;
    tokio::fs::write(&install_receipt_path, self_json)
        .await
        .map_err(|e| HarmonicError::RecordingReceipt(install_receipt_path, e))?;
    Result::<(), HarmonicError>::Ok(())
}

fn current_version() -> Result<Version, semver::Error> {
    let harmonic_version_str = env!("CARGO_PKG_VERSION");
    Version::from_str(harmonic_version_str)
}

fn ensure_version<'de, D: Deserializer<'de>>(d: D) -> Result<Version, D::Error> {
    let plan_version = Version::deserialize(d)?;
    let req = VersionReq::parse(&plan_version.to_string()).map_err(|_e| {
        D::Error::custom(&format!(
            "Could not parse version `{plan_version}` as a version requirement, please report this",
        ))
    })?;
    let harmonic_version = current_version().map_err(|_e| {
        D::Error::custom(&format!(
            "Could not parse Harmonic's version `{}` as a valid version according to Semantic Versioning, therefore the plan version ({plan_version}) compatibility cannot be checked", env!("CARGO_PKG_VERSION")
        ))
    })?;
    if req.matches(&harmonic_version) {
        Ok(plan_version)
    } else {
        Err(D::Error::custom(&format!(
            "This version of Harmonic ({harmonic_version}) is not compatible with this plan's version ({plan_version}), please use a compatible version (according to Semantic Versioning)",
        )))
    }
}

#[tracing::instrument(level = "debug")]
async fn copy_self_to_nix_store() -> Result<(), std::io::Error> {
    let path = std::env::current_exe()?;
    tokio::fs::copy(path, "/nix/harmonic").await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use semver::Version;

    use crate::{planner::BuiltinPlanner, HarmonicError, InstallPlan};

    #[tokio::test]
    async fn ensure_version_allows_compatible() -> Result<(), HarmonicError> {
        let planner = BuiltinPlanner::default().await?;
        let good_version = Version::parse(env!("CARGO_PKG_VERSION"))?;
        let value = serde_json::json!({
            "planner": planner.boxed(),
            "version": good_version,
            "actions": [],
        });
        let maybe_plan: Result<InstallPlan, serde_json::Error> = serde_json::from_value(value);
        maybe_plan.unwrap();
        Ok(())
    }

    #[tokio::test]
    async fn ensure_version_denies_incompatible() -> Result<(), HarmonicError> {
        let planner = BuiltinPlanner::default().await?;
        let bad_version = Version::parse("9999999999999.9999999999.99999999")?;
        let value = serde_json::json!({
            "planner": planner.boxed(),
            "version": bad_version,
            "actions": [],
        });
        let maybe_plan: Result<InstallPlan, serde_json::Error> = serde_json::from_value(value);
        assert!(maybe_plan.is_err());
        let err = maybe_plan.unwrap_err();
        assert!(err.is_data());
        Ok(())
    }
}
