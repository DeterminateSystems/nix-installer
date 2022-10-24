mod actions;
mod error;
mod os;
mod plan;
mod planner;
mod settings;

use std::{
    ffi::OsStr,
    fmt::Display,
    process::{ExitStatus, Output},
};

pub use error::HarmonicError;
pub use plan::InstallPlan;
pub use planner::Planner;
use serde::Serializer;
pub use settings::InstallSettings;

use tokio::process::Command;

#[tracing::instrument(skip_all, fields(command = %format!("{:?}", command.as_std())))]
async fn execute_command(command: &mut Command) -> Result<Output, std::io::Error> {
    tracing::trace!("Executing");
    let command_str = format!("{:?}", command.as_std());
    let output = command.output().await?;
    match output.status.success() {
        true => Ok(output),
        false => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Command `{command_str}` failed status, stderr:\n{}\n",
                String::from_utf8(output.stderr).unwrap_or_else(|_e| String::from("<Non-UTF-8>"))
            ),
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
