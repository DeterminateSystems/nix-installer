/*! An executable or revertable step, possibly orchestrating sub-[`Action`]s using things like
    [`JoinSet`](tokio::task::JoinSet)s


[`Action`]s should be considered an 'atom' of change. Typically they are either a 'base' or
a 'composite' [`Action`].

Base actions are things like:

* [`CreateDirectory`](base::CreateDirectory)
* [`CreateFile`](base::CreateFile)
* [`CreateGroup`](base::CreateGroup)

Composite actions are things like:

* [`CreateNixTree`](common::CreateNixTree)
* [`ConfigureShellProfile`](common::ConfigureShellProfile)

During their `plan` phase, [`Planner`](crate::planner::Planner)s call an [`Action`]s `plan` function, which may accept any
arguments. For example, several 'composite' actions accept a [`CommonSettings`](crate::settings::CommonSettings). Later, the
[`InstallPlan`](crate::InstallPlan) will call [`try_execute`](StatefulAction::try_execute) on the [`StatefulAction`].

You can manually plan, execute, then revert an [`Action`] like so:

```rust,no_run
# async fn wrapper() {
use nix_installer::action::base::CreateDirectory;
let mut action = CreateDirectory::plan("/nix", None, None, 0o0755, true).await.unwrap();
action.try_execute().await.unwrap();
action.try_revert().await.unwrap();
# }
```

A general guidance for what determines how fine-grained an [`Action`] should be is the unit of
reversion. The [`ConfigureInitService`](common::ConfigureInitService) action is a good
example of this, it takes several steps, such as running `systemd-tmpfiles`, and calling
`systemctl link` on some systemd units.

Where possible, tasks which could break during execution should be broken up, as uninstalling/installing
step detection is determined by the wrapping [`StatefulAction`]. If an [`Action`] is a 'composite'
its sub-[`Action`]s can be reverted piece-by-piece. So breaking up actions into faillable units is
ideal.

A custom [`Action`] can be created then used in a custom [`Planner`](crate::planner::Planner):

```rust,no_run
use std::{error::Error, collections::HashMap};
use tracing::{Span, span};
use nix_installer::{
    InstallPlan,
    settings::{CommonSettings, InstallSettingsError},
    planner::{Planner, PlannerError},
    action::{Action, ActionError, StatefulAction, ActionDescription},
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct MyAction {}


impl MyAction {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        Ok(Self {}.into())
    }
}


#[async_trait::async_trait]
#[typetag::serde(name = "my_action")]
impl Action for MyAction {
    fn action_tag() -> nix_installer::action::ActionTag {
        "my_action".into()
    }
    fn tracing_synopsis(&self) -> String {
        "My action".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "my_action",
            // Tracing fields here ...
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        // Execute steps ...
        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
         vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        // Revert steps...
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MyPlanner {
    pub common: CommonSettings,
}


#[async_trait::async_trait]
#[typetag::serde(name = "my-planner")]
impl Planner for MyPlanner {
    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            common: CommonSettings::default().await?,
        })
    }

    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError> {
        Ok(vec![
            // ...
            MyAction::plan()
                    .await
                    .map_err(PlannerError::Action)?.boxed(),
        ])
    }

    fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        let Self { common } = self;
        let mut map = std::collections::HashMap::default();

        map.extend(common.settings()?.into_iter());

        Ok(map)
    }

    async fn configured_settings(
        &self,
    ) -> Result<HashMap<String, serde_json::Value>, PlannerError> {
        let default = Self::default().await?.settings()?;
        let configured = self.settings()?;

        let mut settings: HashMap<String, serde_json::Value> = HashMap::new();
        for (key, value) in configured.iter() {
            if default.get(key) != Some(value) {
                settings.insert(key.clone(), value.clone());
            }
        }

        Ok(settings)
    }

    #[cfg(feature = "diagnostics")]
    async fn diagnostic_data(&self) -> Result<nix_installer::diagnostics::DiagnosticData, PlannerError> {
        Ok(nix_installer::diagnostics::DiagnosticData::new(
            self.common.diagnostic_attribution.clone(),
            self.common.diagnostic_endpoint.clone(),
            self.typetag_name().into(),
            self.configured_settings()
                .await?
                .into_keys()
                .collect::<Vec<_>>(),
            self.common.ssl_cert_file.clone(),
        )?)
    }

    async fn platform_check(&self) -> Result<(), PlannerError> {
        use target_lexicon::OperatingSystem;
        match target_lexicon::OperatingSystem::host() {
            OperatingSystem::MacOSX { .. } | OperatingSystem::Darwin => Ok(()),
            host_os => Err(PlannerError::IncompatibleOperatingSystem {
                planner: self.typetag_name(),
                host_os,
            }),
        }
    }
}

# async fn custom_planner_install() -> color_eyre::Result<()> {
let planner = MyPlanner::default().await?;
let mut plan = InstallPlan::plan(planner).await?;
match plan.install(None).await {
    Ok(()) => tracing::info!("Done"),
    Err(e) => {
        match e.source() {
            Some(source) => tracing::error!("{e}: {}", source),
            None => tracing::error!("{e}"),
        };
        plan.uninstall(None).await?;
    },
};

#    Ok(())
# }
```

*/

