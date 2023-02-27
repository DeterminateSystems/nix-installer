/*! An executable or revertable step, possibly orcestrating sub-[`Action`]s using things like
    [`JoinSet`](tokio::task::JoinSet)s


[`Action`]s should be considered an 'atom' of change. Typically they are either a 'base' or
a 'composite' [`Action`].

Base actions are things like:

* [`CreateDirectory`](base::CreateDirectory)
* [`CreateFile`](base::CreateFile)
* [`CreateUser`](base::CreateUser)

Composite actions are things like:

* [`CreateNixTree`](common::CreateNixTree)
* [`CreateUsersAndGroups`](common::CreateUsersAndGroups)

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

    #[cfg(feature = "diagnostics")]
    async fn diagnostic_data(&self) -> Result<nix_installer::diagnostics::DiagnosticData, PlannerError> {
        Ok(nix_installer::diagnostics::DiagnosticData::new(
            self.common.diagnostic_endpoint.clone(),
            self.typetag_name().into(),
            self.configured_settings().await?,
        ))
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

use crate::error::HasExpectedErrors;

/// An action which can be reverted or completed, with an action state
///
/// This trait interacts with [`StatefulAction`] which does the [`ActionState`] manipulation and provides some tracing facilities.
///
/// Instead of calling [`execute`][Action::execute] or [`revert`][Action::revert], you should prefer [`try_execute`][StatefulAction::try_execute] and [`try_revert`][StatefulAction::try_revert]
#[async_trait::async_trait]
#[typetag::serde(tag = "action")]
pub trait Action: Send + Sync + std::fmt::Debug + dyn_clone::DynClone {
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
    /// /// This is called by [`InstallPlan::uninstall`](crate::InstallPlan::uninstall) through [`StatefulAction::try_revert`] which handles tracing as well as if the action needs to revert based on its `action_state`.
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
    // They should also have an `async fn plan(args...) -> Result<StatefulAction<Self>, ActionError>;`
}

dyn_clone::clone_trait_object!(Action);

/**
A description of an [`Action`](crate::action::Action), intended for humans to review
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

/// An error occurring during an action
#[derive(thiserror::Error, Debug, strum::IntoStaticStr)]
pub enum ActionError {
    /// A custom error
    #[error(transparent)]
    Custom(Box<dyn std::error::Error + Send + Sync>),
    /// A child error
    #[error("Child action `{0}`")]
    Child(&'static str, Box<ActionError>),
    /// Several child errors
    #[error("Multiple errors: {}", .0.iter().map(|v| {
        if let Some(source) = v.source() {
            format!("{v} ({source})")
        } else {
            format!("{v}") 
        }
    }).collect::<Vec<_>>().join(" & "))]
    Children(Vec<Box<ActionError>>),
    /// The path already exists
    #[error(
        "`{0}` exists with different content than planned, consider removing it with `rm {0}`"
    )]
    Exists(std::path::PathBuf),
    #[error("`{0}` exists with a different uid ({1}) than planned ({2}), consider updating it with `chown {2} {0}`")]
    PathUserMismatch(std::path::PathBuf, u32, u32),
    #[error("`{0}` exists with a different gid ({1}) than planned ({2}), consider updating it with `chgrp {2} {0}`")]
    PathGroupMismatch(std::path::PathBuf, u32, u32),
    #[error("`{0}` exists with a different mode ({existing_mode:o}) than planned ({planned_mode:o}), consider updating it with `chmod {planned_mode:o} {0}`",
        existing_mode = .1 & 0o777,
        planned_mode = .2 & 0o777)]
    PathModeMismatch(std::path::PathBuf, u32, u32),
    #[error("`{0}` was not a file")]
    PathWasNotFile(std::path::PathBuf),
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
    #[error("Set mode `{0}` on `{1}`")]
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
    #[error("Read path `{0}`")]
    Read(std::path::PathBuf, #[source] std::io::Error),
    #[error("Open path `{0}`")]
    Open(std::path::PathBuf, #[source] std::io::Error),
    #[error("Write path `{0}`")]
    Write(std::path::PathBuf, #[source] std::io::Error),
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
    /// Failed to execute command
    #[error("Failed to execute command `{0}`")]
    Command(String, #[source] std::io::Error),
    #[error(
        "Failed to execute command{maybe_status} `{command}`, stdout: {stdout}\nstderr: {stderr}\n",
        command = .0,
        stdout = String::from_utf8_lossy(&.1.stdout),
        stderr = String::from_utf8_lossy(&.1.stderr),
        maybe_status = if let Some(status) = .1.status.code() {
            format!(" with status {status}")
        } else {
            "".to_string()
        }
    )]
    CommandOutput(String, Output),
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
    /// A MacOS (Darwin) plist related error
    #[error(transparent)]
    Plist(#[from] plist::Error),
}

impl HasExpectedErrors for ActionError {
    fn expected<'a>(&'a self) -> Option<Box<dyn std::error::Error + 'a>> {
        match self {
            Self::PathUserMismatch(_, _, _)
            | Self::PathGroupMismatch(_, _, _)
            | Self::PathModeMismatch(_, _, _) => Some(Box::new(self)),
            _ => None,
        }
    }
}

#[cfg(feature = "diagnostics")]
impl crate::diagnostics::ErrorDiagnostic for ActionError {
    fn diagnostic(&self) -> (String, Vec<String>) {
        let static_str: &'static str = (self).into();
        let context = match self {
            Self::Read(path, _)
            | Self::Open(path, _)
            | Self::Write(path, _)
            | Self::Flush(path, _)
            | Self::SetPermissions(_, path, _)
            | Self::GettingMetadata(path, _)
            | Self::CreateDirectory(path, _)
            | Self::PathWasNotFile(path) => {
                vec![path.to_str().unwrap_or("<not UTF-8>").to_string()]
            },
            Self::Rename(first_path, second_path, _)
            | Self::Copy(first_path, second_path, _)
            | Self::Symlink(first_path, second_path, _) => {
                vec![
                    first_path.to_str().unwrap_or("<not UTF-8>").to_string(),
                    second_path.to_str().unwrap_or("<not UTF-8>").to_string(),
                ]
            },
            _ => vec![],
        };
        return (static_str.to_string(), context);
    }
}
