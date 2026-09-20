use crate::error::{ForgeError, Result};
use crate::profile::{METADATA_NS, WHONIX_NET};
use crate::role::Role;

#[must_use]
pub fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[derive(Debug, Clone)]
pub struct DomainSpec<'a> {
    pub name: &'a str,
    pub uuid: &'a str,
    pub profile: &'a str,
    pub role: Role,
    pub overlay: &'a str,
    pub base: &'a str,
    pub base_digest: &'a str,
    pub memory_mib: u32,
    pub vcpus: u32,
}

#[must_use]
pub fn domain_xml(spec: &DomainSpec<'_>) -> String {
    let name = escape(spec.name);
    let uuid = escape(spec.uuid);
    let overlay = escape(spec.overlay);
    let base = escape(spec.base);
    let profile = escape(spec.profile);
    let digest = escape(spec.base_digest);
    let role = spec.role.id();
    let nets = network_xml(spec.role);
    format!(
        "\
<domain type='kvm'>
  <name>{name}</name>
  <uuid>{uuid}</uuid>
  <metadata>
    <forge:forge xmlns:forge='{METADATA_NS}'>
      <forge:profile>{profile}</forge:profile>
      <forge:role>{role}</forge:role>
      <forge:base-digest>{digest}</forge:base-digest>
      <forge:overlay>{overlay}</forge:overlay>
    </forge:forge>
  </metadata>
  <memory unit='MiB'>{memory}</memory>
  <currentMemory unit='MiB'>{memory}</currentMemory>
  <vcpu placement='static'>{vcpus}</vcpu>
  <os>
    <type arch='x86_64' machine='q35'>hvm</type>
    <boot dev='hd'/>
  </os>
  <features>
    <acpi/>
    <apic/>
  </features>
  <cpu mode='host-passthrough' check='none'/>
  <clock offset='utc'/>
  <on_poweroff>destroy</on_poweroff>
  <on_reboot>restart</on_reboot>
  <on_crash>destroy</on_crash>
  <devices>
    <disk type='file' device='disk'>
      <driver name='qemu' type='qcow2' discard='unmap'/>
      <source file='{overlay}'/>
      <backingStore type='file'>
        <format type='qcow2'/>
        <source file='{base}'>
          <seclabel model='selinux' relabel='no'/>
        </source>
        <backingStore/>
      </backingStore>
      <target dev='vda' bus='virtio'/>
    </disk>
{nets}    <serial type='pty'>
      <target port='0'/>
    </serial>
    <console type='pty'>
      <target type='serial' port='0'/>
    </console>
    <channel type='spicevmc'>
      <target type='virtio' name='com.redhat.spice.0'/>
    </channel>
    <input type='tablet' bus='usb'/>
    <graphics type='spice' autoport='yes'>
      <listen type='address' address='127.0.0.1'/>
    </graphics>
    <video>
      <model type='virtio' heads='1' primary='yes'/>
    </video>
    <controller type='usb' model='qemu-xhci'/>
    <rng model='virtio'>
      <backend model='random'>/dev/urandom</backend>
    </rng>
  </devices>
</domain>
",
        memory = spec.memory_mib,
        vcpus = spec.vcpus,
    )
}

