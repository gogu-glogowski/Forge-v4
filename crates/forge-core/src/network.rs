//! Explicit lab networking: standard NAT or the legacy dedicated A/B topology.
use std::fmt;
use std::fs;
use std::io;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::str::FromStr;

use crate::error::{ForgeError, Result};
use crate::paths::ForgePaths;
use crate::profile::{METADATA_NS, WAN_NET, WHONIX_NET};
use crate::role::Role;

pub const MODE_PATH: &str = "/var/lib/forge/network-mode";
pub const STANDARD_NET: &str = "forge-standard";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Standard,
    Dedicated,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Standard => "standard",
            Self::Dedicated => "dedicated",
        })
    }
}
impl FromStr for Mode {
    type Err = ForgeError;
    fn from_str(text: &str) -> Result<Self> {
        match text.trim() {
            "standard" => Ok(Self::Standard),
            "dedicated" => Ok(Self::Dedicated),
            _ => Err(ForgeError::InvalidInput(
                "network mode must be standard or dedicated".to_owned(),
            )),
        }
    }
}
impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard (host connection via libvirt NAT)",
            Self::Dedicated => "Dedicated network (host A, guest uplink B)",
        }
    }
    pub fn load(paths: &ForgePaths) -> Result<Self> {
        if let Some(mode) = Self::read_at(Path::new(MODE_PATH))? {
            return Ok(mode);
        }
        // Never silently move an existing A/B lab onto the host uplink.
        let legacy = match fs::read_dir(paths.ownership_root()) {
            Ok(entries) => entries
                .collect::<std::result::Result<Vec<_>, _>>()?
                .iter()
                .any(|e| e.path().extension().is_some_and(|ext| ext == "json")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
        };
        Ok(Self::initial(legacy))
    }
    fn initial(legacy: bool) -> Self {
        if legacy {
            Self::Dedicated
        } else {
            Self::Standard
        }
    }
    pub fn installed() -> Result<Self> {
        // Older installed hooks and unmarked domains have A/B semantics.
        Ok(Self::read_at(Path::new(MODE_PATH))?.unwrap_or(Self::Dedicated))
    }
    fn read_at(path: &Path) -> Result<Option<Self>> {
        let meta = match fs::symlink_metadata(path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let parent = fs::symlink_metadata(
            path.parent()
                .ok_or_else(|| ForgeError::Host("network mode has no parent".to_owned()))?,
        )?;
        if !meta.is_file()
            || meta.file_type().is_symlink()
            || meta.uid() != 0
            || meta.permissions().mode() & 0o022 != 0
            || !parent.is_dir()
            || parent.file_type().is_symlink()
            || parent.uid() != 0
            || parent.permissions().mode() & 0o022 != 0
        {
            return Err(ForgeError::Host("network-mode and its directory must be root-owned, non-writable by other users and not symlinks".to_owned()));
        }
        Ok(Some(fs::read_to_string(path)?.parse()?))
    }
    pub fn install_script(self) -> String {
        format!(
            "printf '%s\\n' '{self}' > {MODE_PATH}.new\nchown root:root {MODE_PATH}.new\nchmod 0644 {MODE_PATH}.new\nmv -f {MODE_PATH}.new {MODE_PATH}\n"
        )
    }
}

pub fn declared_mode(xml: &str) -> Result<Mode> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| ForgeError::Role(format!("invalid network metadata XML: {e}")))?;
    let modes: Vec<_> = doc
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("metadata"))
        .flat_map(|n| n.children())
        .filter(|n| n.has_tag_name((METADATA_NS, "forge")))
        .flat_map(|n| n.children())
        .filter(|n| n.has_tag_name((METADATA_NS, "network-mode")))
        .collect();
    match modes.as_slice() {
        [] => Ok(Mode::Dedicated),
        [mode] => mode.text().unwrap_or_default().parse(),
        _ => Err(ForgeError::Role(
            "duplicate network-mode metadata".to_owned(),
        )),
    }
}

pub fn check_domain(xml: &str, role: Role, mode: Mode) -> Result<()> {
    if declared_mode(xml)? != mode {
        return Err(ForgeError::Role("VM network mode differs from the installed lab mode; stop it and run forge setup --network <mode>".to_owned()));
    }
    let doc = roxmltree::Document::parse(xml).map_err(|e| ForgeError::Role(e.to_string()))?;
    let devices = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name("devices"))
        .ok_or_else(|| ForgeError::Role("missing devices".to_owned()))?;
    let interfaces: Vec<_> = devices
        .children()
        .filter(|n| n.has_tag_name("interface"))
        .collect();
    let expected: &[&str] = match (role, mode) {
        (Role::Isolated, _) | (Role::OsintClearnet, Mode::Dedicated) => &[],
        (Role::OsintClearnet, Mode::Standard) => &[STANDARD_NET],
        (Role::WhonixGw, _) => &[WAN_NET, WHONIX_NET],
        (Role::WhonixWs, _) => &[WHONIX_NET],
    };
    if interfaces.len() != expected.len()
        || !interfaces.iter().zip(expected).all(|(nic, name)| {
            let sources: Vec<_> = nic
                .children()
                .filter(|n| n.has_tag_name("source"))
                .collect();
            nic.attribute("type") == Some("network")
                && sources.len() == 1
                && sources[0].attribute("network") == Some(*name)
        })
    {
        return Err(ForgeError::Role(format!(
            "unexpected NIC/NAT layout: {} in {mode} mode requires networks {:?} in this order",
            role.id(),
            expected
        )));
    }
    if devices
        .children()
        .filter(|n| n.has_tag_name("hostdev"))
        .any(|n| {
            mode == Mode::Standard
                || role != Role::OsintClearnet
                || n.attribute("type") != Some("usb")
        })
    {
        return Err(ForgeError::Role("USB uplink passthrough is only allowed for Kali in dedicated mode; PCI passthrough is forbidden".to_owned()));
    }
    Ok(())
}

