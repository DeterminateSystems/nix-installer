/*! An executable or revertable step, possibly orcestrating sub-[`Action`]s using things like
    [`JoinSet`](tokio::task::JoinSet)s

A custom [`Action`] can be created then used in a custom [`Planner`](crate::planner::Planner):

```rust,no_run
use std::{error::Error, collections::HashMap};
use harmonic::{
    InstallPlan,
    settings::{CommonSettings, InstallSettingsError},
    planner::{Planner, PlannerError, specific::SteamDeck},
    action::{Action, ActionState, ActionDescription},
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct MyAction {
    // ...
    action_state: ActionState,
}


#[async_trait::async_trait]
#[typetag::serde(name = "my_action")]
impl Action for MyAction {
    fn tracing_synopsis(&self) -> String {
        "My action".to_string()
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(skip_all, fields(
        // Tracing fields...
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Execute steps ...
        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
         vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(skip_all, fields(
        // Tracing fields...
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Revert steps...
        Ok(())
    }

    fn action_state(&self) -> ActionState {
        self.action_state
    }

    fn set_action_state(&mut self, action_state: ActionState) {
        self.action_state = action_state;
    }
}

impl MyAction {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            action_state: ActionState::Uncompleted,
        })
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

    async fn plan(&self) -> Result<Vec<Box<dyn Action>>, PlannerError> {
        Ok(vec![
            // ...
            Box::new(
                MyAction::plan()
                    .await
                    .map_err(PlannerError::Action)?,
            ),
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

use serde::{Deserialize, Serialize};

/// An action which can be reverted or completed, with an action state
///
/// This trait interacts with [`ActionImplementation`] which does the [`ActionState`] manipulation and provides some tracing facilities.
///
/// Instead of calling [`execute`][Action::execute] or [`revert`][Action::revert], you should prefer [`try_execute`][ActionImplementation::try_execute] and [`try_revert`][ActionImplementation::try_revert]
#[async_trait::async_trait]
#[typetag::serde(tag = "action")]
pub trait Action: Send + Sync + std::fmt::Debug + dyn_clone::DynClone {
    /// A synopsis of the action for tracing purposes
    fn tracing_synopsis(&self) -> String;
    /// A description of what this action would do during execution
    ///
    /// If this action calls sub-[`Action`]s, care should be taken to use [`ActionImplementation::describe_execute`] on those actions, not [`execute_description`][Action::execute_description].
    ///
    /// This is called by [`InstallPlan::describe_install`](crate::InstallPlan::describe_install) through [`ActionImplementation::describe_execute`] which will skip output if the action is completed.
    fn execute_description(&self) -> Vec<ActionDescription>;
    /// A description of what this action would do during revert
    ///
    /// If this action calls sub-[`Action`]s, care should be taken to use [`ActionImplementation::describe_revert`] on those actions, not [`revert_description`][Action::revert_description].
    ///
    /// This is called by [`InstallPlan::describe_uninstall`](crate::InstallPlan::describe_uninstall) through [`ActionImplementation::describe_revert`] which will skip output if the action is completed.
    fn revert_description(&self) -> Vec<ActionDescription>;
    /// Perform any execution steps
    ///
    /// If this action calls sub-[`Action`]s, care should be taken to call [`try_execute`][ActionImplementation::try_execute], not [`execute`][Action::execute], so that [`ActionState`] is handled correctly and tracing is done.
    ///
    /// This is called by [`InstallPlan::install`](crate::InstallPlan::install) through [`ActionImplementation::try_execute`] which handles tracing as well as if the action needs to execute based on its `action_state`.
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    /// Perform any revert steps
    ///
    /// If this action calls sub-[`Action`]s, care should be taken to call [`try_revert`][ActionImplementation::try_revert], not [`revert`][Action::revert], so that [`ActionState`] is handled correctly and tracing is done.
    ///
    /// /// This is called by [`InstallPlan::uninstall`](crate::InstallPlan::uninstall) through [`ActionImplementation::try_revert`] which handles tracing as well as if the action needs to revert based on its `action_state`.
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    /// Get the `action_state` of the action
    fn action_state(&self) -> ActionState;
    /// Set the `action_state` of the action
    fn set_action_state(&mut self, new_state: ActionState);

    // They should also have an `async fn plan(args...) -> Result<ActionState<Self>, Box<dyn std::error::Error + Send + Sync>>;`
}

/// The main wrapper around [`Action`], handling [`ActionState`] and tracing.
#[async_trait::async_trait]
pub trait ActionImplementation: Action {
    /// A description of what this action would do during execution
    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state() == ActionState::Completed {
            return vec![];
        }
        return self.execute_description();
    }
    /// A description of what this action would do during revert
    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state() == ActionState::Uncompleted {
            return vec![];
        }
        return self.revert_description();
    }
    /// Perform any execution steps
    ///
    /// You should prefer this ([`try_execute`][ActionImplementation::try_execute]) over [`execute`][Action::execute] as it handles [`ActionState`] and does tracing
    async fn try_execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.action_state() == ActionState::Completed {
            tracing::trace!("Completed: (Already done) {}", self.tracing_synopsis());
            return Ok(());
        }
        self.set_action_state(ActionState::Progress);
        tracing::debug!("Executing: {}", self.tracing_synopsis());
        self.execute().await?;
        self.set_action_state(ActionState::Completed);
        tracing::debug!("Completed: {}", self.tracing_synopsis());
        Ok(())
    }
    /// Perform any revert steps
    ///
    /// You should prefer this ([`try_revert`][ActionImplementation::try_revert]) over [`revert`][Action::revert] as it handles [`ActionState`] and does tracing
    async fn try_revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.action_state() == ActionState::Uncompleted {
            tracing::trace!("Reverted: (Already done) {}", self.tracing_synopsis());
            return Ok(());
        }
        self.set_action_state(ActionState::Progress);
        tracing::debug!("Reverting: {}", self.tracing_synopsis());
        self.revert().await?;
        tracing::debug!("Reverted: {}", self.tracing_synopsis());
        self.set_action_state(ActionState::Uncompleted);
        Ok(())
    }
}

impl ActionImplementation for dyn Action {}

impl<A> ActionImplementation for A where A: Action {}

dyn_clone::clone_trait_object!(Action);

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Copy)]
pub enum ActionState {
    Completed,
    // Only applicable to meta-actions that start multiple sub-actions.
    Progress,
    Uncompleted,
}

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
