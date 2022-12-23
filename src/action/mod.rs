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
reversion. The [`ConfigureNixDaemonService`](linux::ConfigureNixDaemonService) action is a good
example of this,it takes several steps, such as running `systemd-tmpfiles`, and calling
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
    planner::{Planner, PlannerError, linux::SteamDeck},
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
            common: CommonSettings::default()?,
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
pub mod darwin;
pub mod linux;
mod stateful;

pub use stateful::{ActionState, StatefulAction};
use std::error::Error;
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
#[derive(thiserror::Error, Debug)]
pub enum ActionError {
    /// A custom error
    #[error(transparent)]
    Custom(Box<dyn std::error::Error + Send + Sync>),
    /// A child error
    #[error(transparent)]
    Child(#[from] Box<ActionError>),
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
    #[error("Path exists `{0}`")]
    Exists(std::path::PathBuf),
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
    #[error("Getting uid for user `{0}`")]
    UserId(String, #[source] nix::errno::Errno),
    #[error("Getting user `{0}`")]
    NoUser(String),
    #[error("Getting gid for group `{0}`")]
    GroupId(String, #[source] nix::errno::Errno),
    #[error("Getting group `{0}`")]
    NoGroup(String),
    #[error("Chowning path `{0}`")]
    Chown(std::path::PathBuf, #[source] nix::errno::Errno),
    /// Failed to execute command
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
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
}

impl HasExpectedErrors for ActionError {
    fn expected(&self) -> Option<Box<dyn std::error::Error>> {
        None
    }
}