pub fn standard_nat_xml(name: &str) -> String {
    let (bridge, address, dhcp) = if name == STANDARD_NET {
        (
            "virbr-forgestd",
            "192.168.124.1",
            "\n    <dhcp><range start='192.168.124.10' end='192.168.124.254'/></dhcp>",
        )
    } else {
        ("virbr-forgewan", "10.0.2.2", "")
    };
    format!(
        "<network>\n  <name>{name}</name>\n  <forward mode='nat'/>\n  <bridge name='{bridge}' stp='off' delay='0'/>\n  <ip address='{address}' netmask='255.255.255.0'>{dhcp}</ip>\n</network>\n"
    )
}

pub fn standard_nat_ok(xml: &str, name: &str) -> bool {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return false;
    };
    let root = doc.root_element();
    let expected_bridge = if name == STANDARD_NET {
        "virbr-forgestd"
    } else {
        "virbr-forgewan"
    };
    let expected_ip = if name == STANDARD_NET {
        "192.168.124.1"
    } else {
        "10.0.2.2"
    };
    let forwards: Vec<_> = root
        .children()
        .filter(|n| n.has_tag_name("forward"))
        .collect();
    let ips: Vec<_> = root.children().filter(|n| n.has_tag_name("ip")).collect();
    root.has_tag_name("network")
        && root
            .children()
            .any(|n| n.has_tag_name("name") && n.text() == Some(name))
        && root
            .children()
            .any(|n| n.has_tag_name("bridge") && n.attribute("name") == Some(expected_bridge))
        && forwards.len() == 1
        && forwards[0].attribute("mode") == Some("nat")
        && forwards[0].attribute("dev").is_none()
        && !forwards[0].children().any(|n| n.is_element())
        && ips.len() == 1
        && ips[0].attribute("address") == Some(expected_ip)
        && ips[0].attribute("netmask") == Some("255.255.255.0")
        && (name != STANDARD_NET
            || ips[0].children().filter(|n| n.has_tag_name("dhcp")).count() == 1)
        && !root
            .children()
            .any(|n| matches!(n.tag_name().name(), "route" | "virtualport" | "portgroup"))
}

pub fn private_ok(xml: &str) -> bool {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return false;
    };
    let root = doc.root_element();
    root.has_tag_name("network")
        && root
            .children()
            .any(|n| n.has_tag_name("name") && n.text() == Some(WHONIX_NET))
        && root
            .children()
            .any(|n| n.has_tag_name("bridge") && n.attribute("name") == Some("virbr-forgewx"))
        && !root.children().any(|n| n.has_tag_name("forward"))
        && !root.children().any(|n| {
            matches!(
                n.tag_name().name(),
                "ip" | "route" | "dns" | "virtualport" | "portgroup"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_labs_default_to_standard_but_existing_labs_keep_dedicated() {
        assert_eq!(Mode::initial(false), Mode::Standard);
        assert_eq!(Mode::initial(true), Mode::Dedicated);
        assert!("automatic".parse::<Mode>().is_err());
    }
    #[test]
    fn standard_nat_rejects_pinned_and_routed_uplinks() {
        for name in [STANDARD_NET, WAN_NET] {
            let xml = standard_nat_xml(name);
            assert!(standard_nat_ok(&xml, name));
            assert!(!standard_nat_ok(
                &xml.replace("mode='nat'", "mode='route'"),
                name
            ));
            assert!(!standard_nat_ok(
                &xml.replace("mode='nat'", "mode='nat' dev='eth0'"),
                name
            ));
            assert!(!standard_nat_ok(
                &xml.replace(
                    "</network>",
                    "<ip family='ipv6' address='fd00::1' prefix='64'/></network>"
                ),
                name
            ));
        }
    }
    #[test]
    fn private_network_never_has_host_ip_dhcp_or_forwarding() {
        let xml = "<network><name>forge-whonix</name><bridge name='virbr-forgewx'/></network>";
        assert!(private_ok(xml));
        assert!(!private_ok(
            &xml.replace("</network>", "<forward mode='nat'/></network>")
        ));
        assert!(!private_ok(&xml.replace(
            "</network>",
            "<ip address='10.152.152.1'/></network>"
        )));
    }
    #[test]
    fn legacy_metadata_is_dedicated_and_standard_requires_explicit_metadata() {
        let legacy = "<domain><metadata><forge:forge xmlns:forge='https://github.com/gogu-glogowski/Forge-v4'/></metadata><devices/></domain>";
        assert_eq!(declared_mode(legacy).unwrap(), Mode::Dedicated);
        assert!(check_domain(legacy, Role::Isolated, Mode::Dedicated).is_ok());
        assert!(check_domain(legacy, Role::Isolated, Mode::Standard).is_err());
        let invalid = legacy.replace(
            "<devices/>",
            "<!-- <forge:network-mode>standard</forge:network-mode> --><devices/>",
        );
        assert_eq!(declared_mode(&invalid).unwrap(), Mode::Dedicated);
    }

    #[test]
    fn untrusted_mode_file_and_symlink_are_rejected() {
        let dir = std::env::temp_dir().join(format!("forge-mode-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("network-mode");
        fs::write(&path, "standard").unwrap();
        assert!(Mode::read_at(&path).is_err());
        let link = dir.join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(Mode::read_at(&link).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