fn network_xml(role: Role) -> String {
    match role {
        Role::Isolated => String::new(),
        Role::WhonixWs => format!(
            "    <interface type='network'>
      <source network='{WHONIX_NET}'/>
      <model type='virtio'/>
    </interface>
"
        ),
        Role::WhonixGw => format!(
            "    <interface type='network'>
      <source network='{WHONIX_NET}'/>
      <model type='virtio'/>
    </interface>
    <!-- USB dongle B is attached as hostdev by the operator / virt-manager spare -->
"
        ),
        Role::OsintClearnet => {
            "    <!-- USB dongle B is attached as hostdev by the operator / virt-manager spare -->\n"
                .to_owned()
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct XmlFacts {
    pub interfaces: usize,
    pub has_default_network: bool,
    pub has_user_net: bool,
    pub has_passt: bool,
    pub has_virbr0: bool,
    pub forge_whonix_nets: usize,
    pub hostdev_usb: usize,
    pub usb_ids: Vec<crate::usb::UsbId>,
    pub disk_files: Vec<String>,
    pub forge_profile: Option<String>,
    pub forge_role: Option<String>,
    pub forge_digest: Option<String>,
    pub forge_overlay: Option<String>,
}

#[must_use]
pub fn inspect(xml: &str) -> XmlFacts {
    let lower = xml.to_ascii_lowercase();
    let mut facts = XmlFacts {
        interfaces: count_tag(&lower, "<interface"),
        has_default_network: lower.contains("network='default'")
            || lower.contains("network=\"default\""),
        has_user_net: lower.contains("type='user'") || lower.contains("type=\"user\""),
        has_passt: lower.contains("type='passt'")
            || lower.contains("type=\"passt\"")
            || lower.contains("model='passt'")
            || lower.contains("model=\"passt\""),
        has_virbr0: lower.contains("virbr0"),
        forge_whonix_nets: {
            let needle = WHONIX_NET;
            lower.matches(needle).count()
        },
        hostdev_usb: hostdev_usb_count(&lower),
        ..XmlFacts::default()
    };
    facts.usb_ids = crate::usb::ids_in_xml(xml);
    facts.disk_files = source_files(xml);
    facts.forge_profile = meta_text(xml, "profile");
    facts.forge_role = meta_text(xml, "role");
    facts.forge_digest = meta_text(xml, "base-digest");
    facts.forge_overlay = meta_text(xml, "overlay");
    facts
}

fn count_tag(xml: &str, tag: &str) -> usize {
    xml.matches(tag).count()
}

fn hostdev_usb_count(xml: &str) -> usize {
    // Count <hostdev ... type='usb'
    let mut n = 0;
    let mut rest = xml;
    while let Some(i) = rest.find("<hostdev") {
        let chunk = &rest[i..];
        let end = chunk.find('>').unwrap_or(chunk.len());
        let head = &chunk[..end];
        if head.contains("type='usb'") || head.contains("type=\"usb\"") {
            n += 1;
        }
        rest = &chunk[1..];
    }
    n
}

fn source_files(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<source") {
        let chunk = &rest[i..];
        if let Some(file) = attr(chunk, "file") {
            out.push(file);
        }
        rest = &chunk[1..];
    }
    out
}

fn attr(chunk: &str, name: &str) -> Option<String> {
    for quote in ['\'', '"'] {
        let pat = format!("{name}={quote}");
        if let Some(i) = chunk.find(&pat) {
            let rest = &chunk[i + pat.len()..];
            if let Some(end) = rest.find(quote) {
                return Some(rest[..end].to_owned());
            }
        }
    }
    None
}

fn meta_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<forge:{tag}>");
    let close = format!("</forge:{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].trim().to_owned())
}

pub fn check_role(xml: &str, expected: Role) -> Result<()> {
    let facts = inspect(xml);
    if facts.has_default_network || facts.has_user_net || facts.has_passt || facts.has_virbr0 {
        return Err(ForgeError::Role(
            "NAT leaked into domain XML (default/user/passt/virbr0)".to_owned(),
        ));
    }
    match expected {
        Role::Isolated => {
            if facts.interfaces != 0 {
                return Err(ForgeError::Role(format!(
                    "isolated VM must have no NIC, found {}",
                    facts.interfaces
                )));
            }
            if facts.hostdev_usb != 0 {
                return Err(ForgeError::Role(
                    "isolated VM must not have USB hostdev (dongle B is not for Tsurugi/SIFT)"
                        .to_owned(),
                ));
            }
        }
        Role::WhonixWs => {
            if facts.interfaces != 1 || facts.forge_whonix_nets == 0 {
                return Err(ForgeError::Role(
                    "whonix-ws must have exactly one NIC on forge-whonix".to_owned(),
                ));
            }
            if facts.hostdev_usb != 0 {
                return Err(ForgeError::Role(
                    "whonix-ws must not hold dongle B".to_owned(),
                ));
            }
        }
        Role::WhonixGw => {
            if facts.interfaces != 1 || facts.forge_whonix_nets == 0 {
                return Err(ForgeError::Role(
                    "whonix-gw must have exactly one NIC on forge-whonix (no host NAT)".to_owned(),
                ));
            }
        }
        Role::OsintClearnet => {
            if facts.interfaces != 0 {
                return Err(ForgeError::Role(
                    "osint-clearnet must not have a libvirt NIC; only USB dongle B".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

#[must_use]
pub fn is_forge_domain(xml: &str) -> bool {
    xml.contains(METADATA_NS)
}

/// Immutable bases cannot be relabeled; libvirt must skip the backing file.
#[must_use]
pub fn backing_relabel_skipped(xml: &str) -> bool {
    let lower = xml.to_ascii_lowercase();
    lower.contains("<backingstore") && lower.contains("relabel='no'")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_spec() -> DomainSpec<'static> {
        DomainSpec {
            name: "tsurugi",
            uuid: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
            profile: "tsurugi",
            role: Role::Isolated,
            overlay: "/var/lib/forge/vms/tsurugi.qcow2",
            base: "/var/lib/forge/bases/tsurugi.qcow2",
            base_digest: "sha256:deadbeef",
            memory_mib: 8192,
            vcpus: 4,
        }
    }

    #[test]
    fn isolated_xml_has_no_nic() {
        let xml = domain_xml(&isolated_spec());
        assert!(!xml.contains("<interface"));
        assert!(!xml.contains("virbr0"));
        assert!(!xml.contains("type='user'"));
        check_role(&xml, Role::Isolated).expect("isolated ok");
        let facts = inspect(&xml);
        assert_eq!(facts.forge_profile.as_deref(), Some("tsurugi"));
        assert_eq!(facts.forge_role.as_deref(), Some("isolated"));
        assert!(xml.contains("<backingStore"));
        assert!(xml.contains("relabel='no'"));
        assert!(backing_relabel_skipped(&xml));
        assert_eq!(facts.disk_files.len(), 2);
        assert!(facts.disk_files[0].ends_with("vms/tsurugi.qcow2"));
        assert!(facts.disk_files[1].ends_with("bases/tsurugi.qcow2"));
    }

    #[test]
    fn isolated_with_default_net_is_error() {
        let mut xml = domain_xml(&isolated_spec());
        xml = xml.replace(
            "</disk>\n",
            "</disk>\n    <interface type='network'><source network='default'/></interface>\n",
        );
        let err = check_role(&xml, Role::Isolated).expect_err("must fail");
        let msg = err.to_string();
        assert!(msg.contains("NAT") || msg.contains("NIC"));
    }

    #[test]
    fn kali_xml_is_osint_no_nic() {
        let spec = DomainSpec {
            name: "kali",
            uuid: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
            profile: "kali",
            role: Role::OsintClearnet,
            overlay: "/var/lib/forge/vms/kali.qcow2",
            base: "/var/lib/forge/bases/kali.qcow2",
            base_digest: "sha256:deadbeef",
            memory_mib: 4096,
            vcpus: 2,
        };
        let xml = domain_xml(&spec);
        assert!(!xml.contains("<interface"));
        assert!(xml.contains("osint-clearnet"));
        check_role(&xml, Role::OsintClearnet).expect("osint ok");
        let with_nat = xml.replace(
            "</disk>\n",
            "</disk>\n    <interface type='network'><source network='default'/></interface>\n",
        );
        assert!(check_role(&with_nat, Role::OsintClearnet).is_err());
    }

    #[test]
    fn whonix_pair_xml_roles() {
        let gw = DomainSpec {
            name: "whonix-gateway",
            uuid: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
            profile: "whonix",
            role: Role::WhonixGw,
            overlay: "/var/lib/forge/vms/whonix-gateway.qcow2",
            base: "/var/lib/forge/bases/whonix-gateway.qcow2",
            base_digest: "sha256:gw",
            memory_mib: 2048,
            vcpus: 2,
        };
        let ws = DomainSpec {
            name: "whonix-workstation",
            uuid: "bbbbbbbb-cccc-dddd-eeee-ffffffffffff",
            profile: "whonix",
            role: Role::WhonixWs,
            overlay: "/var/lib/forge/vms/whonix-workstation.qcow2",
            base: "/var/lib/forge/bases/whonix-workstation.qcow2",
            base_digest: "sha256:ws",
            memory_mib: 4096,
            vcpus: 2,
        };
        let gw_xml = domain_xml(&gw);
        let ws_xml = domain_xml(&ws);
        assert!(gw_xml.contains("forge-whonix"));
        assert!(ws_xml.contains("forge-whonix"));
        assert!(!gw_xml.contains("default"));
        assert!(!ws_xml.contains("<hostdev"));
        check_role(&gw_xml, Role::WhonixGw).unwrap();
        check_role(&ws_xml, Role::WhonixWs).unwrap();
        let nat = gw_xml.replace("forge-whonix", "default");
        assert!(check_role(&nat, Role::WhonixGw).is_err());
    }

    #[test]
    fn isolated_rejects_usb_hostdev() {
        let mut xml = domain_xml(&isolated_spec());
        xml.push_str(&crate::usb::UsbId::parse("0b95:1790").unwrap().hostdev_xml());
        let err = check_role(&xml, Role::Isolated).expect_err("usb");
        assert!(err.to_string().contains("USB"));
    }
}
