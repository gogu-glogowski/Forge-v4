use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{ForgeError, Result};
use crate::profile::{Profile, SYSTEM_URI};
use crate::role::Role;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraDisk {
    pub name: String,
    pub path: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseProof {
    pub profile: String,
    pub source_url: String,
    pub artifact: String,
    pub upstream_checksum: String,
    pub upstream_kind: String,
    pub base_digest: String,
    pub base_path: String,
    #[serde(default)]
    pub extra: Vec<ExtraDisk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ownership {
    pub name: String,
    pub uuid: String,
    pub profile: String,
    pub role: String,
    pub overlay: String,
    pub base: String,
    pub base_digest: String,
    pub libvirt_uri: String,
}

impl Ownership {
    pub fn profile(&self) -> Result<Profile> {
        self.profile.parse()
    }

    pub fn role(&self) -> Result<Role> {
        Role::parse(&self.role).ok_or_else(|| {
            ForgeError::Ownership(format!("unknown role in ownership: {}", self.role))
        })
    }

    #[must_use]
    pub fn uri(&self) -> &str {
        if self.libvirt_uri.is_empty() {
            SYSTEM_URI
        } else {
            &self.libvirt_uri
        }
    }
}

pub fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| ForgeError::Ownership(error.to_string()))?;
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|error| ForgeError::Ownership(error.to_string()))
}
