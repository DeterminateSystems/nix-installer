use crate::action::base::{create_or_insert_into_file, CreateDirectory, CreateOrInsertIntoFile};
use crate::action::{
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};
use crate::planner::ShellProfileLocations;

use nix::unistd::User;
use std::path::{Path, PathBuf};
use tokio::task::JoinSet;
use tracing::{span, Instrument, Span};

const PROFILE_NIX_FILE_SHELL: &str = "/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh";
const PROFILE_NIX_FILE_FISH: &str = "/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.fish";

/**
Configure any detected shell profiles to include Nix support
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "configure_shell_profile")]
pub struct ConfigureShellProfile {
    locations: ShellProfileLocations,
    create_directories: Vec<StatefulAction<CreateDirectory>>,
    create_or_insert_into_files: Vec<StatefulAction<CreateOrInsertIntoFile>>,
}

impl ConfigureShellProfile {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        locations: ShellProfileLocations,
    ) -> Result<StatefulAction<Self>, ActionError> {
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

        for profile_target in locations.bash.iter().chain(locations.zsh.iter()) {
            let profile_target_path = Path::new(profile_target);
            if let Some(parent) = profile_target_path.parent() {
                // Some tools (eg `nix-darwin`) create symlinks to these files, don't write to them if that's the case.
                if !profile_target_path.is_symlink() {
                    if !parent.exists() {
                        create_directories.push(
                            CreateDirectory::plan(parent, None, None, 0o0755, false)
                                .await
                                .map_err(Self::error)?,
                        );
                    }

                    create_or_insert_files.push(
                        CreateOrInsertIntoFile::plan(
                            profile_target_path,
                            None,
                            None,
                            0o644,
                            shell_buf.to_string(),
                            create_or_insert_into_file::Position::Beginning,
                        )
                        .await
                        .map_err(Self::error)?,
                    );
                }
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

        for fish_prefix in &locations.fish.confd_prefixes {
            let fish_prefix_path = PathBuf::from(fish_prefix);

            if !fish_prefix_path.exists() {
                // If the prefix doesn't exist, don't create the `conf.d/nix.fish`
                continue;
            }

            let mut profile_target = fish_prefix_path;
            profile_target.push(locations.fish.confd_suffix.clone());

            // Some tools (eg `nix-darwin`) create symlinks to these files, don't write to them if that's the case.
            if !profile_target.is_symlink() {
                if let Some(conf_d) = profile_target.parent() {
                    create_directories.push(
                        CreateDirectory::plan(conf_d.to_path_buf(), None, None, 0o755, false)
                            .await?,
                    );
                }

                create_or_insert_files.push(
                    CreateOrInsertIntoFile::plan(
                        profile_target,
                        None,
                        None,
                        0o644,
                        fish_buf.to_string(),
                        create_or_insert_into_file::Position::Beginning,
                    )
                    .await?,
                );
            }
        }
        for fish_prefix in &locations.fish.vendor_confd_prefixes {
            let fish_prefix_path = PathBuf::from(fish_prefix);

            if !fish_prefix_path.exists() {
                // If the prefix doesn't exist, don't create the `conf.d/nix.fish`
                continue;
            }

            let mut profile_target = fish_prefix_path;
            profile_target.push(locations.fish.vendor_confd_suffix.clone());

            if let Some(conf_d) = profile_target.parent() {
                create_directories.push(
                    CreateDirectory::plan(conf_d.to_path_buf(), None, None, 0o755, false).await?,
                );
            }

            create_or_insert_files.push(
                CreateOrInsertIntoFile::plan(
                    profile_target,
                    None,
                    None,
                    0o644,
                    fish_buf.to_string(),
                    create_or_insert_into_file::Position::Beginning,
                )
                .await?,
            );
        }

        // If the `$GITHUB_PATH` environment exists, we're almost certainly running on Github
        // Actions, and almost certainly wants the relevant `$PATH` additions added.
        if let Ok(github_path) = std::env::var("GITHUB_PATH") {
            let mut buf = "/nix/var/nix/profiles/default/bin\n".to_string();
            // Actions runners operate as `runner` user by default
            if let Ok(Some(runner)) = User::from_name("runner") {
                #[cfg(target_os = "linux")]
                let path = format!("/home/{}/.nix-profile/bin\n", runner.name);
                #[cfg(target_os = "macos")]
                let path = format!("/Users/{}/.nix-profile/bin\n", runner.name);
                buf += &path;
            }
            create_or_insert_files.push(
                CreateOrInsertIntoFile::plan(
                    &github_path,
                    None,
                    None,
                    // We want the `nix-installer-action` to not error if it writes here.
                    // Prior to `v5` this was done in this crate, in `v5` and later, this is done in the action.
                    0o777,
                    buf,
                    create_or_insert_into_file::Position::End,
                )
                .await?,
            );
        }

        Ok(Self {
            locations,
            create_directories,
            create_or_insert_into_files: create_or_insert_files,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "configure_shell_profile")]
impl Action for ConfigureShellProfile {
    fn action_tag() -> ActionTag {
        ActionTag("configure_shell_profile")
    }
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
        for create_directory in &mut self.create_directories {
            create_directory.try_execute().await?;
        }

        let mut set = JoinSet::new();
        let mut errors = vec![];

        for (idx, create_or_insert_into_file) in
            self.create_or_insert_into_files.iter_mut().enumerate()
        {
            let span = tracing::Span::current().clone();
            let mut create_or_insert_into_file_clone = create_or_insert_into_file.clone();
            let _abort_handle = set.spawn(async move {
                create_or_insert_into_file_clone
                    .try_execute()
                    .instrument(span)
                    .await
                    .map_err(Self::error)?;
                Result::<_, ActionError>::Ok((idx, create_or_insert_into_file_clone))
            });
        }

        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok((idx, create_or_insert_into_file))) => {
                    self.create_or_insert_into_files[idx] = create_or_insert_into_file
                },
                Ok(Err(e)) => errors.push(e),
                Err(e) => return Err(Self::error(e))?,
            };
        }

        if !errors.is_empty() {
            if errors.len() == 1 {
                return Err(Self::error(errors.into_iter().next().unwrap()))?;
            } else {
                return Err(Self::error(ActionErrorKind::MultipleChildren(
                    errors.into_iter().collect(),
                )));
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
        let mut set = JoinSet::new();
        let mut errors = vec![];

        for (idx, create_or_insert_into_file) in
            self.create_or_insert_into_files.iter_mut().enumerate()
        {
            let mut create_or_insert_file_clone = create_or_insert_into_file.clone();
            let _abort_handle = set.spawn(async move {
                create_or_insert_file_clone.try_revert().await?;
                Result::<_, _>::Ok((idx, create_or_insert_file_clone))
            });
        }

        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok((idx, create_or_insert_into_file))) => {
                    self.create_or_insert_into_files[idx] = create_or_insert_into_file
                },
                Ok(Err(e)) => errors.push(e),
                // This is quite rare and generally a very bad sign.
                Err(e) => return Err(e).map_err(|e| Self::error(ActionErrorKind::from(e)))?,
            };
        }

        for create_directory in self.create_directories.iter_mut() {
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
