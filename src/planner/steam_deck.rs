/** Testing the Steam Deck Install (Summary of https://blogs.igalia.com/berto/2022/07/05/running-the-steam-decks-os-in-a-virtual-machine-using-qemu/)

One time step:

1. Grab the SteamOS: Steam Deck Image from https://store.steampowered.com/steamos/download/?ver=steamdeck&snr=
2. Extract it (this can take a bit)
    ```sh
    bunzip2 steamdeck-recovery-4.img.bz2
    ```
2. Create a disk image
    ```sh
    qemu-img create -f qcow2 steamos.qcow2 64G
    ```
3. Start a VM to run the install onto the created disk

    *Note:*
    ```sh
    RECOVERY_IMAGE=steamdeck-recovery-4.img
    nix build "nixpkgs#legacyPackages.x86_64-linux.OVMF.fd" --out-link ovmf
    qemu-system-x86_64 -enable-kvm -smp cores=4 -m 8G \
        -device usb-ehci -device usb-tablet \
        -device intel-hda -device hda-duplex \
        -device VGA,xres=1280,yres=800 \
        -drive if=pflash,format=raw,readonly=on,file=ovmf-fd/FV/OVMF.fd \
        -drive if=virtio,file=$RECOVERY_IMAGE,driver=raw \
        -device nvme,drive=drive0,serial=badbeef \
        -drive if=none,id=drive0,file=steamos.qcow2
    ```
4. Pick "Reimage Steam Deck". **Important:** when it is done do not reboot the steam deck, hit "Cancel"
5. Run `sudo steamos-chroot --disk /dev/nvme0n1 --partset A` and inside run this
    ```sh
    steamos-readonly disable
    echo -e '[Autologin]\nSession=plasma.desktop' > /etc/sddm.conf.d/zz-steamos-autologin.conf
    passwd deck
    sudo systemctl enable sshd
    steamos-readonly enable
    exit
    ```
6. Run `sudo steamos-chroot --disk /dev/nvme0n1 --partset B` and inside run the same above commands
7. Safely turn off the VM!


Repeated step:
1. Create a snapshot of the base install to work on
    ```sh
    cp steamos.qcow2 steamos-hack.qcow2
2. Run the VM
    ```sh
    nix build "nixpkgs#legacyPackages.x86_64-linux.OVMF.fd" --out-link ovmf
    qemu-system-x86_64 -enable-kvm -smp cores=4 -m 8G \
        -device usb-ehci -device usb-tablet \
        -device intel-hda -device hda-duplex \
        -device VGA,xres=1280,yres=800 \
        -drive if=pflash,format=raw,readonly=on,file=ovmf-fd/FV/OVMF_CODE.fd \
        -drive if=pflash,format=raw,readonly=on,file=ovmf-fd/FV/OVMF_VARS.fd \
        -drive if=virtio,file=steamos-hack.qcow2 \
        -device virtio-net-pci,netdev=net0 \
        -netdev user,id=net0,hostfwd=tcp::2222-:22
    ```
3. **Do your testing!** You can `ssh deck@localhost -p 2222` in and use `rsync -e 'ssh -p 2222' result/bin/nix-installer deck@localhost:nix-installer` to send a `nix-installer build.
4. Delete `steamos-hack.qcow2`


To test a specific channel of the Steam Deck:
1. Use `steamos-select-branch -l` to list possible branches.
2. Run `steamos-select-branch $BRANCH` to choose a branch
3. Run `steamos-update`
4. Run `sudo steamos-chroot --disk /dev/vda --partset A` and inside run this
    ```sh
    steamos-readonly disable
    echo -e '[Autologin]\nSession=plasma.desktop' > /etc/sddm.conf.d/zz-steamos-autologin.conf
    passwd deck
    sudo systemctl enable sshd
    steamos-readonly enable
    exit
    ```
5. Run `sudo steamos-chroot --disk /dev/vda --partset B` and inside run the same above commands
6. Safely turn off the VM!


To test on a specific build id of the Steam Deck:
1. Determine the build id to be targeted. On a running system this is found in `/etc/os-release` under `BUILD_ID`.
2. Run `steamos-update-os now --update-version $BUILD_ID`
    + If you can't access a specific build ID you may need to change branches, see above.
    + Be patient, don't ctrl+C it, it breaks. Don't reboot yet!
4. Run `sudo steamos-chroot --disk /dev/vda --partset A` and inside run this
    ```sh
    steamos-readonly disable
    echo -e '[Autologin]\nSession=plasma.desktop' > /etc/sddm.conf.d/zz-steamos-autologin.conf
    passwd deck
    sudo systemctl enable sshd
    steamos-readonly enable
    exit
    ```
5. Run `sudo steamos-chroot --disk /dev/vda --partset B` and inside run the same above commands
6. Safely turn off the VM!

*/
use std::{collections::HashMap, path::PathBuf, process::Output};

