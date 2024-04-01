use crate::{
    action::{
        base::{CreateDirectory, CreateFile, RemoveDirectory},
        common::{ConfigureInitService, ConfigureNix, CreateUsersAndGroups, ProvisionNix},
        linux::{ProvisionSelinux, StartSystemdUnit, SystemctlDaemonReload},
        StatefulAction,
    },
    error::HasExpectedErrors,
    planner::{Planner, PlannerError},
    settings::CommonSettings,
    settings::{InitSystem, InstallSettingsError},
    Action, BuiltinPlanner,
};
use std::{collections::HashMap, path::PathBuf};

use super::{
    linux::{
        check_nix_not_already_installed, check_not_nixos, check_not_wsl1, check_systemd_active,
        detect_selinux,
    },
    ShellProfileLocations,
};

/// A planner suitable for immutable systems using ostree, such as Fedora Silverblue
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Parser))]
pub struct Ostree {
    /// Where `/nix` will be bind mounted to.
    #[cfg_attr(feature = "cli", clap(long, default_value = "/var/home/nix"))]
    persistence: PathBuf,
    #[cfg_attr(feature = "cli", clap(flatten))]
    pub settings: CommonSettings,
}

#[async_trait::async_trait]
#[typetag::serde(name = "ostree")]
impl Planner for Ostree {
    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            persistence: PathBuf::from("/var/home/nix"),
            settings: CommonSettings::default().await?,
        })
    }

    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError> {
        let has_selinux = detect_selinux().await?;
        let mut plan = vec![
            // Primarily for uninstall
            SystemctlDaemonReload::plan()
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        ];

        plan.push(
            CreateDirectory::plan(&self.persistence, None, None, 0o0755, true)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );

        let nix_directory_buf = "\
                [Unit]\n\
                Description=Enable mount points in / for ostree\n\
                ConditionPathExists=!/nix\n\
                DefaultDependencies=no\n\
                Requires=local-fs-pre.target\n\
                After=local-fs-pre.target\n\
                [Service]\n\
                Type=oneshot\n\
                ExecStartPre=chattr -i /\n\
                ExecStart=mkdir -p /nix\n\
                ExecStopPost=chattr +i /\n\
            "
        .to_string();
        let nix_directory_unit = CreateFile::plan(
            "/etc/systemd/system/nix-directory.service",
            None,
            None,
            0o0644,
            nix_directory_buf,
            false,
        )
        .await
        .map_err(PlannerError::Action)?;
        plan.push(nix_directory_unit.boxed());

        let create_bind_mount_buf = format!(
            "\
                [Unit]\n\
                Description=Mount `{persistence}` on `/nix`\n\
                PropagatesStopTo=nix-daemon.service\n\
                PropagatesStopTo=nix-directory.service\n\
                After=nix-directory.service\n\
                Requires=nix-directory.service\n\
                ConditionPathIsDirectory=/nix\n\
                DefaultDependencies=no\n\
                \n\
                [Mount]\n\
                What={persistence}\n\
                Where=/nix\n\
                Type=none\n\
                DirectoryMode=0755\n\
                Options=bind\n\
                \n\
                [Install]\n\
                RequiredBy=nix-daemon.service\n\
                RequiredBy=nix-daemon.socket\n
            ",
            persistence = self.persistence.display(),
        );
        let create_bind_mount_unit = CreateFile::plan(
            "/etc/systemd/system/nix.mount",
            None,
            None,
            0o0644,
            create_bind_mount_buf,
            false,
        )
        .await
        .map_err(PlannerError::Action)?;
        plan.push(create_bind_mount_unit.boxed());

        let ensure_symlinked_units_resolve_buf = "\
        [Unit]\n\
        Description=Ensure Nix related units which are symlinked resolve\n\
        After=nix.mount\n\
        Requires=nix.mount\n\
        DefaultDependencies=no\n\
        \n\
        [Service]\n\
        Type=oneshot\n\
        RemainAfterExit=yes\n\
        ExecStart=/usr/bin/systemctl daemon-reload\n\
        ExecStart=/usr/bin/systemctl restart --no-block nix-daemon.socket\n\
        \n\
        [Install]\n\
        WantedBy=sysinit.target\n\
    "
        .to_string();
        let ensure_symlinked_units_resolve_unit = CreateFile::plan(
            "/etc/systemd/system/ensure-symlinked-units-resolve.service",
            None,
            None,
            0o0644,
            ensure_symlinked_units_resolve_buf,
            false,
        )
        .await
        .map_err(PlannerError::Action)?;
        plan.push(ensure_symlinked_units_resolve_unit.boxed());

        // We need to remove this path since it's part of the read-only install.
        let mut shell_profile_locations = ShellProfileLocations::default();
        if let Some(index) = shell_profile_locations
            .fish
            .vendor_confd_prefixes
            .iter()
            .position(|v| *v == PathBuf::from("/usr/share/fish/"))
        {
            shell_profile_locations
                .fish
                .vendor_confd_prefixes
                .remove(index);
        }

        plan.push(
            StartSystemdUnit::plan("nix.mount".to_string(), false)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );

        plan.push(
            ProvisionNix::plan(&self.settings.clone())
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );
        plan.push(
            CreateUsersAndGroups::plan(self.settings.clone())
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );
        plan.push(
            ConfigureNix::plan(shell_profile_locations, &self.settings)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );

        if has_selinux {
            plan.push(
                ProvisionSelinux::plan("/etc/nix-installer/selinux/packages/nix.pp".into())
                    .await
                    .map_err(PlannerError::Action)?
                    .boxed(),
            );
        }

        plan.push(
            CreateDirectory::plan("/etc/tmpfiles.d", None, None, 0o0755, false)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );

        plan.push(
            ConfigureInitService::plan(InitSystem::Systemd, self.settings.nix_enterprise, true)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );
        plan.push(
            StartSystemdUnit::plan("ensure-symlinked-units-resolve.service".to_string(), true)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );
        plan.push(
            RemoveDirectory::plan(crate::settings::SCRATCH_DIR)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );
        plan.push(
            SystemctlDaemonReload::plan()
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );

        Ok(plan)
    }

    fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        let Self {
            persistence,
            settings,
        } = self;
        let mut map = HashMap::default();

        map.extend(settings.settings()?);
        map.insert(
            "persistence".to_string(),
            serde_json::to_value(persistence)?,
        );

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
    async fn diagnostic_data(&self) -> Result<crate::diagnostics::DiagnosticData, PlannerError> {
        Ok(crate::diagnostics::DiagnosticData::new(
            self.settings.diagnostic_attribution.clone(),
            self.settings.diagnostic_endpoint.clone(),
            self.typetag_name().into(),
            self.configured_settings()
                .await?
                .into_keys()
                .collect::<Vec<_>>(),
            self.settings.ssl_cert_file.clone(),
        )?)
    }
    async fn pre_uninstall_check(&self) -> Result<(), PlannerError> {
        check_not_wsl1()?;

        check_systemd_active()?;

        Ok(())
    }

    async fn pre_install_check(&self) -> Result<(), PlannerError> {
        check_not_nixos()?;

        check_nix_not_already_installed().await?;

        check_not_wsl1()?;

        check_systemd_active()?;

        Ok(())
    }
}

