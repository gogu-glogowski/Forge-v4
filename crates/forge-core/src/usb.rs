use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{ForgeError, Result};
use crate::paths::ForgePaths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UsbId {
    pub vendor: u16,
    pub product: u16,
}

impl UsbId {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        let (v, p) = s.split_once(':').ok_or_else(|| {
            ForgeError::InvalidInput(format!(
                "dongle B id '{s}' must be vendor:product hex (e.g. 0b95:1790)"
            ))
        })?;
        let vendor = u16::from_str_radix(v.trim().trim_start_matches("0x"), 16)
            .map_err(|_| ForgeError::InvalidInput(format!("bad USB vendor '{v}'")))?;
        let product = u16::from_str_radix(p.trim().trim_start_matches("0x"), 16)
            .map_err(|_| ForgeError::InvalidInput(format!("bad USB product '{p}'")))?;
        Ok(Self { vendor, product })
    }

    #[must_use]
    pub fn display(self) -> String {
        format!("{:04x}:{:04x}", self.vendor, self.product)
    }

    #[must_use]
    pub fn hostdev_xml(self) -> String {
        format!(
            "<hostdev mode='subsystem' type='usb' managed='yes'>\n  <source>\n    <vendor id='0x{:04x}'/>\n    <product id='0x{:04x}'/>\n  </source>\n</hostdev>\n",
            self.vendor, self.product
        )
    }
}

#[derive(Debug, Clone)]
pub struct UsbNet {
    pub id: UsbId,
    pub label: String,
}

#[must_use]
pub fn ids_in_xml(xml: &str) -> Vec<UsbId> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<vendor") {
        let chunk = &rest[i..];
        let vendor = attr_hex(chunk, "id");
        let product = chunk
            .find("<product")
            .and_then(|j| attr_hex(&chunk[j..], "id"));
        if let (Some(vendor), Some(product)) = (vendor, product) {
            out.push(UsbId { vendor, product });
        }
        rest = &chunk[1..];
    }
    out
}

fn attr_hex(chunk: &str, name: &str) -> Option<u16> {
    for quote in ['\'', '"'] {
        let pat = format!("{name}={quote}");
        if let Some(i) = chunk.find(&pat) {
            let rest = &chunk[i + pat.len()..];
            let end = rest.find(quote)?;
            let raw = rest[..end].trim().trim_start_matches("0x");
            return u16::from_str_radix(raw, 16).ok();
        }
    }
    None
}

pub fn load_pin(paths: &ForgePaths) -> Result<Option<UsbId>> {
    if let Ok(from_env) = std::env::var("FORGE_DONGLE_B") {
        let from_env = from_env.trim();
        if !from_env.is_empty() {
            return Ok(Some(UsbId::parse(from_env)?));
        }
    }
    let file = dongle_file(paths);
    if !file.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&file)?;
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'));
    match line {
        Some(line) => {
            let id = line.split('#').next().unwrap_or(line).trim();
            Ok(Some(UsbId::parse(id)?))
        }
        None => Ok(None),
    }
}

#[must_use]
pub fn dongle_file(paths: &ForgePaths) -> PathBuf {
    paths.meta.join("dongle-b")
}

/// USB devices that currently expose a network interface (dongle B candidates).
pub fn scan_usb_net() -> Result<Vec<UsbNet>> {
    scan_usb_net_root(Path::new("/sys/bus/usb/devices"))
}

pub fn scan_usb_net_root(root: &Path) -> Result<Vec<UsbNet>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.contains(':') {
            continue;
        }
        if !usb_device_has_net(&path) {
            continue;
        }
        let Some(id) = read_id(&path) else {
            continue;
        };
        let manuf = read_trim(&path.join("manufacturer")).unwrap_or_default();
        let product = read_trim(&path.join("product")).unwrap_or_default();
        let label = format!("{manuf} {product}").trim().to_owned();
        out.push(UsbNet { id, label });
    }
    out.sort_by_key(|d| d.id.display());
    out.dedup_by_key(|d| d.id);
    Ok(out)
}

