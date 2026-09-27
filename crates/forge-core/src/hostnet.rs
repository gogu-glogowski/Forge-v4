//! Cable A stays on the host. Cable B (any USB network device) does not.
//!
//! A is a PCI Ethernet NIC. B is a NIC whose sysfs device sits under a USB bus,
//! including a USB Wi-Fi dongle — that is the same slot, not a third uplink.
//! Onboard Wi-Fi is neither: doctor warns if it becomes the default route.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::cmd;
use crate::error::{ForgeError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    PciEthernet,
    UsbNet,
    Wireless,
    Other,
}

#[derive(Debug, Clone)]
struct Nic {
    name: String,
    mac: String,
    kind: Kind,
    carrier: bool,
}

#[derive(Debug, Clone)]
struct Snapshot {
    nics: Vec<Nic>,
    defaults: BTreeSet<String>,
    addressed: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct Profile {
    uuid: String,
    iface: Option<String>,
    mac: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Audit {
    pub summary: String,
    pub held: Vec<String>,
    pub foreign_defaults: Vec<String>,
    /// USB NIC that owns the host default route. That is Fedora using cable B.
    pub usb_defaults: Vec<String>,
}

pub fn audit() -> Result<Audit> {
    let snap = observe()?;
    Ok(Audit {
        summary: render(&snap),
        held: snap
            .usb_held()
            .into_iter()
            .map(|n| n.name.clone())
            .collect(),
        foreign_defaults: snap.foreign_defaults(),
        usb_defaults: snap
            .nics
            .iter()
            .filter(|nic| nic.kind == Kind::UsbNet && snap.defaults.contains(&nic.name))
            .map(|nic| nic.name.clone())
            .collect(),
    })
}

const B_CONNECTION: &str = "forge-b";

/// Take a DHCP lease on dongle B without making it Fedora's default route,
/// and send only the gateway NAT range (10.0.2.0/24) out that NIC.
pub fn lease_usb(iface: &str) -> Result<String> {
    if !iface_ok(iface) {
        return Err(ForgeError::Host(format!(
            "refusing to configure unexpected interface '{iface}'"
        )));
    }
    let snap = observe()?;
    let Some(nic) = snap.nics.iter().find(|nic| nic.name == iface) else {
        return Err(ForgeError::Host(format!(
            "dongle B interface {iface} is not up"
        )));
    };
    if nic.kind != Kind::UsbNet {
        return Err(ForgeError::Host(format!(
            "{iface} is not dongle B; cable A stays on Fedora"
        )));
    }
    release_usb()?;
    nm(&[
        "connection",
        "add",
        "type",
        "ethernet",
        "ifname",
        iface,
        "con-name",
        B_CONNECTION,
        "ipv4.method",
        "auto",
        "ipv4.never-default",
        "yes",
        "ipv6.method",
        "disabled",
        "connection.autoconnect",
        "no",
    ])?;
    nm(&[
        "connection",
        "modify",
        B_CONNECTION,
        "ipv4.routing-rules",
        "priority 100 from 10.0.2.0/24 table 100",
    ])?;
    nm(&["connection", "up", B_CONNECTION])?;
    let options = nm(&["-g", "DHCP4.OPTION", "device", "show", iface]).unwrap_or_default();
    let Some(gw) = dhcp_router(&options) else {
        release_usb()?;
        return Err(ForgeError::Host(format!(
            "dongle B ({iface}) got no IPv4 router from DHCP"
        )));
    };
    nm(&[
        "connection",
        "modify",
        B_CONNECTION,
        "ipv4.routes",
        &format!("0.0.0.0/0 {gw} table=100"),
    ])?;
    nm(&["connection", "up", B_CONNECTION])?;
    let table = cmd::run_checked("ip", &["route", "show", "table", "100"]).unwrap_or_default();
    if !policy_default_via(&table, iface, &gw) {
        release_usb()?;
        return Err(ForgeError::Host(format!(
            "dongle B ({iface}) did not install the 10.0.2.0/24 route via {gw}"
        )));
    }
    let after = observe()?;
    if after.defaults.contains(iface) {
        release_usb()?;
        return Err(ForgeError::Host(format!(
            "dongle B ({iface}) became Fedora's default route; refused"
        )));
    }
    Ok(gw.to_owned())
}

/// Fedora's libvirt zone drops forwarded packets until this is on. The gateway
/// NAT uses that forward path, so a fresh install must turn it on itself.
pub fn allow_libvirt_forward() -> Result<()> {
    if !cmd::exists("firewall-cmd") {
        return Ok(());
    }
    match firewall_state() {
        Firewalld::Absent | Firewalld::Stopped => return Ok(()),
        Firewalld::Running => {}
    }
    if libvirt_forwards() == Some(true) {
        let _ = firewall_cmd(&["--permanent", "--zone=libvirt", "--add-forward"]);
        return Ok(());
    }
    firewall_cmd(&["--zone=libvirt", "--add-forward"])?;
    firewall_cmd(&["--permanent", "--zone=libvirt", "--add-forward"])?;
    if libvirt_forwards() != Some(true) {
        return Err(ForgeError::Host(
            "firewalld zone libvirt still does not forward; the gateway cannot reach the router"
                .to_owned(),
        ));
    }
    Ok(())
}

#[must_use]
pub fn libvirt_forwards() -> Option<bool> {
    let output = cmd::command("firewall-cmd")
        .args(["--zone=libvirt", "--query-forward"])
        .output()
        .ok()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if text.to_ascii_lowercase().contains("yes") {
        Some(true)
    } else if text.to_ascii_lowercase().contains("no") {
        Some(false)
    } else {
        None
    }
}

pub fn release_usb() -> Result<()> {
    let _ = nm(&["connection", "down", B_CONNECTION]);
    match nm(&["connection", "delete", B_CONNECTION]) {
        Ok(_) => Ok(()),
        Err(error) => {
            let text = error.to_string().to_ascii_lowercase();
            if text.contains("unknown") || text.contains("not found") || text.contains("nie znaleziono")
            {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

fn policy_default_via(dump: &str, iface: &str, gw: &str) -> bool {
    dump.lines().any(|line| {
        let parts: Vec<&str> = line.split_whitespace().collect();
        parts.first() == Some(&"default")
            && parts.windows(2).any(|pair| pair == ["via", gw])
            && parts.windows(2).any(|pair| pair == ["dev", iface])
    })
}

enum Firewalld {
    Absent,
    Stopped,
    Running,
}

fn firewall_state() -> Firewalld {
    let Ok(output) = cmd::command("firewall-cmd").arg("--state").output() else {
        return Firewalld::Absent;
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();
    if text.contains("running") && !text.contains("not running") {
        Firewalld::Running
    } else {
        Firewalld::Stopped
    }
}

fn firewall_cmd(args: &[&str]) -> Result<String> {
    cmd::run_checked("firewall-cmd", args)
}

fn dhcp_router(options: &str) -> Option<String> {
    for part in options.split('|') {
        let part = part.trim();
        let Some(rest) = part.strip_prefix("routers") else {
            continue;
        };
        let value = rest.trim().trim_start_matches('=').trim();
        if ipv4_ok(value) {
            return Some(value.to_owned());
        }
    }
    None
}

fn ipv4_ok(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty() && part.len() <= 3 && part.chars().all(|ch| ch.is_ascii_digit()) && part.parse::<u8>().is_ok()
        })
}

/// Drop every USB NIC from the host. PCI Ethernet (cable A) is left up; if its
/// active profile was marked never-default, that flag is cleared so Fedora
/// does not lose its only uplink when B disconnects.
pub fn enforce() -> Result<String> {
    let before = observe()?;
    if !before.nics.iter().any(|nic| nic.kind == Kind::UsbNet) {
        return Ok(render(&before));
    }
    clear_never_default_on_host(&before)?;
    quarantine_usb(&before)?;
    let after = wait_until_usb_clear()?;
    let text = render(&after);
    if after.usb_held().is_empty() {
        Ok(text)
    } else {
        Err(ForgeError::Host(format!(
            "cable B is still configured on the host:\n{text}"
        )))
    }
}

fn observe() -> Result<Snapshot> {
    let nics = read_nics(Path::new("/sys/class/net"))?;
    let defaults = parse_default_devs(&route_dump()?);
    let addressed = parse_addressed(&addr_dump()?);
    Ok(Snapshot {
        nics,
        defaults,
        addressed,
    })
}

fn route_dump() -> Result<String> {
    let v4 = cmd::run_checked("ip", &["-4", "route", "show", "default"])?;
    let v6 = cmd::run_checked("ip", &["-6", "route", "show", "default"]).unwrap_or_default();
    Ok(format!("{v4}\n{v6}"))
}

fn addr_dump() -> Result<String> {
    cmd::run_checked("ip", &["-o", "addr", "show"])
}

fn wait_until_usb_clear() -> Result<Snapshot> {
    let mut last = observe()?;
    for _ in 0..15 {
        if last.usb_held().is_empty() {
            return Ok(last);
        }
        thread::sleep(Duration::from_millis(200));
        last = observe()?;
    }
    Ok(last)
}

fn clear_never_default_on_host(snap: &Snapshot) -> Result<()> {
    let profiles = ethernet_profiles()?;
    for nic in &snap.nics {
        if nic.kind != Kind::PciEthernet || !nic.carrier {
            continue;
        }
        let Some(uuid) = profile_for_host(nic, &profiles)? else {
            continue;
        };
        let flags = nm(&[
            "-g",
            "ipv4.never-default,ipv6.never-default",
            "connection",
            "show",
            &uuid,
        ])?;
        if flags.lines().any(|line| line.trim() == "yes") {
            nm(&[
                "connection",
                "modify",
                &uuid,
                "ipv4.never-default",
                "no",
                "ipv6.never-default",
                "no",
            ])?;
            let _ = nm(&["device", "reapply", &nic.name]);
        }
    }
    Ok(())
}

fn quarantine_usb(snap: &Snapshot) -> Result<()> {
    let profiles = ethernet_profiles()?;
    let pci_names: BTreeSet<&str> = snap
        .nics
        .iter()
        .filter(|nic| nic.kind == Kind::PciEthernet)
        .map(|nic| nic.name.as_str())
        .collect();
    for nic in snap.nics.iter().filter(|nic| nic.kind == Kind::UsbNet) {
        if !iface_ok(&nic.name) {
            return Err(ForgeError::Host(format!(
                "refusing to touch unexpected interface name '{}'",
                nic.name
            )));
        }
        for profile in profiles_for_usb(nic, &profiles, &pci_names) {
            nm(&[
                "connection",
                "modify",
                &profile.uuid,
                "connection.autoconnect",
                "no",
                "ipv4.never-default",
                "yes",
                "ipv6.never-default",
                "yes",
            ])?;
            if device_is_active(&nic.name)? {
                disconnect(&nic.name)?;
            }
            nm(&[
                "connection",
                "modify",
                &profile.uuid,
                "ipv4.method",
                "disabled",
                "ipv6.method",
                "disabled",
            ])?;
        }
        if device_is_active(&nic.name)? {
            disconnect(&nic.name)?;
        }
    }
    Ok(())
}

fn profile_for_host(nic: &Nic, profiles: &[Profile]) -> Result<Option<String>> {
    if let Some(uuid) = active_uuid(&nic.name)? {
        return Ok(Some(uuid));
    }
    Ok(profiles
        .iter()
        .find(|profile| profile.iface.as_deref() == Some(nic.name.as_str()))
        .map(|profile| profile.uuid.clone()))
}

fn profiles_for_usb<'a>(
    nic: &Nic,
    profiles: &'a [Profile],
    pci_names: &BTreeSet<&str>,
) -> Vec<&'a Profile> {
    let mac = nic.mac.to_ascii_lowercase();
    profiles
        .iter()
        .filter(|profile| {
            if profile
                .iface
                .as_deref()
                .is_some_and(|name| pci_names.contains(name))
            {
                return false;
            }
            profile.iface.as_deref() == Some(nic.name.as_str())
                || profile.mac.as_deref() == Some(mac.as_str())
        })
        .collect()
}

fn ethernet_profiles() -> Result<Vec<Profile>> {
    let text = nm(&["-t", "-f", "UUID,TYPE", "connection", "show"])?;
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((uuid, kind)) = line.split_once(':') else {
            continue;
        };
        if kind != "802-3-ethernet" || !uuid_ok(uuid) {
            continue;
        }
        let bind = nm(&[
            "-g",
            "connection.interface-name,802-3-ethernet.mac-address",
            "connection",
            "show",
            uuid,
        ])?;
        let mut lines = bind.lines();
        out.push(Profile {
            uuid: uuid.to_owned(),
            iface: nonempty(lines.next()),
            mac: nonempty(lines.next()).map(|value| value.to_ascii_lowercase()),
        });
    }
    Ok(out)
}

fn active_uuid(iface: &str) -> Result<Option<String>> {
    let text = nm(&["-g", "GENERAL.CON-UUID", "device", "show", iface]).unwrap_or_default();
    Ok(nonempty(text.lines().next()).filter(|uuid| uuid_ok(uuid)))
}

fn device_is_active(iface: &str) -> Result<bool> {
    let text = nm(&["-g", "GENERAL.STATE", "device", "show", iface]).unwrap_or_default();
    let state = text.to_ascii_lowercase();
    Ok(state.contains("connected") && !state.contains("disconnected"))
}

fn disconnect(iface: &str) -> Result<()> {
    match nm(&["device", "disconnect", iface]) {
        Ok(_) => Ok(()),
        Err(error) => {
            let text = error.to_string().to_ascii_lowercase();
            if text.contains("not active") || text.contains("not an active") {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

fn nm(args: &[&str]) -> Result<String> {
    if !cmd::exists("nmcli") {
        return Err(ForgeError::Host(
            "nmcli is required to keep cable B off the host (NetworkManager)".to_owned(),
        ));
    }
    cmd::run_checked("nmcli", args)
}

fn read_nics(root: &Path) -> Result<Vec<Nic>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut nics = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name == "lo" || !iface_ok(name) {
            continue;
        }
        let device = fs::canonicalize(path.join("device")).ok();
        let wireless = path.join("wireless").exists() || path.join("phy80211").exists();
        let kind = kind_of(device.as_deref(), wireless);
        if kind == Kind::Other {
            continue;
        }
        let mac = fs::read_to_string(path.join("address"))
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let carrier = fs::read_to_string(path.join("carrier"))
            .unwrap_or_default()
            .trim()
            == "1";
        nics.push(Nic {
            name: name.to_owned(),
            mac,
            kind,
            carrier,
        });
    }
    nics.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(nics)
}

fn kind_of(device: Option<&Path>, wireless: bool) -> Kind {
    match device {
        Some(path) if path_is_usb(path) => Kind::UsbNet,
        _ if wireless => Kind::Wireless,
        Some(path) if path_is_pci(path) => Kind::PciEthernet,
        _ => Kind::Other,
    }
}

fn path_is_usb(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        name.len() > 3 && name.starts_with("usb") && name[3..].chars().all(|ch| ch.is_ascii_digit())
    })
}

fn path_is_pci(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        name.starts_with("pci") || name.starts_with("0000:")
    })
}

