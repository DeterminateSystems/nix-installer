use std::collections::HashMap;

use crate::execute_command;

#[derive(thiserror::Error, Debug)]
pub enum LoadError {
    #[error("Profile plist parsing error: {0}")]
    Parse(#[from] plist::Error),

    #[error("Profile discovery error: {0}")]
    ProfileListing(#[from] crate::ActionErrorKind),
}

pub async fn load() -> Result<Policies, LoadError> {
    let buf = execute_command(
        tokio::process::Command::new("/usr/bin/profiles")
            .args(["show", "-output", "stdout-xml"])
            .stdin(std::process::Stdio::null()),
    )
    .await?
    .stdout;

    Ok(plist::from_reader(std::io::Cursor::new(buf))?)
}

pub type Policies = HashMap<Target, Vec<Profile>>;

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Target {
    #[serde(rename(deserialize = "_computerlevel"))]
    Computer,
    #[serde(untagged)]
    User(String),
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct Profile {
    pub profile_description: Option<String>,
    pub profile_display_name: Option<String>,
    pub profile_identifier: Option<String>,
    pub profile_install_date: Option<String>,
    #[serde(rename = "ProfileUUID")]
    pub profile_uuid: Option<String>,
    pub profile_version: Option<usize>,

    #[serde(default)]
    pub profile_items: Vec<ProfileItem>,
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "PayloadType", content = "PayloadContent")]
pub enum ProfileItem {
    #[serde(rename = "com.apple.systemuiserver")]
    SystemUIServer(SystemUIServer),

    #[serde(untagged)]
    Unknown(UnknownProfileItem),
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UnknownProfileItem {
    payload_type: String,
    payload_content: plist::Value,
}

impl std::cmp::Eq for UnknownProfileItem {}

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SystemUIServer {
    pub mount_controls: Option<MountControls>,
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct MountControls {
    #[serde(default)]
    pub harddisk_internal: Vec<HardDiskInternalOpts>,
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HardDiskInternalOpts {
    Authenticate,
    ReadOnly,
    Deny,
    Eject,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_parse_fail() {
        let parsed: Policies = plist::from_reader(std::io::Cursor::new(include_str!(
            "./profile.sample.fail.plist"
        )))
        .unwrap();
        assert_eq!(
            Policies::from([(
                Target::User("foo".into()),
                vec![Profile {
                    profile_description: Some("".into()),
                    profile_display_name: Some("Don't allow mounting internal devices".into()),
                    profile_identifier: Some(
                        "MyProfile.6F6670A3-65AC-4EA4-8665-91F8FCE289AB".into()
                    ),
                    profile_install_date: Some("2024-04-22 14:12:42 +0000".into()),
                    profile_uuid: Some("6F6670A3-65AC-4EA4-8665-91F8FCE289AB".into()),
                    profile_version: Some(1),
                    profile_items: vec![ProfileItem::SystemUIServer(SystemUIServer {
                        mount_controls: Some(MountControls {
                            harddisk_internal: vec![HardDiskInternalOpts::Deny],
                        })
                    })],
                }]
            )]),
            parsed
        );
    }

    #[test]
    fn try_parse_unknown() {
        let parsed: Policies = plist::from_reader(std::io::Cursor::new(include_str!(
            "./profile.sample.unknown.plist"
        )))
        .unwrap();

        assert_eq!(
            Policies::from([(
                Target::Computer,
                vec![Profile {
                    profile_description: Some("".into()),
                    profile_display_name: Some(
                        "macOS Software Update Policy: Mandatory Minor Upgrades".into()
                    ),
                    profile_identifier: Some("com.example".into()),
                    profile_install_date: Some("2024-04-22 00:00:00 +0000".into()),
                    profile_uuid: Some("F7972F85-2A4D-4609-A4BB-02CB0C34A3F8".into()),
                    profile_version: Some(1),
                    profile_items: vec![ProfileItem::Unknown(UnknownProfileItem {
                        payload_type: "com.apple.SoftwareUpdate".into(),
                        payload_content: plist::Value::Dictionary({
                            let mut dict = plist::dictionary::Dictionary::new();
                            dict.insert("AllowPreReleaseInstallation".into(), false.into());
                            dict.insert("AutomaticCheckEnabled".into(), true.into());
                            dict
                        })
                    })],
                }]
            )]),
            parsed
        );
    }
}