use tokio::process::Command;

use crate::{
    action::{
        base::{CreateDirectory, CreateFile, RemoveDirectory},
        common::{ConfigureInitService, ConfigureNix, CreateUsersAndGroups, ProvisionNix},
        linux::{
            EnsureSteamosNixDirectory, RevertCleanSteamosNixOffload, StartSystemdUnit,
            SystemctlDaemonReload,
        },
        Action, StatefulAction,
    },
    planner::{Planner, PlannerError},
    settings::{CommonSettings, InitSystem, InstallSettingsError},
    BuiltinPlanner,
};

use super::ShellProfileLocations;

/// A planner for the Valve Steam Deck running SteamOS
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Parser))]
pub struct SteamDeck {
    /// Where `/nix` will be bind mounted to. Deprecated in SteamOS build ID 20230522.1000 or later
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            env = "NIX_INSTALLER_STEAM_DECK_PERSISTENCE",
            default_value = "/home/nix"
        )
    )]
    persistence: PathBuf,
    #[cfg_attr(feature = "cli", clap(flatten))]
    pub settings: CommonSettings,
}

#[async_trait::async_trait]
#[typetag::serde(name = "steam-deck")]
impl Planner for SteamDeck {
    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            persistence: PathBuf::from("/home/nix"),
            settings: CommonSettings::default().await?,
        })
    }

    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError> {
        // Starting in roughly build ID `20230522.1000`, the Steam Deck has a `/home/.steamos/offload/nix` directory and `nix.mount` unit we can use instead of creating a mountpoint.
        let requires_nix_bind_mount = detect_requires_bind_mount().await?;

        let mut actions = vec![
            // Primarily for uninstall
            SystemctlDaemonReload::plan()
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        ];

        if let Ok(nix_mount_status) = systemctl_status("nix.mount").await {
            let nix_mount_status_stderr = String::from_utf8(nix_mount_status.stderr)?;
            if nix_mount_status_stderr.contains("Warning: The unit file, source configuration file or drop-ins of nix.mount changed on disk. Run 'systemctl daemon-reload' to reload units.") {
                return Err(PlannerError::Custom(Box::new(
                    SteamDeckError::NixMountSystemctlDaemonReloadRequired,
                )))
            }
        }

        if requires_nix_bind_mount {
            let persistence = &self.persistence;
            if !persistence.is_absolute() {
                return Err(PlannerError::Custom(Box::new(
                    SteamDeckError::AbsolutePathRequired(self.persistence.clone()),
                )));
            };
            actions.push(
                CreateDirectory::plan(&persistence, None, None, 0o0755, true)
                    .await
                    .map_err(PlannerError::Action)?
                    .boxed(),
            );

            let nix_directory_buf = "\
                [Unit]\n\
                Description=Create a `/nix` directory to be used for bind mounting\n\
                PropagatesStopTo=nix-daemon.service\n\
                PropagatesStopTo=nix.mount\n\
                DefaultDependencies=no\n\
                After=grub-recordfail.service\n\
                After=steamos-finish-oobe-migration.service\n\
                \n\
                [Service]\n\
                Type=oneshot\n\
                ExecStart=steamos-readonly disable\n\
                ExecStart=mkdir -vp /nix\n\
                ExecStart=chmod -v 0755 /nix\n\
                ExecStart=chown -v root /nix\n\
                ExecStart=chgrp -v root /nix\n\
                ExecStart=steamos-readonly enable\n\
                ExecStop=steamos-readonly disable\n\
                ExecStop=rmdir /nix\n\
                ExecStop=steamos-readonly enable\n\
                RemainAfterExit=true\n\
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
            actions.push(nix_directory_unit.boxed());

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
                persistence = persistence.display(),
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
            actions.push(create_bind_mount_unit.boxed());
        } else {
            let revert_clean_streamos_nix_offload = RevertCleanSteamosNixOffload::plan()
                .await
                .map_err(PlannerError::Action)?;
            actions.push(revert_clean_streamos_nix_offload.boxed());

            let ensure_steamos_nix_directory = EnsureSteamosNixDirectory::plan()
                .await
                .map_err(PlannerError::Action)?;
            actions.push(ensure_steamos_nix_directory.boxed());

            let start_nix_mount = StartSystemdUnit::plan("nix.mount".to_string(), true)
                .await
                .map_err(PlannerError::Action)?;
            actions.push(start_nix_mount.boxed());
        }

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
        actions.push(ensure_symlinked_units_resolve_unit.boxed());

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

        if requires_nix_bind_mount {
            actions.push(
                StartSystemdUnit::plan("nix.mount".to_string(), false)
                    .await
                    .map_err(PlannerError::Action)?
                    .boxed(),
            )
        }

        actions.append(&mut vec![
            ProvisionNix::plan(&self.settings.clone())
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            CreateUsersAndGroups::plan(self.settings.clone())
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            ConfigureNix::plan(shell_profile_locations, &self.settings)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            // Init is required for the steam-deck archetype to make the `/nix` mount
            ConfigureInitService::plan(InitSystem::Systemd, false, true)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            StartSystemdUnit::plan("ensure-symlinked-units-resolve.service".to_string(), true)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            RemoveDirectory::plan(crate::settings::SCRATCH_DIR)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            SystemctlDaemonReload::plan()
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        ]);
        Ok(actions)
    }

    fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        let Self {
            settings,
            persistence,
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
        super::linux::check_not_wsl1()?;

        // Unlike the Linux planner, the steam deck planner requires systemd
        super::linux::check_systemd_active()?;

        Ok(())
    }

    async fn pre_install_check(&self) -> Result<(), PlannerError> {
        super::linux::check_not_nixos()?;

        super::linux::check_nix_not_already_installed().await?;

        super::linux::check_not_wsl1()?;

        // Unlike the Linux planner, the steam deck planner requires systemd
        super::linux::check_systemd_active()?;

        Ok(())
    }
}

