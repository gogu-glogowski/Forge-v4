use std::fmt;
use std::str::FromStr;

use crate::error::{ForgeError, Result};
use crate::role::Role;

pub const APP_NAME: &str = "forge";
pub const METADATA_NS: &str = "https://github.com/gogu-glogowski/Forge-v4";
pub const SYSTEM_URI: &str = "qemu:///system";
pub const FORGE_VMS_POOL: &str = "forge-vms";
pub const WHONIX_NET: &str = "forge-whonix";

/// Tsurugi LAB 26.03 — published short id 0x116AD57C is the encryption subkey.
pub const TSURUGI_KEY_FPR: &str = "68A60308FCD7BCA0F81E75016DF20CE124289711";
pub const TSURUGI_KEY_ASC: &str = include_str!("../../../keys/tsurugi.asc");
pub const TSURUGI_SUMS_URL: &str = "https://tsurugi-linux.org/signed_hashes.sha512";
pub const TSURUGI_OVA_NAME: &str = "tsurugi_linux_26.03.ova";
pub const TSURUGI_OVA_URL: &str = "https://ftp.nluug.nl/os/Linux/distr/tsurugi/01.Tsurugi_Linux_%5bLAB%5d/tsurugi_linux_26.03.ova";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Profile {
    Tsurugi,
    Sift,
    Kali,
    Whonix,
}

impl Profile {
    #[must_use]
    pub fn all() -> [Self; 4] {
        [Self::Tsurugi, Self::Sift, Self::Kali, Self::Whonix]
    }

    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Tsurugi => "tsurugi",
            Self::Sift => "sift",
            Self::Kali => "kali",
            Self::Whonix => "whonix",
        }
    }

    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Tsurugi => "Tsurugi LAB",
            Self::Sift => "SIFT Workstation",
            Self::Kali => "Kali",
            Self::Whonix => "Whonix (gateway + workstation)",
        }
    }

    #[must_use]
    pub fn role(self) -> Role {
        match self {
            Self::Tsurugi | Self::Sift => Role::Isolated,
            Self::Kali => Role::OsintClearnet,
            Self::Whonix => Role::WhonixGw,
        }
    }

    #[must_use]
    pub fn is_isolated(self) -> bool {
        matches!(self, Self::Tsurugi | Self::Sift)
    }

    /// This cut: pull + create for Tsurugi (isolated) only.
    #[must_use]
    pub fn engine_ready(self) -> bool {
        matches!(self, Self::Tsurugi)
    }

    #[must_use]
    pub fn default_vm_names(self) -> &'static [&'static str] {
        match self {
            Self::Tsurugi => &["tsurugi"],
            Self::Sift => &["sift"],
            Self::Kali => &["kali"],
            Self::Whonix => &["whonix-gateway", "whonix-workstation"],
        }
    }

    #[must_use]
    pub fn memory_mib(self) -> u32 {
        match self {
            Self::Tsurugi | Self::Sift => 8192,
            Self::Kali | Self::Whonix => 4096,
        }
    }

    #[must_use]
    pub fn vcpus(self) -> u32 {
        match self {
            Self::Tsurugi | Self::Sift => 4,
            Self::Kali | Self::Whonix => 2,
        }
    }

    pub fn require_engine(self) -> Result<()> {
        if self.engine_ready() {
            Ok(())
        } else {
            Err(ForgeError::NotThisCut(format!(
                "{}: this cut implements pull/create for tsurugi (isolated) only",
                self.id()
            )))
        }
    }
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

impl FromStr for Profile {
    type Err = ForgeError;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "tsurugi" => Ok(Self::Tsurugi),
            "sift" => Ok(Self::Sift),
            "kali" => Ok(Self::Kali),
            "whonix" => Ok(Self::Whonix),
            other => Err(ForgeError::InvalidInput(format!(
                "unknown profile '{other}' (tsurugi|sift|kali|whonix)"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsurugi_is_isolated_and_ready() {
        assert!(Profile::Tsurugi.is_isolated());
        assert!(Profile::Tsurugi.engine_ready());
        assert_eq!(Profile::Tsurugi.role(), Role::Isolated);
        assert!(!Profile::Kali.engine_ready());
        assert!(!Profile::Whonix.is_isolated());
    }

    #[test]
    fn parse_rejects_fedora_guest() {
        assert!(Profile::from_str("fedora").is_err());
        assert!(Profile::from_str("ubuntu").is_err());
    }
}