fn render(snap: &Snapshot) -> String {
    let mut out = String::from("cable A (PCI ethernet, host only):\n");
    let host: Vec<&Nic> = snap
        .nics
        .iter()
        .filter(|nic| nic.kind == Kind::PciEthernet)
        .collect();
    if host.is_empty() {
        out.push_str("  none\n");
    } else {
        for nic in host {
            out.push_str(&format!("  {}\n", nic_line(snap, nic)));
        }
    }
    out.push_str("cable B (USB net, VMs only):\n");
    let usb: Vec<&Nic> = snap
        .nics
        .iter()
        .filter(|nic| nic.kind == Kind::UsbNet)
        .collect();
    if usb.is_empty() {
        out.push_str("  not plugged\n");
    } else {
        for nic in usb {
            out.push_str(&format!("  {}\n", nic_line(snap, nic)));
        }
    }
    let foreign = snap.foreign_defaults();
    if foreign.is_empty() {
        out.push_str("other default route: none\n");
    } else {
        out.push_str(&format!("other default route: {}\n", foreign.join(", ")));
    }
    out
}

fn nic_line(snap: &Snapshot, nic: &Nic) -> String {
    let route = if snap.defaults.contains(&nic.name) {
        "default"
    } else if snap.addressed.contains(&nic.name) {
        "host holds address"
    } else if nic.carrier {
        "link up, not configured on the host"
    } else {
        "down, not configured on the host"
    };
    format!("{}  {}  {route}", nic.name, nic.mac)
}

