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
use std::collections::HashMap;

use crate::{
    action::{
        base::CreateDirectory,
        common::{ConfigureNix, ProvisionNix},
        linux::{CreateSystemdSysext, StartSystemdUnit},
    },
    planner::Planner,
    BuiltinPlanner, CommonSettings, InstallPlan,
};
use clap::ArgAction;

#[derive(Debug, Clone, clap::Parser, serde::Serialize, serde::Deserialize)]
pub struct SteamDeck {
    #[clap(flatten)]
    pub settings: CommonSettings,
}

#[async_trait::async_trait]
#[typetag::serde(name = "steam-deck")]
impl Planner for SteamDeck {
    async fn default() -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(Self {
            settings: CommonSettings::default()?,
        })
    }

    async fn plan(self) -> Result<crate::InstallPlan, Box<dyn std::error::Error + Sync + Send>> {
        let sysext = "/var/lib/extensions/nix";
        Ok(InstallPlan {
            planner: Box::new(self.clone()),
            actions: vec![
                Box::new(
                    CreateDirectory::plan("/var/lib/extensions/", None, None, None, true).await?,
                ),
                Box::new(CreateDirectory::plan("/home/nix", None, None, None, true).await?),
                Box::new(CreateSystemdSysext::plan(sysext, "/home/nix").await?),
                Box::new(StartSystemdUnit::plan("systemd-sysext.service".to_string()).await?), // TODO: We should not disable this during uninstall if it's already on
                Box::new(StartSystemdUnit::plan("nix.mount").await?),
                Box::new(ProvisionNix::plan(self.settings.clone()).await?),
                Box::new(ConfigureNix::plan(self.settings, Some(sysext)).await?),
                Box::new(StartSystemdUnit::plan("nix-daemon.service".to_string()).await?),
            ],
        })
    }

    fn settings(
        &self,
    ) -> Result<HashMap<String, serde_json::Value>, Box<dyn std::error::Error + Sync + Send>> {
        let Self { settings } = self;
        let mut map = HashMap::default();

        map.extend(settings.describe()?.into_iter());

        Ok(map)
    }
}

impl Into<BuiltinPlanner> for SteamDeck {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::SteamDeck(self)
    }
}
