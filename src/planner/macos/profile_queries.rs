use crate::planner::macos::profiles::{
    HardDiskInternalOpts, MountControls, Policies, Profile, ProfileItem, SystemUIServer, Target,
};

struct TargetProfileItem<'a> {
    target: &'a Target,
    profile: &'a Profile,
    item: &'a ProfileItem,
}

pub struct TargetProfileHardDiskInternalOpts<'a> {
    pub target: &'a Target,
    pub profile: &'a Profile,
    pub opts: &'a [HardDiskInternalOpts],
}

impl TargetProfileHardDiskInternalOpts<'_> {
    pub fn display(&self) -> String {
        let owner = match self.target {
            crate::planner::macos::profiles::Target::Computer => {
                "A computer-wide profile".to_string()
            },
            crate::planner::macos::profiles::Target::User(u) => format!("A profile owned by {u}"),
        };

        let desc = [
            ("Name", &self.profile.profile_display_name),
            (
                "Version",
                &self.profile.profile_version.map(|v| v.to_string()),
            ),
            ("Description", &self.profile.profile_description),
            ("ID", &self.profile.profile_identifier),
            ("UUID", &self.profile.profile_uuid),
            ("Installation Date", &self.profile.profile_install_date),
        ]
        .into_iter()
        .filter_map(|(k, v)| Some((k, (*v).as_ref()?)))
        .map(|(key, val)| format!(" * {}: {}", key, val))
        .collect::<Vec<String>>()
        .join("\n");

        format!("{owner}:\n{}\n", desc)
    }
}

fn flatten(policies: &Policies) -> impl Iterator<Item = TargetProfileItem<'_>> {
    policies
        .iter()
        .flat_map(|(target, profiles): (&Target, &Vec<Profile>)| {
            profiles.iter().map(move |profile| (target, profile))
        })
        .flat_map(|(target, profile): (&Target, &Profile)| {
            profile
                .profile_items
                .iter()
                .map(move |item| TargetProfileItem {
                    target,
                    profile,
                    item,
                })
        })
}

pub fn blocks_internal_mounting(policies: &Policies) -> Vec<TargetProfileHardDiskInternalOpts<'_>> {
    flatten(policies)
        .filter_map(move |target_profile_item| {
            let ProfileItem::SystemUIServer(system_ui_server) = target_profile_item.item else {
                return None;
            };
            let SystemUIServer {
                mount_controls: Some(mount_controls),
            } = system_ui_server
            else {
                return None;
            };

            let MountControls { harddisk_internal } = mount_controls;

            Some(TargetProfileHardDiskInternalOpts {
                target: target_profile_item.target,
                profile: target_profile_item.profile,
                opts: harddisk_internal,
            })
        })
        .filter(|TargetProfileHardDiskInternalOpts { opts, .. }| {
            opts.iter().any(|x| {
                [
                    HardDiskInternalOpts::ReadOnly,
                    HardDiskInternalOpts::Deny,
                    HardDiskInternalOpts::Eject,
                ]
                .contains(x)
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_error() {
        let parsed: Policies = plist::from_reader(std::io::Cursor::new(include_str!(
            "./profile.sample.block.plist"
        )))
        .unwrap();

        let blocks = blocks_internal_mounting(&parsed);
        let err = &blocks[0];

        assert_eq!(
            r#"A profile owned by foo:
 * Name: Don't allow mounting internal devices
 * Version: 1
 * Description: The description
 * ID: MyProfile.6F6670A3-65AC-4EA4-8665-91F8FCE289AB
 * UUID: 6F6670A3-65AC-4EA4-8665-91F8FCE289AB
 * Installation Date: 2024-04-22 14:12:42 +0000"#
                .trim()
                .to_string(),
            err.display().trim()
        );
    }

    #[test]
    fn no_error() {
        let parsed: Policies = plist::from_reader(std::io::Cursor::new(include_str!(
            "./profile.sample.unknown.plist"
        )))
        .unwrap();

        let blocks = blocks_internal_mounting(&parsed);
        assert!(blocks.is_empty());
    }
}