impl Snapshot {
    fn usb_held(&self) -> Vec<&Nic> {
        self.nics
            .iter()
            .filter(|nic| nic.kind == Kind::UsbNet)
            .filter(|nic| self.defaults.contains(&nic.name) || self.addressed.contains(&nic.name))
            .collect()
    }

    fn foreign_defaults(&self) -> Vec<String> {
        let known: BTreeSet<&str> = self
            .nics
            .iter()
            .filter(|nic| matches!(nic.kind, Kind::PciEthernet | Kind::UsbNet))
            .map(|nic| nic.name.as_str())
            .collect();
        self.defaults
            .iter()
            .filter(|dev| !known.contains(dev.as_str()))
            .cloned()
            .collect()
    }
}

fn parse_default_devs(dump: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in dump.lines() {
        let Some(rest) = line.trim().strip_prefix("default ") else {
            continue;
        };
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if let Some(index) = parts.iter().position(|part| *part == "dev") {
            if let Some(dev) = parts.get(index + 1) {
                if iface_ok(dev) {
                    out.insert((*dev).to_owned());
                }
            }
        }
    }
    out
}

fn parse_addressed(dump: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in dump.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let name = parts[1];
        if !iface_ok(name) {
            continue;
        }
        match parts[2] {
            "inet" => {
                out.insert(name.to_owned());
            }
            "inet6" => {
                let global = parts
                    .iter()
                    .position(|part| *part == "scope")
                    .and_then(|index| parts.get(index + 1))
                    .is_some_and(|scope| *scope == "global");
                if global {
                    out.insert(name.to_owned());
                }
            }
            _ => {}
        }
    }
    out
}