fn usb_device_has_net(dev: &Path) -> bool {
    if dev.join("net").is_dir() {
        return true;
    }
    let Ok(entries) = fs::read_dir(dev) else {
        return false;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.join("net").is_dir() {
            return true;
        }
    }
    false
}

fn read_id(dev: &Path) -> Option<UsbId> {
    let vendor = u16::from_str_radix(read_trim(&dev.join("idVendor"))?.trim(), 16).ok()?;
    let product = u16::from_str_radix(read_trim(&dev.join("idProduct"))?.trim(), 16).ok()?;
    Some(UsbId { vendor, product })
}

fn read_trim(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// Resolve which USB net device is dongle B, if it is plugged in.
///
/// Human plugs the cable. Forge does not steal it from another VM.
pub fn resolve_plugged(paths: &ForgePaths) -> Result<Resolve> {
    let plugged = scan_usb_net()?;
    let pin = load_pin(paths)?;
    match (pin, plugged.as_slice()) {
        (Some(pin), devices) => {
            if devices.iter().any(|d| d.id == pin) {
                Ok(Resolve::Plugged(pin))
            } else {
                Ok(Resolve::PinnedMissing(pin))
            }
        }
        (None, []) => Ok(Resolve::None),
        (None, [one]) => Ok(Resolve::Plugged(one.id)),
        (None, many) => Err(ForgeError::Role(format!(
            "several USB net devices; pin dongle B (FORGE_DONGLE_B=vvvv:pppp or {}): {}",
            dongle_file(paths).display(),
            many.iter()
                .map(|d| format!("{} {}", d.id.display(), d.label))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolve {
    Plugged(UsbId),
    PinnedMissing(UsbId),
    None,
}

#[must_use]
pub fn format_dev_list(paths: &ForgePaths) -> String {
    let pin = load_pin(paths).ok().flatten();
    let plugged = scan_usb_net().unwrap_or_default();
    let mut out = String::from("Dongle B (USB net passthrough)\n");
    match pin {
        Some(id) => out.push_str(&format!(
            "pin: {} (FORGE_DONGLE_B or {})\n",
            id.display(),
            dongle_file(paths).display()
        )),
        None => out.push_str(&format!(
            "pin: none — auto if exactly one USB net device; else set FORGE_DONGLE_B or {}\n",
            dongle_file(paths).display()
        )),
    }
    if plugged.is_empty() {
        out.push_str("plugged: none\n");
    } else {
        out.push_str("plugged:\n");
        for dev in plugged {
            let mark = if pin == Some(dev.id) { "  [B]" } else { "" };
            out.push_str(&format!("  {}  {}{mark}\n", dev.id.display(), dev.label));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_xml() {
        let id = UsbId::parse("0b95:1790").unwrap();
        assert_eq!(id.display(), "0b95:1790");
        let xml = id.hostdev_xml();
        assert!(xml.contains("type='usb'"));
        assert!(xml.contains("0x0b95"));
        assert!(xml.contains("0x1790"));
        assert_eq!(ids_in_xml(&xml), vec![id]);
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(UsbId::parse("usb").is_err());
        assert!(UsbId::parse("zzzz:0001").is_err());
    }

    #[test]
    fn scan_fixture_sysfs() {
        let root = std::env::temp_dir().join(format!("forge-usb-{}", uuid::Uuid::new_v4()));
        let dev = root.join("3-4");
        fs::create_dir_all(dev.join("3-4:1.0").join("net").join("enp0s20")).unwrap();
        fs::write(dev.join("idVendor"), "0b95\n").unwrap();
        fs::write(dev.join("idProduct"), "1790\n").unwrap();
        fs::write(dev.join("manufacturer"), "ASIX\n").unwrap();
        fs::write(dev.join("product"), "USB Ethernet\n").unwrap();
        let found = scan_usb_net_root(&root).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id.display(), "0b95:1790");
        let _ = fs::remove_dir_all(root);
    }
}