impl From<Ostree> for BuiltinPlanner {
    fn from(val: Ostree) -> Self {
        BuiltinPlanner::Ostree(val)
    }
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum OstreeError {
    #[error(
        "\
        systemd was not active.\n\
        \n\
        If it will be started later consider, passing `--no-start-daemon`.\n\
        \n\
        To use a `root`-only Nix install, consider passing `--init none`."
    )]
    SystemdNotActive,
    #[error(
        "\
        systemd was not active.\n\
        \n\
        On WSL2, systemd is not enabled by default. Consider enabling it by adding it to your `/etc/wsl.conf` with `echo -e '[boot]\\nsystemd=true'` then restarting WSL2 with `wsl.exe --shutdown` and re-entering the WSL shell. For more information, see https://devblogs.microsoft.com/commandline/systemd-support-is-now-available-in-wsl/.\n\
        \n\
        If it will be started later consider, passing `--no-start-daemon`.\n\
        \n\
        To use a `root`-only Nix install, consider passing `--init none`."
    )]
    Wsl2SystemdNotActive,
}

impl HasExpectedErrors for OstreeError {
    fn expected<'a>(&'a self) -> Option<Box<dyn std::error::Error + 'a>> {
        match self {
            OstreeError::SystemdNotActive => Some(Box::new(self)),
            OstreeError::Wsl2SystemdNotActive => Some(Box::new(self)),
        }
    }
}

impl From<OstreeError> for PlannerError {
    fn from(v: OstreeError) -> PlannerError {
        PlannerError::Custom(Box::new(v))
    }
}