fn iface_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 15
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.')
}

fn uuid_ok(uuid: &str) -> bool {
    let mut groups = uuid.split('-');
    let widths = [8, 4, 4, 4, 12];
    widths.iter().all(|width| {
        groups.next().is_some_and(|part| {
            part.len() == *width && part.chars().all(|ch| ch.is_ascii_hexdigit())
        })
    }) && groups.next().is_none()
}

fn nonempty(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() || value == "--" {
        None
    } else {
        Some(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_controller_path_is_dongle_not_cable_a() {
        let usb = Path::new("/sys/devices/pci0000:00/0000:00:01.2/0000:02:00.0/usb2/2-1/2-1:1.0");
        let pci = Path::new("/sys/devices/pci0000:00/0000:00:1c.6/0000:0a:00.0");
        assert!(path_is_usb(usb));
        assert!(path_is_pci(usb));
        assert_eq!(kind_of(Some(usb), false), Kind::UsbNet);
        assert!(!path_is_usb(pci));
        assert_eq!(kind_of(Some(pci), false), Kind::PciEthernet);
        assert_eq!(
            kind_of(
                Some(Path::new("/sys/devices/pci0000:00/0000:09:00.0")),
                true
            ),
            Kind::Wireless
        );
    }

    #[test]
    fn addressed_ignores_link_local() {
        let dump = "\
2: enp10s0 inet 192.168.50.110/24 brd 192.168.50.255 scope global dynamic noprefixroute enp10s0
2: enp10s0 inet6 fe80::1/64 scope link noprefixroute
3: enp2s0 inet6 fe80::2/64 scope link
3: enp2s0 inet6 2001:db8::2/64 scope global
";
        let set = parse_addressed(dump);
        assert!(set.contains("enp10s0"));
        assert!(set.contains("enp2s0"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn defaults_and_hold() {
        let routes = "\
default via 192.168.100.1 dev enp2s0 proto dhcp src 192.168.100.27 metric 100
default via 192.168.50.1 dev enp10s0 proto dhcp metric 101
";
        let defaults = parse_default_devs(routes);
        assert!(defaults.contains("enp2s0"));
        assert!(defaults.contains("enp10s0"));
        let snap = Snapshot {
            nics: vec![
                Nic {
                    name: "enp10s0".to_owned(),
                    mac: "10:7c:61:72:eb:f9".to_owned(),
                    kind: Kind::PciEthernet,
                    carrier: true,
                },
                Nic {
                    name: "enp2s0".to_owned(),
                    mac: "00:e0:4c:68:08:04".to_owned(),
                    kind: Kind::UsbNet,
                    carrier: true,
                },
            ],
            defaults,
            addressed: BTreeSet::from(["enp10s0".to_owned(), "enp2s0".to_owned()]),
        };
        assert_eq!(snap.usb_held().len(), 1);
        assert!(snap.foreign_defaults().is_empty());
        let text = render(&snap);
        assert!(text.contains("enp2s0"));
        assert!(text.contains("default"));
        assert!(text.contains("host holds address") || text.contains("default"));
    }

    #[test]
    fn policy_route_matches_gateway_and_device() {
        let dump = "default via 192.168.100.1 dev enp2s0f0u1 proto static metric 20101\n";
        assert!(policy_default_via(dump, "enp2s0f0u1", "192.168.100.1"));
        assert!(!policy_default_via(dump, "enp10s0", "192.168.100.1"));
        assert!(!policy_default_via("", "enp2s0f0u1", "192.168.100.1"));
    }

    #[test]
    fn dhcp_router_ignores_the_requested_flag() {
        let options = "requested_routers = 1 | routers = 192.168.100.1 | subnet_mask = 255.255.255.0";
        assert_eq!(dhcp_router(options).as_deref(), Some("192.168.100.1"));
        assert!(dhcp_router("requested_routers = 1").is_none());
    }

    #[test]
    fn uuid_and_iface_names() {
        assert!(uuid_ok("dd6f8098-1b6c-31e4-95f8-25a09b4c4fbb"));
        assert!(!uuid_ok("not-a-uuid"));
        assert!(iface_ok("enp2s0f0u1"));
        assert!(!iface_ok("enp2s0f0u1;rm"));
        assert!(!iface_ok(""));
    }

    #[test]
    fn read_nics_from_fixture() {
        let root = std::env::temp_dir().join(format!("forge-net-{}", uuid::Uuid::new_v4()));
        write_nic(
            &root,
            "enp10s0",
            &root.join("sys").join("pci0000:00").join("0000:0a:00.0"),
            "10:7c:61:72:eb:f9",
        );
        write_nic(
            &root,
            "enp2s0",
            &root
                .join("sys")
                .join("pci0000:00")
                .join("usb2")
                .join("2-1:1.0"),
            "00:e0:4c:68:08:04",
        );
        let nics = read_nics(&root).unwrap();
        assert_eq!(nics.len(), 2);
        assert_eq!(nics[0].kind, Kind::PciEthernet);
        assert_eq!(nics[1].kind, Kind::UsbNet);
        let _ = fs::remove_dir_all(root);
    }

    fn write_nic(root: &Path, name: &str, target: &Path, mac: &str) {
        fs::create_dir_all(target).unwrap();
        let nic = root.join(name);
        fs::create_dir_all(&nic).unwrap();
        std::os::unix::fs::symlink(target, nic.join("device")).unwrap();
        fs::write(nic.join("address"), format!("{mac}\n")).unwrap();
        fs::write(nic.join("carrier"), "1\n").unwrap();
    }
}
