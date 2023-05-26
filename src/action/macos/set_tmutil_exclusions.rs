use tracing::{span, Span};

use crate::action::base::CreateDirectory;
use crate::action::{
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};

/**
Set a time machine exclusion on several paths.

Note, this cannot be used on Volumes easily:

```bash,no_run
% sudo tmutil addexclusion -v "Nix Store"
tmutil: addexclusion requires Full Disk Access privileges.
To allow this operation, select Full Disk Access in the Privacy
tab of the Security & Privacy preference pane, and add Terminal
to the list of applications which are allowed Full Disk Access.
% sudo tmutil addexclusion /nix
/nix: The operation couldn’t be completed. Invalid argument
```

 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct SetTmutilExclusions {
    set_tmutil_exclusions: Vec<StatefulAction<SetTmutilExclusion>>,
}

impl SetTmutilExclusions {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(paths: Vec<PathBuf>) -> Result<StatefulAction<Self>, ActionError> {
        /* Testing with `sudo tmutil addexclusion -p /nix` and  `sudo tmutil addexclusion -v "Nix Store"` on DetSys's Macs
           yielded this error:

           ```
            tmutil: addexclusion requires Full Disk Access privileges.
            To allow this operation, select Full Disk Access in the Privacy
            tab of the Security & Privacy preference pane, and add Terminal
            to the list of applications which are allowed Full Disk Access.
            ```

            So we do these subdirectories instead.
        */
        let set_tmutil_exclusions = Vec::new();
        for path in paths {
            set_tmutil_exclusion = SetTmutilExclusion::plan(path).await.map_err(Self::error)?;
            set_tmutil_exclusions.push(set_tmutil_exclusion);
        }

        Ok(Self {
            set_tmutil_exclusions,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "set_tmutil_exclusions")]
impl Action for SetTmutilExclusions {
    fn action_tag() -> ActionTag {
        ActionTag("set_tmutil_exclusions")
    }
    fn tracing_synopsis(&self) -> String {
        String::from("Configure Time Machine exclusions")
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "set_tmutil_exclusions",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let Self { create_directories } = &self;

        let mut create_directory_descriptions = Vec::new();
        for create_directory in create_directories {
            if let Some(val) = create_directory.describe_execute().iter().next() {
                create_directory_descriptions.push(val.description.clone())
            }
        }
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            create_directory_descriptions,
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        // Just do sequential since parallelizing this will have little benefit
        for create_directory in self.create_directories.iter_mut() {
            create_directory.try_execute().await.map_err(Self::error)?;
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Remove the directory tree in `/nix`"),
            vec![
                format!(
                    "Nix and the Nix daemon require a Nix Store, which will be stored at `/nix`"
                ),
                format!(
                    "Removes: {}",
                    PATHS
                        .iter()
                        .rev()
                        .map(|v| format!("`{v}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let mut errors = vec![];
        // Just do sequential since parallelizing this will have little benefit
        for create_directory in self.create_directories.iter_mut().rev() {
            if let Err(err) = create_directory.try_revert().await {
                errors.push(err);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else if errors.len() == 1 {
            Err(errors
                .into_iter()
                .next()
                .expect("Expected 1 len Vec to have at least 1 item"))
        } else {
            Err(Self::error(ActionErrorKind::MultipleChildren(errors)))
        }
    }
}
