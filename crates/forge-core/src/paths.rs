use std::env;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::profile::Profile;

#[derive(Debug, Clone)]
pub struct ForgePaths {
    pub root: PathBuf,
    pub bases: PathBuf,
    pub vms: PathBuf,
    pub meta: PathBuf,
    pub privileged_bases: bool,
}

impl ForgePaths {
    #[must_use]
    pub fn discover() -> Self {
        if let Some(root) = env::var_os("FORGE_DATA_DIR").map(PathBuf::from) {
            return Self::under(root, false);
        }
        let home = env::var_os("HOME").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
        let meta = env::var_os("XDG_DATA_HOME").map_or_else(
            || home.join(".local/share/forge"),
            |xdg| PathBuf::from(xdg).join("forge"),
        );
        Self {
            root: PathBuf::from("/var/lib/forge"),
            bases: PathBuf::from("/var/lib/forge/bases"),
            vms: PathBuf::from("/var/lib/forge/vms"),
            meta,
            privileged_bases: true,
        }
    }

    #[must_use]
    pub fn under(root: PathBuf, privileged_bases: bool) -> Self {
        Self {
            bases: root.join("bases"),
            vms: root.join("vms"),
            meta: root.join("meta"),
            root,
            privileged_bases,
        }
    }

    pub fn ensure_user(&self) -> Result<()> {
        fs::create_dir_all(self.cache_root())?;
        fs::create_dir_all(self.keys_root())?;
        fs::create_dir_all(self.proofs_root())?;
        fs::create_dir_all(self.ownership_root())?;
        if !self.privileged_bases {
            fs::create_dir_all(&self.bases)?;
            fs::create_dir_all(&self.vms)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn cache_root(&self) -> PathBuf {
        self.meta.join("cache")
    }

    #[must_use]
    pub fn keys_root(&self) -> PathBuf {
        self.meta.join("keys")
    }

    #[must_use]
    pub fn proofs_root(&self) -> PathBuf {
        self.meta.join("proofs")
    }

    #[must_use]
    pub fn ownership_root(&self) -> PathBuf {
        self.meta.join("vms")
    }

    #[must_use]
    pub fn cache_dir(&self, profile: Profile) -> PathBuf {
        self.cache_root().join(profile.id())
    }

    #[must_use]
    pub fn base_qcow2(&self, profile: Profile) -> PathBuf {
        match profile {
            Profile::Whonix => self.named_base(crate::profile::WHONIX_GW_NAME),
            _ => self.named_base(profile.id()),
        }
    }

    #[must_use]
    pub fn named_base(&self, name: &str) -> PathBuf {
        self.bases.join(format!("{name}.qcow2"))
    }

    #[must_use]
    pub fn whonix_bases(&self) -> (PathBuf, PathBuf) {
        (
            self.named_base(crate::profile::WHONIX_GW_NAME),
            self.named_base(crate::profile::WHONIX_WS_NAME),
        )
    }

    #[must_use]
    pub fn overlay_qcow2(&self, vm: &str) -> PathBuf {
        self.vms.join(format!("{vm}.qcow2"))
    }

    #[must_use]
    pub fn proof_file(&self, profile: Profile) -> PathBuf {
        self.proofs_root().join(format!("{}.json", profile.id()))
    }

    #[must_use]
    pub fn ownership_file(&self, vm: &str) -> PathBuf {
        self.ownership_root().join(format!("{vm}.json"))
    }
}

pub fn chmod(path: &Path, mode: u32) -> Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(mode);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[must_use]
pub fn is_readable(path: &Path) -> bool {
    File::open(path).is_ok()
}

#[must_use]
pub fn dir_writable(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    let probe = path.join(format!(".forge-w-{}", std::process::id()));
    match OpenOptions::new().create_new(true).write(true).open(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// SELinux type from `ls -Z`, if any (e.g. `virt_content_t`).
#[must_use]
pub fn selinux_type(path: &Path) -> Option<String> {
    let output = crate::cmd::command("ls")
        .args(["-Zd", path.to_str()?])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // ls -Z: unconfined_u:object_r:virt_content_t:s0 filename
    text.split_whitespace().find_map(|token| {
        token
            .split(':')
            .find(|part| part.ends_with("_t") && *part != "s0")
            .map(ToOwned::to_owned)
    })
}

/// Effective UID from `/proc/self/status`. `sudo forge` is 0; that is not how Forge runs.
#[must_use]
pub fn effective_uid() -> u32 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("Uid:").and_then(|rest| {
                    rest.split_whitespace()
                        .nth(1)
                        .and_then(|uid| uid.parse().ok())
                })
            })
        })
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_override_is_unprivileged() {
        let paths = ForgePaths::under(PathBuf::from("/tmp/forge-test"), false);
        assert!(!paths.privileged_bases);
        assert_eq!(
            paths.base_qcow2(Profile::Tsurugi).as_os_str(),
            "/tmp/forge-test/bases/tsurugi.qcow2"
        );
    }

    #[test]
    fn dir_writable_and_readable_probes() {
        let dir = std::env::temp_dir().join(format!("forge-paths-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("x");
        fs::write(&file, b"ok").unwrap();
        assert!(dir_writable(&dir));
        assert!(is_readable(&file));
        let mut perms = fs::metadata(&file).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&file, perms).unwrap();
        assert!(!is_readable(&file));
        let mut perms = fs::metadata(&file).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&file, perms).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_ne!(effective_uid(), 0);
    }
}
