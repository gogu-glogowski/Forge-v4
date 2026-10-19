//! Host confinement policy. Hooks use a trusted file, never call back into libvirt.
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

use crate::error::{ForgeError, Result};
use crate::host::{Host, SecurityState};
use crate::paths::ForgePaths;

pub const POLICY_PATH: &str = "/var/lib/forge/security-policy";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Selinux,
    Apparmor,
}

impl Policy {
    pub fn model(self) -> &'static str {
        match self {
            Self::Selinux => "selinux",
            Self::Apparmor => "apparmor",
        }
    }

    pub fn select(
        models: &[String],
        selinux: SecurityState,
        apparmor: SecurityState,
    ) -> Result<Self> {
        if models.iter().any(|m| m == "selinux") && selinux == SecurityState::Enforcing {
            Ok(Self::Selinux)
        } else if models.iter().any(|m| m == "apparmor") && apparmor == SecurityState::Enabled {
            Ok(Self::Apparmor)
        } else {
            Err(ForgeError::Host("libvirt must advertise an active enforcing SELinux or enabled AppArmor driver; refusing an unconfined VM".to_owned()))
        }
    }

    pub fn discover() -> Result<Self> {
        let host = Host::discover();
        Self::select(
            &host.security.advertised_models,
            host.security.selinux,
            host.security.apparmor,
        )
    }

    pub fn for_paths(paths: &ForgePaths) -> Result<Self> {
        // Unit fixtures generate XML without a running system libvirt daemon.
        #[cfg(test)]
        if !paths.privileged_bases {
            return Ok(Self::Selinux);
        }
        let _ = paths;
        Self::discover()
    }

    pub fn kernel_active(self) -> bool {
        match self {
            Self::Selinux => {
                fs::read_to_string("/sys/fs/selinux/enforce").is_ok_and(|s| s.trim() == "1")
            }
            Self::Apparmor => fs::read_to_string("/sys/module/apparmor/parameters/enabled")
                .is_ok_and(|s| s.trim() == "Y"),
        }
    }

    pub fn trusted() -> Result<Self> {
        let policy = Self::trusted_at(Path::new(POLICY_PATH))?;
        if !policy.kernel_active() {
            return Err(ForgeError::Host(format!(
                "{} is no longer active; refusing Forge start",
                policy.model()
            )));
        }
        Ok(policy)
    }

    fn trusted_at(path: &Path) -> Result<Self> {
        for p in [
            path.parent()
                .ok_or_else(|| ForgeError::Host("policy has no parent".to_owned()))?,
            path,
        ] {
            let meta = fs::symlink_metadata(p)?;
            if meta.uid() != 0
                || meta.permissions().mode() & 0o022 != 0
                || meta.file_type().is_symlink()
            {
                return Err(ForgeError::Host("security policy and its directory must be root-owned, non-writable by other users and not symlinks".to_owned()));
            }
        }
        if !fs::symlink_metadata(path)?.is_file() {
            return Err(ForgeError::Host(
                "security policy must be a regular file".to_owned(),
            ));
        }
        match fs::read_to_string(path)?.trim() {
            "selinux" => Ok(Self::Selinux),
            "apparmor" => Ok(Self::Apparmor),
            _ => Err(ForgeError::Host(
                "invalid Forge security policy; reinstall with forge dev hook".to_owned(),
            )),
        }
    }

    pub fn install_stamp_script(self) -> String {
        format!(
            "printf '%s\\n' '{}' > {POLICY_PATH}.new\nchown root:root {POLICY_PATH}.new\nchmod 0644 {POLICY_PATH}.new\nmv -f {POLICY_PATH}.new {POLICY_PATH}\n",
            self.model()
        )
    }

    /// Only the helper gets additional read access. No global QEMU guest grants.
    pub fn helper_script(self, paths: &ForgePaths) -> Result<String> {
        if self == Self::Selinux {
            return Ok(String::new());
        }
        validate_paths(paths)?;
        let bases = paths.bases.display();
        let vms = paths.vms.display();
        Ok(format!(
            r#"
command -v apparmor_parser >/dev/null
profile=''
for name in usr.lib.libvirt.virt-aa-helper usr.libexec.virt-aa-helper; do
  candidate="/etc/apparmor.d/$name"
  if [ -f "$candidate" ]; then profile="$candidate"; break; fi
done
[ -n "$profile" ] || {{ echo 'AppArmor virt-aa-helper profile missing' >&2; exit 1; }}
name=${{profile##*/}}
local="/etc/apparmor.d/local/$name"
[ ! -L "$local" ] && [ ! -L /etc/apparmor.d/local/forge-virt-aa-helper ]
grep -F "local/$name" "$profile" >/dev/null || {{ echo 'virt-aa-helper profile has no local include; configure it before installing the Forge hook' >&2; exit 1; }}
install -d -m 0755 -o root -g root /etc/apparmor.d/local
cat > /etc/apparmor.d/local/forge-virt-aa-helper.new <<'FORGE_AA'
# Forge: image inspection by virt-aa-helper, not access for every VM.
{bases}/ r,
{bases}/** r,
{vms}/ r,
{vms}/** r,
FORGE_AA
chown root:root /etc/apparmor.d/local/forge-virt-aa-helper.new
chmod 0644 /etc/apparmor.d/local/forge-virt-aa-helper.new
mv -f /etc/apparmor.d/local/forge-virt-aa-helper.new /etc/apparmor.d/local/forge-virt-aa-helper
touch "$local"
if ! grep -Fx '#include if exists <local/forge-virt-aa-helper>' "$local" >/dev/null; then
  printf '\n#include if exists <local/forge-virt-aa-helper>\n' >> "$local"
fi
chown root:root "$local"
chmod 0644 "$local"
apparmor_parser -r "$profile"
"#
        ))
    }
}

pub fn validate_paths(paths: &ForgePaths) -> Result<()> {
    let root = paths.root.to_str().unwrap_or_default();
    if !root.starts_with("/var/lib/")
        || root.ends_with('/')
        || !root
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
        || root
            .split('/')
            .any(|s| s == "." || s == ".." || (s.is_empty() && root != "/" && root.contains("//")))
        || paths.bases != paths.root.join("bases")
        || paths.vms != paths.root.join("vms")
    {
        return Err(ForgeError::Host("privileged Forge storage must be a literal directory below /var/lib without wildcards, traversal or shell metacharacters".to_owned()));
    }
    Ok(())
}

/// Immutable bases remain readable by any confined QEMU account, but writable by none.
pub fn base_is_protected(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.is_file()
        || meta.uid() != 0
        || meta.gid() != 0
        || meta.permissions().mode() & 0o7777 != 0o444
    {
        return false;
    }
    crate::cmd::run_checked("lsattr", &["-d", "--", path.to_str().unwrap_or("")]).is_ok_and(
        |text| {
            text.split_whitespace()
                .next()
                .is_some_and(|flags| flags.contains('i'))
        },
    )
}

pub fn storage_directories_protected(paths: &ForgePaths) -> bool {
    let libvirt_gid = crate::cmd::run_checked("getent", &["group", "libvirt"])
        .ok()
        .and_then(|text| text.split(':').nth(2)?.parse::<u32>().ok());
    [
        (&paths.root, 0o755, Some(0)),
        (&paths.bases, 0o755, Some(0)),
        (&paths.vms, 0o2771, libvirt_gid),
    ]
    .iter()
    .all(|(path, mode, gid)| {
        fs::symlink_metadata(path).is_ok_and(|meta| {
            meta.is_dir()
                && !meta.file_type().is_symlink()
                && meta.uid() == 0
                && Some(meta.gid()) == *gid
                && meta.permissions().mode() & 0o7777 == *mode
        })
    })
}

/// Parse actual XML structure. A comment or another device cannot satisfy policy.
pub fn validate_domain(xml: &str, policy: Policy) -> Result<()> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|e| ForgeError::Role(format!("invalid domain XML: {e}")))?;
    let domain = document.root_element();
    let labels: Vec<_> = domain
        .children()
        .filter(|n| n.has_tag_name("seclabel"))
        .collect();
    if !domain.has_tag_name("domain")
        || labels
            .iter()
            .any(|n| n.attribute("type") == Some("none") || n.attribute("relabel") == Some("no"))
        || labels
            .iter()
            .filter(|n| n.attribute("model") == Some(policy.model()))
            .count()
            != 1
        || !labels.iter().any(|n| {
            n.attribute("model") == Some(policy.model())
                && n.attribute("type") == Some("dynamic")
                && n.attribute("relabel") == Some("yes")
        })
        || labels.iter().any(|n| {
            matches!(n.attribute("model"), Some("apparmor" | "selinux"))
                && n.attribute("model") != Some(policy.model())
        })
    {
        return Err(ForgeError::Role(format!(
            "VM requires dynamic {} confinement; run forge start to regenerate its XML",
            policy.model()
        )));
    }
    let devices = domain
        .children()
        .find(|n| n.has_tag_name("devices"))
        .ok_or_else(|| ForgeError::Role("missing VM devices".to_owned()))?;
    let disks: Vec<_> = devices
        .children()
        .filter(|n| n.has_tag_name("disk"))
        .collect();
    if disks.len() != 1 {
        return Err(ForgeError::Role(
            "Forge requires exactly one overlay disk".to_owned(),
        ));
    }
    let backing = disks[0]
        .children()
        .find(|n| n.has_tag_name("backingStore"))
        .ok_or_else(|| ForgeError::Role("missing base backingStore".to_owned()))?;
    let source = backing
        .children()
        .find(|n| n.has_tag_name("source"))
        .ok_or_else(|| ForgeError::Role("missing base source".to_owned()))?;
    let labels: Vec<_> = source
        .children()
        .filter(|n| n.has_tag_name("seclabel"))
        .collect();
    let required: &[&str] = if policy == Policy::Selinux {
        &["dac", "selinux"]
    } else {
        &["dac"]
    };
    if labels.len() != required.len()
        || !required.iter().all(|model| {
            labels
                .iter()
                .filter(|n| {
                    n.attribute("model") == Some(*model) && n.attribute("relabel") == Some("no")
                })
                .count()
                == 1
        })
    {
        return Err(ForgeError::Role(
            "immutable base must skip DAC ownership changes and, on SELinux, relabeling".to_owned(),
        ));
    }
    // The writable overlay must inherit the domain policy.
    if disks[0]
        .children()
        .filter(|n| n.has_tag_name("source"))
        .any(|n| n.children().any(|c| c.has_tag_name("seclabel")))
    {
        return Err(ForgeError::Role(
            "overlay must inherit dynamic confinement and ownership".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_requires_kernel_and_libvirt_agreement() {
        assert_eq!(
            Policy::select(
                &["apparmor".into(), "dac".into()],
                SecurityState::Unknown,
                SecurityState::Enabled
            )
            .unwrap(),
            Policy::Apparmor
        );
        assert!(
            Policy::select(
                &["selinux".into()],
                SecurityState::Permissive,
                SecurityState::Enabled
            )
            .is_err()
        );
        assert!(
            Policy::select(
                &["dac".into()],
                SecurityState::Enforcing,
                SecurityState::Enabled
            )
            .is_err()
        );
    }
    #[test]
    fn helper_rules_do_not_grant_access_to_every_guest() {
        let paths = ForgePaths::under("/var/lib/forge".into(), true);
        let script = Policy::Apparmor.helper_script(&paths).unwrap();
        assert!(script.contains("/var/lib/forge/bases/** r,"));
        assert!(!script.contains("local/abstractions/libvirt-qemu"));
        assert!(!script.contains(" aa-disable"));
        assert!(
            std::process::Command::new("sh")
                .args(["-n", "-c", &script])
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(Policy::Selinux.helper_script(&paths).unwrap(), "");
    }
    #[test]
    fn privileged_paths_reject_apparmor_and_shell_injection() {
        for path in [
            "/",
            "/tmp/forge",
            "/var/lib/../etc",
            "/var/lib/a*",
            "/var/lib/a{b}",
            "/var/lib/a'",
            "/var/lib/a//b",
        ] {
            assert!(
                validate_paths(&ForgePaths::under(path.into(), true)).is_err(),
                "{path}"
            );
        }
        assert!(validate_paths(&ForgePaths::under("/var/lib/forge".into(), true)).is_ok());
    }
    #[test]
    fn regular_readable_but_mutable_user_base_is_not_protected() {
        let dir = std::env::temp_dir().join(format!("forge-base-policy-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("base.qcow2");
        fs::write(&path, b"fixture").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        assert!(!base_is_protected(&path));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn user_owned_and_symlink_policy_files_are_untrusted() {
        use std::os::unix::fs::symlink;
        let dir = std::env::temp_dir().join(format!("forge-policy-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("security-policy");
        fs::write(&path, "apparmor\n").unwrap();
        assert!(Policy::trusted_at(&path).is_err());
        let link = dir.join("link");
        symlink(&path, &link).unwrap();
        assert!(Policy::trusted_at(&link).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
