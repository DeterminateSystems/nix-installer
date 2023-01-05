use crate::action::base::{CreateDirectory, CreateOrInsertFile};
use crate::action::{Action, ActionDescription, ActionError, StatefulAction};

use nix::unistd::User;
use std::path::{Path, PathBuf};
use tokio::task::JoinSet;
use tracing::{span, Instrument, Span};

// Fish has different syntax than zsh/bash, treat it separate
const PROFILE_FISH_SUFFIX: &str = "conf.d/nix.fish";

/**
 Each of these are common values of $__fish_sysconf_dir,
under which Fish will look for a file named
[`PROFILE_FISH_SUFFIX`].
*/
const PROFILE_FISH_PREFIXES: &[&str] = &[
    "/etc/fish",              // standard
    "/usr/local/etc/fish",    // their installer .pkg for macOS
    "/opt/homebrew/etc/fish", // homebrew
    "/opt/local/etc/fish",    // macports
];
const PROFILE_TARGETS: &[&str] = &[
    "/etc/bashrc",
    "/etc/profile.d/nix.sh",
    "/etc/zshenv",
    "/etc/bash.bashrc",
    "/etc/zsh/zshenv",
];
const PROFILE_NIX_FILE_SHELL: &str = "/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh";
const PROFILE_NIX_FILE_FISH: &str = "/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.fish";

/**
Configure any detected shell profiles to include Nix support
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureShellProfile {
    create_directories: Vec<StatefulAction<CreateDirectory>>,
    create_or_insert_files: Vec<StatefulAction<CreateOrInsertFile>>,
}

impl ConfigureShellProfile {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        let mut create_or_insert_files = Vec::default();
        let mut create_directories = Vec::default();

        let shell_buf = format!(
            "\n\
            # Nix\n\
            if [ -e '{PROFILE_NIX_FILE_SHELL}' ]; then\n\
            {inde}. '{PROFILE_NIX_FILE_SHELL}'\n\
            fi\n\
            # End Nix\n
        \n",
            inde = "    ", // indent
        );

        for profile_target in PROFILE_TARGETS {
            let profile_target_path = Path::new(profile_target);
            if let Some(parent) = profile_target_path.parent() {
                if !parent.exists() {
                    tracing::trace!(
                        "Did not plan to edit `{profile_target}` as its parent folder does not exist."
                    );
                    continue;
                }
                create_or_insert_files.push(
                    CreateOrInsertFile::plan(
                        profile_target_path,
                        None,
                        None,
                        0o0755,
                        shell_buf.to_string(),
                    )
                    .await?,
                );
            }
        }

        let fish_buf = format!(
            "\n\
            # Nix\n\
            if test -e '{PROFILE_NIX_FILE_FISH}'\n\
            {inde}. '{PROFILE_NIX_FILE_FISH}'\n\
            end\n\
            # End Nix\n\
        \n",
            inde = "    ", // indent
        );

        for fish_prefix in PROFILE_FISH_PREFIXES {
            let fish_prefix_path = PathBuf::from(fish_prefix);

            if !fish_prefix_path.exists() {
                // If the prefix doesn't exist, don't create the `conf.d/nix.fish`
                continue;
            }

            let mut profile_target = fish_prefix_path;
            profile_target.push(PROFILE_FISH_SUFFIX);

            if let Some(conf_d) = profile_target.parent() {
                create_directories.push(
                    CreateDirectory::plan(conf_d.to_path_buf(), None, None, 0o0644, false).await?,
                );
            }

            create_or_insert_files.push(
                CreateOrInsertFile::plan(profile_target, None, None, 0o0755, fish_buf.to_string())
                    .await?,
            );
        }

        // If the `$GITHUB_PATH` environment exists, we're almost certainly running on Github
        // Actions, and almost certainly wants the relevant `$PATH` additions added.
        if let Ok(github_path) = std::env::var("GITHUB_PATH") {
            let mut buf = "/nix/var/nix/profiles/default/bin\n".to_string();
            // Actions runners operate as `runner` user by default
            if let Ok(Some(runner)) = User::from_name("runner") {
                buf += &format!(
                    "/nix/var/nix/profiles/per-user/{}/profile/bin\n",
                    runner.uid
                );
            }
            create_or_insert_files
                .push(CreateOrInsertFile::plan(&github_path, None, None, None, buf).await?)
        }

        Ok(Self {
            create_directories,
            create_or_insert_files,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "configure_shell_profile")]
impl Action for ConfigureShellProfile {
    fn tracing_synopsis(&self) -> String {
        "Configure the shell profiles".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "configure_shell_profile",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec!["Update shell profiles to import Nix".to_string()],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            create_or_insert_files,
            create_directories,
        } = self;

        for create_directory in create_directories {
            create_directory.try_execute().await?;
        }

        let mut set = JoinSet::new();
        let mut errors = Vec::default();

        for (idx, create_or_insert_file) in create_or_insert_files.iter().enumerate() {
            let span = tracing::Span::current().clone();
            let mut create_or_insert_file_clone = create_or_insert_file.clone();
            let _abort_handle = set.spawn(async move {
                create_or_insert_file_clone
                    .try_execute()
                    .instrument(span)
                    .await?;
                Result::<_, ActionError>::Ok((idx, create_or_insert_file_clone))
            });
        }

        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok((idx, create_or_insert_file))) => {
                    create_or_insert_files[idx] = create_or_insert_file
                },
                Ok(Err(e)) => errors.push(Box::new(e)),
                Err(e) => return Err(e.into()),
            };
        }

        if !errors.is_empty() {
            if errors.len() == 1 {
                return Err(errors.into_iter().next().unwrap().into());
            } else {
                return Err(ActionError::Children(errors));
            }
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            "Unconfigure the shell profiles".to_string(),
            vec!["Update shell profiles to no longer import Nix".to_string()],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self {
            create_directories,
            create_or_insert_files,
        } = self;

        let mut set = JoinSet::new();
        let mut errors: Vec<Box<ActionError>> = Vec::default();

        for (idx, create_or_insert_file) in create_or_insert_files.iter().enumerate() {
            let mut create_or_insert_file_clone = create_or_insert_file.clone();
            let _abort_handle = set.spawn(async move {
                create_or_insert_file_clone.try_revert().await?;
                Result::<_, _>::Ok((idx, create_or_insert_file_clone))
            });
        }

        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok((idx, create_or_insert_file))) => {
                    create_or_insert_files[idx] = create_or_insert_file
                },
                Ok(Err(e)) => errors.push(Box::new(e)),
                Err(e) => return Err(e.into()),
            };
        }

        for create_directory in create_directories {
            create_directory.try_revert().await?;
        }

        if !errors.is_empty() {
            if errors.len() == 1 {
                return Err(errors.into_iter().next().unwrap().into());
            } else {
                return Err(ActionError::Children(errors));
            }
        }

        Ok(())
    }
}