pub mod base;
pub mod common;
pub mod linux;
pub mod macos;
mod stateful;

pub use stateful::{ActionState, StatefulAction};
use std::{error::Error, process::Output};
use tokio::task::JoinError;
use tracing::Span;

use crate::{error::HasExpectedErrors, settings::UrlOrPathError, CertificateError};

/// An action which can be reverted or completed, with an action state
///
/// This trait interacts with [`StatefulAction`] which does the [`ActionState`] manipulation and provides some tracing facilities.
///
/// Instead of calling [`execute`][Action::execute] or [`revert`][Action::revert], you should prefer [`try_execute`][StatefulAction::try_execute] and [`try_revert`][StatefulAction::try_revert]
#[async_trait::async_trait]
#[typetag::serde(tag = "action")]
pub trait Action: Send + Sync + std::fmt::Debug + dyn_clone::DynClone {
    fn action_tag() -> ActionTag
    where
        Self: Sized;
    /// A synopsis of the action for tracing purposes
    fn tracing_synopsis(&self) -> String;
    /// A tracing span suitable for the action
    ///
    /// It should be [`tracing::Level::DEBUG`] and contain the same name as the [`typetag::serde`] entry.
    ///
    /// It may contain any fields, and will be attached in the [`StatefulAction::try_execute`] and [`StatefulAction::try_revert`] functions.
    ///
    /// See [`tracing::Span`] for more details.
    fn tracing_span(&self) -> Span;
    /// A description of what this action would do during execution
    ///
    /// If this action calls sub-[`Action`]s, care should be taken to use [`StatefulAction::describe_execute`] on those actions, not [`execute_description`][Action::execute_description].
    ///
    /// This is called by [`InstallPlan::describe_install`](crate::InstallPlan::describe_install) through [`StatefulAction::describe_execute`] which will skip output if the action is completed.
    fn execute_description(&self) -> Vec<ActionDescription>;
    /// A description of what this action would do during revert
    ///
    /// If this action calls sub-[`Action`]s, care should be taken to use [`StatefulAction::describe_revert`] on those actions, not [`revert_description`][Action::revert_description].
    ///
    /// This is called by [`InstallPlan::describe_uninstall`](crate::InstallPlan::describe_uninstall) through [`StatefulAction::describe_revert`] which will skip output if the action is completed.
    fn revert_description(&self) -> Vec<ActionDescription>;
    /// Perform any execution steps
    ///
    /// If this action calls sub-[`Action`]s, care should be taken to call [`try_execute`][StatefulAction::try_execute], not [`execute`][Action::execute], so that [`ActionState`] is handled correctly and tracing is done.
    ///
    /// This is called by [`InstallPlan::install`](crate::InstallPlan::install) through [`StatefulAction::try_execute`] which handles tracing as well as if the action needs to execute based on its `action_state`.
    async fn execute(&mut self) -> Result<(), ActionError>;
    /// Perform any revert steps
    ///
    /// If this action calls sub-[`Action`]s, care should be taken to call [`try_revert`][StatefulAction::try_revert], not [`revert`][Action::revert], so that [`ActionState`] is handled correctly and tracing is done.
    ///
    /// This is called by [`InstallPlan::uninstall`](crate::InstallPlan::uninstall) through [`StatefulAction::try_revert`] which handles tracing as well as if the action needs to revert based on its `action_state`.
    async fn revert(&mut self) -> Result<(), ActionError>;

    fn stateful(self) -> StatefulAction<Self>
    where
        Self: Sized,
    {
        StatefulAction {
            action: self,
            state: ActionState::Uncompleted,
        }
    }

    fn error(kind: impl Into<ActionErrorKind>) -> ActionError
    where
        Self: Sized,
    {
        ActionError::new(Self::action_tag(), kind)
    }

    // They should also have an `async fn plan(args...) -> Result<StatefulAction<Self>, ActionError>;`
}

dyn_clone::clone_trait_object!(Action);

