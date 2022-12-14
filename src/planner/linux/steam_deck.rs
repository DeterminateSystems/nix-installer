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
3. **Do your testing!** You can `ssh deck@localhost -p 2222` in and use `rsync -e 'ssh -p 2222' result/bin/harmonic deck@localhost:harmonic` to send a harmonic build.
4. Delete `steamos-hack.qcow2`
*/
use std::{collections::HashMap, path::PathBuf};

use crate::{
    action::{
        base::{CreateDirectory, CreateFile},
        common::{ConfigureNix, ProvisionNix},
        linux::StartSystemdUnit,
        Action, StatefulAction,
    },
    planner::{Planner, PlannerError},
    settings::{CommonSettings, InstallSettingsError},
    BuiltinPlanner,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Parser))]
pub struct SteamDeck {
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            env = "HARMONIC_STEAM_DECK_PERSISTENCE",
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
            settings: CommonSettings::default()?,
        })
    }

    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError> {
        let persistence = &self.persistence;
        if persistence.is_absolute() {
            return Err(PlannerError::Custom(Box::new(
                SteamDeckError::AbsolutePathRequired(self.persistence.clone()),
            )));
        };

        let nix_directory_buf = format!(
            "\
            [Unit]\n\
            Description=Create a `/nix` directory to be used for bind mounting\n\
            PropagatesStopTo=nix-daemon.service\n\
            PropagatesStopTo=nix.mount\n\
            DefaultDependencies=no\n\
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
        );
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

        let ensure_symlinked_units_resolve_buf = format!(
            "\
            [Unit]\n\
            Description=Ensure Nix related units which are symlinked resolve\n\
            After=nix.mount\n\
            Requires=nix-directory.service\n\
            Requires=nix.mount\n\
            PropagatesStopTo=nix-directory.service\n\
            PropagatesStopTo=nix.mount\n\
            DefaultDependencies=no\n\
            \n\
            [Service]\n\
            Type=oneshot\n\
            RemainAfterExit=yes\n\
            ExecStart=/usr/bin/systemctl daemon-reload\n\
            ExecStart=/usr/bin/systemctl restart --no-block sockets.target timers.target multi-user.target\n\
            \n\
            [Install]\n\
            WantedBy=sysinit.target\n\
        "
        );
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

        Ok(vec![
            CreateDirectory::plan(&persistence, None, None, 0o0755, true)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            nix_directory_unit.boxed(),
            create_bind_mount_unit.boxed(),
            ensure_symlinked_units_resolve_unit.boxed(),
            StartSystemdUnit::plan("ensure-symlinked-units-resolve.service".to_string())
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            ProvisionNix::plan(&self.settings.clone())
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            ConfigureNix::plan(&self.settings)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            StartSystemdUnit::plan("nix-daemon.socket".to_string())
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        ])
    }

    fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        let Self {
            settings,
            persistence,
        } = self;
        let mut map = HashMap::default();

        map.extend(settings.settings()?.into_iter());
        map.insert(
            "persistence".to_string(),
            serde_json::to_value(persistence)?,
        );

        Ok(map)
    }
}

impl Into<BuiltinPlanner> for SteamDeck {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::SteamDeck(self)
    }
}

#[derive(thiserror::Error, Debug)]
enum SteamDeckError {
    #[error("`{0}` is not an absolute path, bind mounts require an absolute path and it cannot be canonicalized during planning")]
    AbsolutePathRequired(PathBuf),
}
