mod actions;
mod error;
mod plan;
mod planner;
mod settings;

use std::{ffi::OsStr, fmt::Display, process::ExitStatus};

pub use error::HarmonicError;
pub use plan::InstallPlan;
pub use planner::Planner;
use serde::Serializer;
pub use settings::InstallSettings;

use tokio::process::Command;

#[tracing::instrument(skip_all, fields(command = %format!("{:?}", command.as_std())))]
async fn execute_command(command: &mut Command) -> Result<ExitStatus, std::io::Error> {
    tracing::trace!("Executing");
    let command_str = format!("{:?}", command.as_std());
    let status = command.status().await?;
    match status.success() {
        true => Ok(status),
        false => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Command `{command_str}` failed status"),
        )),
    }
}

#[tracing::instrument(skip_all, fields(
    k = %k.as_ref().to_string_lossy(),
    v = %v.as_ref().to_string_lossy(),
))]
fn set_env(k: impl AsRef<OsStr>, v: impl AsRef<OsStr>) {
    tracing::trace!("Setting env");
    std::env::set_var(k.as_ref(), v.as_ref());
}

fn serialize_error_to_display<E, S>(err: &E, ser: S) -> Result<S::Ok, S::Error>
where
    E: Display,
    S: Serializer,
{
    ser.serialize_str(&format!("{err:#}"))
}
