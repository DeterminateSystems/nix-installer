use crate::action::base::{create_or_insert_into_file, CreateDirectory, CreateOrInsertIntoFile};
use crate::action::{Action, ActionDescription, ActionError, ActionTag, StatefulAction};

use nix::unistd::User;
use std::path::{Path, PathBuf};
use target_lexicon::OperatingSystem;
use tokio::task::JoinSet;
use tracing::{span, Instrument, Span};

// Fish has different syntax than zsh/bash, treat it separate
const PROFILE_FISH_VENDOR_CONFD_SUFFIX: &str = "vendor_conf.d/nix.fish";
/**
 Each of these are common values of $__fish_vendor_confdir,
under which Fish will look for a file named
[`PROFILE_FISH_CONFD_SUFFIX`].

More info: https://fishshell.com/docs/3.3/index.html#configuration-files
*/
const PROFILE_FISH_VENDOR_CONFD_PREFIXES: &[&str] = &["/usr/share/fish/", "/usr/local/share/fish/"];

const PROFILE_FISH_CONFD_SUFFIX: &str = "conf.d/nix.fish";
/**
 Each of these are common values of $__fish_sysconf_dir,
under which Fish will look for a file named
[`PROFILE_FISH_CONFD_PREFIXES`].
*/
const PROFILE_FISH_CONFD_PREFIXES: &[&str] = &[
    "/etc/fish",              // standard
    "/usr/local/etc/fish",    // their installer .pkg for macOS
    "/opt/homebrew/etc/fish", // homebrew
    "/opt/local/etc/fish",    // macports
];

const PROFILE_TARGETS: &[&str] = &[
    "/etc/bashrc",
    "/etc/profile.d/nix.sh",
    "/etc/bash.bashrc",
    // https://zsh.sourceforge.io/Intro/intro_3.html
    "/etc/zshrc",
    "/etc/zsh/zshrc",
];
const PROFILE_NIX_FILE_SHELL: &str = "/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh";
const PROFILE_NIX_FILE_FISH: &str = "/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.fish";

/**
Configure any detected shell profiles to include Nix support
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureShellProfile {
    create_directories: Vec<StatefulAction<CreateDirectory>>,
    create_or_insert_into_files: Vec<StatefulAction<CreateOrInsertIntoFile>>,
}

impl ConfigureShellProfile {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(ssl_cert_file: Option<PathBuf>) -> Result<StatefulAction<Self>, ActionError> {
        let mut create_or_insert_files = Vec::default();
        let mut create_directories = Vec::default();

        let maybe_ssl_cert_file_setting = if let Some(ssl_cert_file) = ssl_cert_file {
            format!(
                "export NIX_SSL_CERT_FILE={:?}\n",
                ssl_cert_file
                    .canonicalize()
                    .map_err(|e| { ActionError::Canonicalize(ssl_cert_file, e) })?
            )
        } else {
            "".to_string()
        };
        let shell_buf = format!(
            "\n\
            # Nix\n\
            {maybe_ssl_cert_file_setting}\
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
                    CreateOrInsertIntoFile::plan(
                        profile_target_path,
                        None,
                        None,
                        None,
                        shell_buf.to_string(),
                        create_or_insert_into_file::Position::Beginning,
                    )
                    .await?,
                );
            }
        }

        let fish_buf = format!(
            "\n\
            # Nix\n\
            {maybe_ssl_cert_file_setting}\
            if test -e '{PROFILE_NIX_FILE_FISH}'\n\
            {inde}. '{PROFILE_NIX_FILE_FISH}'\n\
            end\n\
            # End Nix\n\
        \n",
            inde = "    ", // indent
        );

        for fish_prefix in PROFILE_FISH_CONFD_PREFIXES {
            let fish_prefix_path = PathBuf::from(fish_prefix);

            if !fish_prefix_path.exists() {
                // If the prefix doesn't exist, don't create the `conf.d/nix.fish`
                continue;
            }

            let mut profile_target = fish_prefix_path;
            profile_target.push(PROFILE_FISH_CONFD_SUFFIX);

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
        for fish_prefix in PROFILE_FISH_VENDOR_CONFD_PREFIXES {
            let fish_prefix_path = PathBuf::from(fish_prefix);

            if !fish_prefix_path.exists() {
                // If the prefix doesn't exist, don't create the `conf.d/nix.fish`
                continue;
            }

            let mut profile_target = fish_prefix_path;
            profile_target.push(PROFILE_FISH_VENDOR_CONFD_SUFFIX);

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
                buf += &format!(
                    "/nix/var/nix/profiles/per-user/{}/profile/bin\n",
                    runner.name
                );
            }
            create_or_insert_files.push(
                CreateOrInsertIntoFile::plan(
                    &github_path,
                    None,
                    None,
                    None,
                    buf,
                    create_or_insert_into_file::Position::End,
                )
                .await?,
            )
        }

        Ok(Self {
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
        let mut errors = Vec::default();

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
                    .map_err(|e| {
                        ActionError::Child(
                            create_or_insert_into_file_clone.action_tag(),
                            Box::new(e),
                        )
                    })?;
                Result::<_, ActionError>::Ok((idx, create_or_insert_into_file_clone))
            });
        }

        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok((idx, create_or_insert_into_file))) => {
                    self.create_or_insert_into_files[idx] = create_or_insert_into_file
                },
                Ok(Err(e)) => errors.push(e),
                Err(e) => return Err(e)?,
            };
        }

        if !errors.is_empty() {
            if errors.len() == 1 {
                return Err(errors.into_iter().next().unwrap())?;
            } else {
                return Err(ActionError::Children(
                    errors.into_iter().map(|v| Box::new(v)).collect(),
                ));
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
        let mut errors: Vec<ActionError> = Vec::default();

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
                Err(e) => return Err(e)?,
            };
        }

        for create_directory in self.create_directories.iter_mut() {
            create_directory
                .try_revert()
                .await
                .map_err(|e| ActionError::Child(create_directory.action_tag(), Box::new(e)))?;
        }

        if !errors.is_empty() {
            if errors.len() == 1 {
                return Err(errors.into_iter().next().unwrap())?;
            } else {
                return Err(ActionError::Children(
                    errors.into_iter().map(|v| Box::new(v)).collect(),
                ));
            }
        }

        Ok(())
    }
}
