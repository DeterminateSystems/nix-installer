use std::{process::Output, time::SystemTime};

use tokio::process::Command;
use which::which;

#[non_exhaustive]
#[derive(thiserror::Error, Debug, strum::IntoStaticStr)]
pub enum SelfTestError {
    #[error("Shell `{shell}` failed self-test with command `{command}`, stderr:\n{}", String::from_utf8_lossy(&output.stderr))]
    ShellFailed {
        shell: Shell,
        command: String,
        output: Output,
    },
    /// Failed to execute command
    #[error("Failed to execute command `{command}`",
        command = .command,
    )]
    Command {
        shell: Shell,
        command: String,
        #[source]
        error: std::io::Error,
    },
    #[error(transparent)]
    SystemTime(#[from] std::time::SystemTimeError),
    #[error("command timed out")]
    TimedOut { shell: Shell, command: String },
}

#[cfg(feature = "diagnostics")]
impl crate::diagnostics::ErrorDiagnostic for SelfTestError {
    fn diagnostic(&self) -> String {
        let static_str: &'static str = (self).into();
        let context = match self {
            Self::ShellFailed { shell, .. } => vec![shell.to_string()],
            Self::Command { shell, .. } => vec![shell.to_string()],
            Self::SystemTime(_) => vec![],
            Self::TimedOut { shell, .. } => vec![shell.to_string()],
        };
        format!(
            "{}({})",
            static_str,
            context
                .iter()
                .map(|v| format!("\"{v}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Shell {
    Sh,
    Bash,
    Fish,
    Zsh,
}

impl std::fmt::Display for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.executable())
    }
}

impl Shell {
    pub fn all() -> &'static [Shell] {
        &[Shell::Sh, Shell::Bash, Shell::Fish, Shell::Zsh]
    }
    pub fn executable(&self) -> &'static str {
        match &self {
            Shell::Sh => "sh",
            Shell::Bash => "bash",
            Shell::Fish => "fish",
            Shell::Zsh => "zsh",
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn self_test(&self) -> Result<(), SelfTestError> {
        let executable = self.executable();
        let mut command = match &self {
            // On Mac, `bash -ic nix` won't work, but `bash -lc nix` will.
            Shell::Sh | Shell::Bash => {
                let mut command = Command::new(executable);
                command.arg("-lc");
                command
            },
            Shell::Zsh | Shell::Fish => {
                let mut command = Command::new(executable);
                command.arg("-ic");
                command
            },
        };

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        const SYSTEM: &str = "x86_64-linux";
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        const SYSTEM: &str = "aarch64-linux";
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        const SYSTEM: &str = "aarch64-darwin";

        let timestamp_millis = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis();

        command.arg(format!(
            r#"exec nix build --option substitute false --option post-build-hook '' --no-link --expr 'derivation {{ name = "self-test-{executable}-{timestamp_millis}"; system = "{SYSTEM}"; builder = "/bin/sh"; args = ["-c" "echo hello > \$out"]; }}'"#
        ));
        let command_str = format!("{:?}", command.as_std());

        tracing::debug!(
            command = command_str,
            "Testing Nix install via `{executable}`"
        );
        let output = command
            .stdin(std::process::Stdio::null())
            .env("NIX_REMOTE", "daemon")
            .kill_on_drop(true)
            .output();
        let output = tokio::time::timeout(std::time::Duration::from_secs(10), output)
            .await
            .map_err(|_| SelfTestError::TimedOut {
                shell: *self,
                command: command_str.clone(),
            })?
            .map_err(|error| SelfTestError::Command {
                shell: *self,
                command: command_str.clone(),
                error,
            })?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SelfTestError::ShellFailed {
                shell: *self,
                command: command_str,
                output,
            })
        }
    }

    #[tracing::instrument(skip_all)]
    pub fn discover() -> Vec<Shell> {
        let mut found_shells = vec![];
        for shell in Self::all() {
            if which(shell.executable()).is_ok() {
                tracing::debug!("Discovered `{shell}`");
                found_shells.push(*shell)
            }
        }
        found_shells
    }
}

#[tracing::instrument(skip_all)]
pub async fn self_test() -> Result<(), Vec<SelfTestError>> {
    let shells = Shell::discover();

    let mut failures = vec![];

    for shell in shells {
        match shell.self_test().await {
            Ok(()) => (),
            Err(err) => failures.push(err),
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}
