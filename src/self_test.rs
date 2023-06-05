use std::process::Output;

use tokio::process::Command;
use which::which;

#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
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
        command: String,
        #[source]
        error: std::io::Error,
    },
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

    pub async fn self_test(&self) -> Result<(), SelfTestError> {
        let executable = self.executable();
        let mut command = match &self {
            Shell::Sh => {
                let mut command = Command::new(executable);
                command.arg("-lc");
                command
            },
            Shell::Zsh | Shell::Bash | Shell::Fish => {
                let mut command = Command::new(executable);
                command.arg("-ic");
                command
            },
        };

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        const SYSTEM: &str = "x86_64-linux";
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        const SYSTEM: &str = "aarch64-linux";
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        const SYSTEM: &str = "x86_64-darwin";
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        const SYSTEM: &str = "aarch64-darwin";

        command.arg(format!(
            r#"nix build --no-link --expr 'derivation {{ name = "self-test-{executable}"; system = "{SYSTEM}"; builder = "/bin/sh"; args = ["-c" "echo hello > \$out"]; }}'"#
        ));
        let command_str = format!("{:?}", command.as_std());

        tracing::debug!(
            command = command_str,
            "Testing Nix install via `{executable}`"
        );
        let output = command
            .output()
            .await
            .map_err(|error| SelfTestError::Command {
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

    pub fn discover() -> Vec<Shell> {
        let mut found_shells = vec![];
        for shell in Self::all() {
            if which(shell.executable()).is_ok() {
                tracing::trace!("Discovered `{shell}`");
                found_shells.push(*shell)
            }
        }
        found_shells
    }
}

pub async fn self_test() -> Result<(), SelfTestError> {
    let shells = Shell::discover();

    for shell in shells {
        shell.self_test().await?;
    }

    Ok(())
}