/**
A description of an [`Action`], intended for humans to review
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ActionDescription {
    pub description: String,
    pub explanation: Vec<String>,
}

impl ActionDescription {
    pub fn new(description: String, explanation: Vec<String>) -> Self {
        Self {
            description,
            explanation,
        }
    }
}

/// A 'tag' name an action has that corresponds to the one we serialize in [`typetag]`
pub struct ActionTag(&'static str);

impl std::fmt::Display for ActionTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::fmt::Debug for ActionTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl From<&'static str> for ActionTag {
    fn from(value: &'static str) -> Self {
        Self(value)
    }
}

#[derive(Debug)]
pub struct ActionError {
    action_tag: ActionTag,
    kind: ActionErrorKind,
}

impl ActionError {
    pub fn new(action_tag: ActionTag, kind: impl Into<ActionErrorKind>) -> Self {
        Self {
            action_tag,
            kind: kind.into(),
        }
    }

    pub fn kind(&self) -> &ActionErrorKind {
        &self.kind
    }

    pub fn action_tag(&self) -> &ActionTag {
        &self.action_tag
    }

    #[cfg(feature = "diagnostics")]
    pub fn diagnostic(&self) -> String {
        use crate::diagnostics::ErrorDiagnostic;
        self.kind.diagnostic()
    }
}

impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Action `{}` errored", self.action_tag))
    }
}

impl std::error::Error for ActionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.kind)
    }
}

impl From<ActionError> for ActionErrorKind {
    fn from(value: ActionError) -> Self {
        Self::Child(Box::new(value))
    }
}

/// An error occurring during an action
#[non_exhaustive]
#[derive(thiserror::Error, Debug, strum::IntoStaticStr)]
pub enum ActionErrorKind {
    /// A custom error
    #[error(transparent)]
    Custom(Box<dyn std::error::Error + Send + Sync>),
    /// An error to do with certificates
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// A child error
    #[error(transparent)]
    Child(Box<ActionError>),
    /// Several errors
    #[error("Multiple child errors\n\n{}", .0.iter().map(|err| {
        if let Some(source) = err.source() {
            format!("{err}\n{source}\n")
        } else {
            format!("{err}\n") 
        }
    }).collect::<Vec<_>>().join("\n"))]
    MultipleChildren(Vec<ActionError>),
    /// Several errors
    #[error("Multiple errors\n\n{}", .0.iter().map(|err| {
        if let Some(source) = err.source() {
            format!("{err}\n{source}")
        } else {
            format!("{err}\n")
        }
    }).collect::<Vec<_>>().join("\n"))]
    Multiple(Vec<ActionErrorKind>),
    #[error("Determinate Nix planned, but this installer is not equipped to install it.")]
    DeterminateNixUnavailable,
    /// The path already exists with different content that expected
    #[error(
        "`{0}` exists with different content than planned, consider removing it with `rm {0}`"
    )]
    DifferentContent(std::path::PathBuf),
    /// The file already exists
    #[error("`{0}` already exists, consider removing it with `rm {0}`")]
    FileExists(std::path::PathBuf),
    /// The directory already exists
    #[error("`{0}` already exists, consider removing it with `rm -r {0}`")]
    DirExists(std::path::PathBuf),
    /// The symlink already exists
    #[error("`{0}` already exists, consider removing it with `rm {0}`")]
    SymlinkExists(std::path::PathBuf),
    #[error("`{0}` exists with a different uid ({1}) than planned ({2}), consider updating it with `chown {2} {0}` (you may need to do this recursively with the `-R` flag)")]
    PathUserMismatch(std::path::PathBuf, u32, u32),
    #[error("`{0}` exists with a different gid ({1}) than planned ({2}), consider updating it with `chgrp {2} {0}` (you may need to do this recursively with the `-R` flag)")]
    PathGroupMismatch(std::path::PathBuf, u32, u32),
    #[error("`{0}` exists with a different mode ({existing_mode:o}) than planned ({planned_mode:o}), consider updating it with `chmod {planned_mode:o} {0}` (you may need to do this recursively with the `-R` flag)",
        existing_mode = .1 & 0o777,
        planned_mode = .2 & 0o777)]
    PathModeMismatch(std::path::PathBuf, u32, u32),
    #[error("Path `{0}` exists, but is not a file, consider removing it with `rm {0}`")]
    PathWasNotFile(std::path::PathBuf),
    #[error("Path `{0}` exists, but is not a directory, consider removing it with `rm {0}`")]
    PathWasNotDirectory(std::path::PathBuf),
    #[error("Getting metadata for {0}`")]
    GettingMetadata(std::path::PathBuf, #[source] std::io::Error),
    #[error("Creating directory `{0}`")]
    CreateDirectory(std::path::PathBuf, #[source] std::io::Error),
    #[error("Symlinking from `{0}` to `{1}`")]
    Symlink(
        std::path::PathBuf,
        std::path::PathBuf,
        #[source] std::io::Error,
    ),
    #[error("Getting filesystem metadata for `{0}` on `{1}`")]
    GetMetadata(std::path::PathBuf, #[source] std::io::Error),
    #[error("Set mode `{0:#o}` on `{1}`")]
    SetPermissions(u32, std::path::PathBuf, #[source] std::io::Error),
    #[error("Remove file `{0}`")]
    Remove(std::path::PathBuf, #[source] std::io::Error),
    #[error("Copying file `{0}` to `{1}`")]
    Copy(
        std::path::PathBuf,
        std::path::PathBuf,
        #[source] std::io::Error,
    ),
    #[error("Rename `{0}` to `{1}`")]
    Rename(
        std::path::PathBuf,
        std::path::PathBuf,
        #[source] std::io::Error,
    ),
    #[error("Canonicalizing `{0}`")]
    Canonicalize(std::path::PathBuf, #[source] std::io::Error),
    #[error("Read path `{0}`")]
    Read(std::path::PathBuf, #[source] std::io::Error),
    #[error("Reading directory `{0}`")]
    ReadDir(std::path::PathBuf, #[source] std::io::Error),
    #[error("Reading symbolic link `{0}`")]
    ReadSymlink(std::path::PathBuf, #[source] std::io::Error),
    #[error("Open path `{0}`")]
    Open(std::path::PathBuf, #[source] std::io::Error),
    #[error("Write path `{0}`")]
    Write(std::path::PathBuf, #[source] std::io::Error),
    #[error("Sync path `{0}`")]
    Sync(std::path::PathBuf, #[source] std::io::Error),
    #[error("Seek path `{0}`")]
    Seek(std::path::PathBuf, #[source] std::io::Error),
    #[error("Flushing `{0}`")]
    Flush(std::path::PathBuf, #[source] std::io::Error),
    #[error("Truncating `{0}`")]
    Truncate(std::path::PathBuf, #[source] std::io::Error),
    #[error("Getting uid for user `{0}`")]
    GettingUserId(String, #[source] nix::errno::Errno),
    #[error("User `{0}` existed but had a different uid ({1}) than planned ({2})")]
    UserUidMismatch(String, u32, u32),
    #[error("User `{0}` existed but had a different gid ({1}) than planned ({2})")]
    UserGidMismatch(String, u32, u32),
    #[error("Getting user `{0}`")]
    NoUser(String),
    #[error("Getting gid for group `{0}`")]
    GettingGroupId(String, #[source] nix::errno::Errno),
    #[error("Group `{0}` existed but had a different gid ({1}) than planned ({2})")]
    GroupGidMismatch(String, u32, u32),
    #[error("Getting group `{0}`")]
    NoGroup(String),
    #[error("Chowning path `{0}`")]
    Chown(std::path::PathBuf, #[source] nix::errno::Errno),
    #[error("Glob globbing error")]
    GlobGlobError(
        #[from]
        #[source]
        glob::GlobError,
    ),
    #[error("Glob pattern error")]
    GlobPatternError(
        #[from]
        #[source]
        glob::PatternError,
    ),
    /// Failed to execute command
    #[error("Failed to execute command `{command}`",
        command = .command,
    )]
    Command {
        #[cfg(feature = "diagnostics")]
        program: String,
        command: String,
        #[source]
        error: std::io::Error,
    },
    #[error(
        "Failed to execute command{maybe_status} `{command}`, stdout: {stdout}\nstderr: {stderr}\n",
        command = .command,
        stdout = String::from_utf8_lossy(&.output.stdout),
        stderr = String::from_utf8_lossy(&.output.stderr),
        maybe_status = if let Some(status) = .output.status.code() {
            format!(" with status {status}")
        } else {
            "".to_string()
        }
    )]
    CommandOutput {
        #[cfg(feature = "diagnostics")]
        program: String,
        command: String,
        output: Output,
    },
    #[error("Joining spawned async task")]
    Join(
        #[source]
        #[from]
        JoinError,
    ),
    #[error("String from UTF-8 error")]
    FromUtf8(
        #[source]
        #[from]
        std::string::FromUtf8Error,
    ),
    #[error("Path `{}` could not be converted to valid UTF-8 string", .0.display())]
    PathNoneString(std::path::PathBuf),
    /// A MacOS (Darwin) plist related error
    #[error(transparent)]
    Plist(#[from] plist::Error),
    #[error("Unexpected binary tarball contents found, the build result from `https://releases.nixos.org/?prefix=nix/` or `nix build nix#hydraJobs.binaryTarball.$SYSTEM` is expected")]
    MalformedBinaryTarball,
    #[error("Could not find `{0}` in PATH; This action only works on SteamOS, which should have this present in PATH.")]
    MissingSteamosBinary(String),
    #[error(
        "Could not find a supported command to create users in PATH; please install `useradd` or `adduser`"
    )]
    MissingUserCreationCommand,
    #[error("Could not find a supported command to create groups in PATH; please install `groupadd` or `addgroup`")]
    MissingGroupCreationCommand,
    #[error("Could not find a supported command to add users to groups in PATH; please install `gpasswd` or `addgroup`")]
    MissingAddUserToGroupCommand,
    #[error(
        "Could not find a supported command to delete users in PATH; please install `userdel` or `deluser`"
    )]
    MissingUserDeletionCommand,
    #[error("Could not find a supported command to delete groups in PATH; please install `groupdel` or `delgroup`")]
    MissingGroupDeletionCommand,
    #[error("Could not find a supported command to remove users from groups in PATH; please install `gpasswd` or `deluser`")]
    MissingRemoveUserFromGroupCommand,
    #[error("\
        Could not detect systemd; you may be able to get up and running without systemd with `nix-installer install linux --init none`.\n\
        See https://github.com/DeterminateSystems/nix-installer#without-systemd-linux-only for documentation on usage and drawbacks.\
        ")]
    SystemdMissing,
    #[error("`{command}` failed, message: {message}")]
    DiskUtilInfoError { command: String, message: String },
    #[error(transparent)]
    UrlOrPathError(#[from] UrlOrPathError),
    #[error("Request error")]
    Reqwest(
        #[from]
        #[source]
        reqwest::Error,
    ),
    #[error("Unknown url scheme")]
    UnknownUrlScheme,
}

impl ActionErrorKind {
    pub fn command(command: &tokio::process::Command, error: std::io::Error) -> Self {
        Self::Command {
            #[cfg(feature = "diagnostics")]
            program: command.as_std().get_program().to_string_lossy().into(),
            command: format!("{:?}", command.as_std()),
            error,
        }
    }
    pub fn command_output(command: &tokio::process::Command, output: std::process::Output) -> Self {
        Self::CommandOutput {
            #[cfg(feature = "diagnostics")]
            program: command.as_std().get_program().to_string_lossy().into(),
            command: format!("{:?}", command.as_std()),
            output,
        }
    }
}

impl HasExpectedErrors for ActionErrorKind {
    fn expected<'a>(&'a self) -> Option<Box<dyn std::error::Error + 'a>> {
        match self {
            Self::PathUserMismatch(_, _, _)
            | Self::PathGroupMismatch(_, _, _)
            | Self::PathModeMismatch(_, _, _) => Some(Box::new(self)),
            Self::SystemdMissing => Some(Box::new(self)),
            _ => None,
        }
    }
}

#[cfg(feature = "diagnostics")]
impl crate::diagnostics::ErrorDiagnostic for ActionErrorKind {
    fn diagnostic(&self) -> String {
        let static_str: &'static str = (self).into();
        let context = match self {
            Self::Child(child) => vec![child.diagnostic()],
            Self::MultipleChildren(children) => {
                children.iter().map(|child| child.diagnostic()).collect()
            },
            Self::Read(path, _)
            | Self::Open(path, _)
            | Self::Write(path, _)
            | Self::Flush(path, _)
            | Self::SetPermissions(_, path, _)
            | Self::GettingMetadata(path, _)
            | Self::CreateDirectory(path, _)
            | Self::PathWasNotFile(path)
            | Self::Remove(path, _) => {
                vec![path.to_string_lossy().to_string()]
            },
            Self::Rename(first_path, second_path, _)
            | Self::Copy(first_path, second_path, _)
            | Self::Symlink(first_path, second_path, _) => {
                vec![
                    first_path.to_string_lossy().to_string(),
                    second_path.to_string_lossy().to_string(),
                ]
            },
            Self::NoGroup(name) | Self::NoUser(name) => {
                vec![name.clone()]
            },
            Self::Command {
                program,
                command: _,
                error: _,
            }
            | Self::CommandOutput {
                program,
                command: _,
                output: _,
            } => {
                vec![program.clone()]
            },
            _ => vec![],
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