impl From<SteamDeck> for BuiltinPlanner {
    fn from(val: SteamDeck) -> Self {
        BuiltinPlanner::SteamDeck(val)
    }
}

#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum SteamDeckError {
    #[error("`{0}` is not a path that can be canonicalized into an absolute path, bind mounts require an absolute path")]
    AbsolutePathRequired(PathBuf),
    #[error("A `/home/.steamos/offload/nix` exists, however `nix.mount` does not point at it. If Nix was previously installed, try uninstalling then rebooting first")]
    OffloadExistsButUnitIncorrect,
    #[error("Detected the SteamOS `nix.mount` unit exists, but `systemctl status nix.mount` did not return success. Try running `systemctl daemon-reload`.")]
    SteamosNixMountUnitNotExists,
    #[error("Detected the SteamOS `nix.mount` unit exists, but `systemctl status nix.mount` returned a warning that `systemctl daemon-reload` should be run. Run `systemctl daemon-reload` then `systemctl start nix.mount`, then try again.")]
    NixMountSystemctlDaemonReloadRequired,
}

pub(crate) async fn detect_requires_bind_mount() -> Result<bool, PlannerError> {
    let steamos_nix_mount_unit_path = "/usr/lib/systemd/system/nix.mount";
    let nix_mount_unit = tokio::fs::read_to_string(steamos_nix_mount_unit_path)
        .await
        .ok();

    match nix_mount_unit {
        Some(nix_mount_unit) if nix_mount_unit.contains("What=/home/.steamos/offload/nix") => {
            Ok(false)
        },
        None | Some(_) => Ok(true),
    }
}

async fn systemctl_status(unit: &str) -> Result<Output, PlannerError> {
    let mut command = Command::new("systemctl");
    command.arg("status");
    command.arg(unit);
    let output = command
        .output()
        .await
        .map_err(|e| PlannerError::Command(format!("{:?}", command.as_std()), e))?;
    Ok(output)
}
