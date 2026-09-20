//! Point GNOME Boxes at `qemu:///system` so Forge overlays show up.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Result;

pub const SOURCE_NAME: &str = "QEMU System";
const SOURCE_BODY: &str = "\
[source]
name=QEMU System
type=libvirt
uri=qemu+unix:///system
save-on-quit=true
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStatus {
    Already,
    Wrote,
}

/// Idempotent. Boxes only lists brokers listed under `sources/`.
pub fn ensure_system_source() -> Result<SourceStatus> {
    ensure_system_source_in(&config_home())
}

pub fn ensure_system_source_in(config_home: &Path) -> Result<SourceStatus> {
    let dir = config_home.join("gnome-boxes").join("sources");
    fs::create_dir_all(&dir)?;
    let path = dir.join(SOURCE_NAME);
    if path.is_file() {
        let text = fs::read_to_string(&path)?;
        if has_system_uri(&text) {
            return Ok(SourceStatus::Already);
        }
    }
    fs::write(&path, SOURCE_BODY)?;
    Ok(SourceStatus::Wrote)
}

#[must_use]
pub fn has_system_uri(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    t.contains("qemu+unix:///system") || t.contains("qemu:///system")
}

#[must_use]
pub fn source_path() -> PathBuf {
    config_home()
        .join("gnome-boxes")
        .join("sources")
        .join(SOURCE_NAME)
}

fn config_home() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg);
    }
    let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(home).join(".config")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_system_uri_once() {
        let dir = std::env::temp_dir().join(format!("forge-boxes-{}", uuid::Uuid::new_v4()));
        assert_eq!(ensure_system_source_in(&dir).unwrap(), SourceStatus::Wrote);
        let text = fs::read_to_string(dir.join("gnome-boxes/sources/QEMU System")).unwrap();
        assert!(has_system_uri(&text));
        assert!(text.contains("type=libvirt"));
        assert_eq!(
            ensure_system_source_in(&dir).unwrap(),
            SourceStatus::Already
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn repairs_wrong_uri() {
        let dir = std::env::temp_dir().join(format!("forge-boxes-{}", uuid::Uuid::new_v4()));
        let path = dir.join("gnome-boxes/sources/QEMU System");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "[source]\nuri=qemu+unix:///session\n").unwrap();
        assert_eq!(ensure_system_source_in(&dir).unwrap(), SourceStatus::Wrote);
        let text = fs::read_to_string(path).unwrap();
        assert!(has_system_uri(&text));
        let _ = fs::remove_dir_all(dir);
    }
}
