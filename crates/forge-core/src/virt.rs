use std::fs;
use std::path::Path;

use crate::cmd::{self, command};
use crate::error::{ForgeError, Result, io_path};
use crate::profile::{FORGE_VMS_POOL, SYSTEM_URI, WAN_NET, WHONIX_NET};
use crate::role::VmPower;

pub fn uri() -> Result<String> {
    if let Ok(from_env) = std::env::var("FORGE_LIBVIRT_URI") {
        if from_env.trim() != SYSTEM_URI {
            return Err(ForgeError::Host(format!(
                "FORGE_LIBVIRT_URI={from_env} — Forge 4.2.0 uses only {SYSTEM_URI}"
            )));
        }
    }
    try_connect(SYSTEM_URI)?;
    Ok(SYSTEM_URI.to_owned())
}

pub fn try_connect(uri: &str) -> Result<String> {
    virsh(uri, &["uri"])
}

pub fn virsh(uri: &str, args: &[&str]) -> Result<String> {
    retry_read(args, || virsh_once(uri, args))
}

fn retry_read(args: &[&str], mut operation: impl FnMut() -> Result<String>) -> Result<String> {
    let safe = matches!(args.first().copied(), Some("uri" | "list" | "domstate" | "dumpxml" | "net-list" | "net-dumpxml" | "net-info" | "pool-info" | "capabilities"));
    for attempt in 0..3 {
        match operation() {
            Err(error) if safe && attempt < 2 && transient_disconnect(&error.to_string()) => {
                std::thread::sleep(std::time::Duration::from_millis(250 * (attempt + 1)));
            }
            result => return result,
        }
    }
    unreachable!()
}

fn transient_disconnect(error: &str) -> bool {
    error.contains("Connection reset by peer") || error.contains("End of file while reading data")
}

fn virsh_once(uri: &str, args: &[&str]) -> Result<String> {
    let mut all = vec!["-c", uri];
    all.extend_from_slice(args);
    if cmd::has_priv_session() {
        let arguments = all.iter().map(|arg| cmd::shell_quote(arg)).collect::<Vec<_>>().join(" ");
        return cmd::priv_capture(&format!("LC_ALL=C virsh {arguments} 2>&1"))
            .map_err(|error| ForgeError::Virt(error.to_string()));
    }
    let output = command("virsh")
        .args(&all)
        .output()
        .map_err(|error| ForgeError::Virt(format!("cannot run virsh: {error}")))?;
    if output.status.success() {
        Ok(cmd::stdout(&output))
    } else {
        Err(ForgeError::Virt(cmd::stdout_or_stderr(&output)))
    }
}

pub fn domain_exists(uri: &str, domain: &str) -> Result<bool> {
    match virsh(uri, &["domstate", domain]) {
        Ok(_) => Ok(true),
        Err(ForgeError::Virt(message))
            if message.contains("failed to get domain")
                || message.contains("Domain not found")
                || message.contains("failed to get")
                || message.contains("nie znaleziono") =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

pub fn domstate(uri: &str, domain: &str) -> Result<VmPower> {
    let state = virsh(uri, &["domstate", domain])?;
    Ok(VmPower::from_domstate(&state))
}

pub fn define_xml(uri: &str, xml: &str) -> Result<()> {
    let tmp = tempfile_xml(xml)?;
    let result = virsh(uri, &["define", &tmp]);
    let _ = fs::remove_file(&tmp);
    result.map(|_| ())
}

pub fn undefine(uri: &str, domain: &str) -> Result<()> {
    match virsh(uri, &["undefine", domain, "--nvram"]) {
        Ok(_) => Ok(()),
        Err(_) => virsh(uri, &["undefine", domain]).map(|_| ()),
    }
}

pub fn start(uri: &str, domain: &str) -> Result<()> {
    virsh(uri, &["start", domain]).map(|_| ())
}

pub fn shutdown(uri: &str, domain: &str) -> Result<()> {
    virsh(uri, &["shutdown", domain]).map(|_| ())
}

pub fn destroy(uri: &str, domain: &str) -> Result<()> {
    virsh(uri, &["destroy", domain]).map(|_| ())
}

pub fn dumpxml(uri: &str, domain: &str) -> Result<String> {
    virsh(uri, &["dumpxml", domain])
}

pub fn dumpxml_inactive(uri: &str, domain: &str) -> Result<String> {
    virsh(uri, &["dumpxml", domain, "--inactive"])
}

pub fn attach_device_live(uri: &str, domain: &str, xml: &str) -> Result<()> {
    let tmp = tempfile_xml(xml)?;
    let result = virsh(uri, &["attach-device", domain, &tmp, "--live"]);
    let _ = fs::remove_file(&tmp);
    result.map(|_| ())
}

pub fn detach_device_live(uri: &str, domain: &str, xml: &str) -> Result<()> {
    let tmp = tempfile_xml(xml)?;
    let result = virsh(uri, &["detach-device", domain, &tmp, "--live"]);
    let _ = fs::remove_file(&tmp);
    result.map(|_| ())
}

pub fn detach_device_config(uri: &str, domain: &str, xml: &str) -> Result<()> {
    let tmp = tempfile_xml(xml)?;
    let result = virsh(uri, &["detach-device", domain, &tmp, "--config"]);
    let _ = fs::remove_file(&tmp);
    result.map(|_| ())
}

pub fn list_all_names(uri: &str) -> Result<Vec<String>> {
    let raw = virsh(uri, &["list", "--all", "--name"])?;
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

pub fn ensure_vms_pool(uri: &str, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::create_dir_all(path);
    match virsh(uri, &["pool-info", FORGE_VMS_POOL]) {
        Ok(_) => {
            let _ = virsh(uri, &["pool-start", FORGE_VMS_POOL]);
            Ok(())
        }
        Err(_) => {
            let xml = format!(
                "<pool type='dir'>\n  <name>{FORGE_VMS_POOL}</name>\n  <target>\n    <path>{}</path>\n  </target>\n</pool>\n",
                crate::xml::escape(&path.display().to_string())
            );
            let tmp = tempfile_xml(&xml)?;
            let result = virsh(uri, &["pool-define", &tmp]);
            let _ = fs::remove_file(&tmp);
            result?;
            virsh(uri, &["pool-build", FORGE_VMS_POOL])?;
            virsh(uri, &["pool-start", FORGE_VMS_POOL])?;
            let _ = virsh(uri, &["pool-autostart", FORGE_VMS_POOL]);
            Ok(())
        }
    }
}

pub fn ensure_whonix_net(uri: &str) -> Result<()> {
    let xml = format!(
        "<network><name>{WHONIX_NET}</name><bridge name='virbr-forgewx' stp='off' delay='0'/></network>"
    );
    ensure_network(uri, WHONIX_NET, &xml, crate::network::private_ok)
}

pub fn ensure_standard_networks(uri: &str) -> Result<()> {
    ensure_whonix_net(uri)?;
    for name in [crate::network::STANDARD_NET, WAN_NET] {
        ensure_network(uri, name, &crate::network::standard_nat_xml(name), |xml| {
            crate::network::standard_nat_ok(xml, name)
        })?;
    }
    Ok(())
}

fn ensure_network(
    uri: &str,
    name: &str,
    desired: &str,
    matches: impl Fn(&str) -> bool,
) -> Result<()> {
    let names = virsh(uri, &["net-list", "--all", "--name"])?;
    let exists = names.lines().any(|n| n.trim() == name);
    let mut desired = desired.to_owned();
    let mut active = false;
    if exists {
        let live = virsh(uri, &["net-dumpxml", name])?;
        let persistent = virsh(uri, &["net-dumpxml", name, "--inactive"])?;
        let info = virsh(uri, &["net-info", name])?;
        active = info.lines().any(|line| {
            line.split_once(':')
                .is_some_and(|(key, value)| key.trim() == "Active" && value.trim() == "yes")
        });
        if matches(&live) && matches(&persistent) {
            if !active {
                virsh(uri, &["net-start", name])?;
            }
            virsh(uri, &["net-autostart", name])?;
            return Ok(());
        }
        if !virsh(uri, &["list", "--name"])?.trim().is_empty() {
            return Err(ForgeError::Role(format!(
                "network {name} needs repair; stop all QEMU guests and run forge setup before changing it"
            )));
        }
        if let Ok(doc) = roxmltree::Document::parse(&persistent) {
            if let Some(uuid) = doc
                .root_element()
                .children()
                .find(|n| n.has_tag_name("uuid"))
                .and_then(|n| n.text())
            {
                desired = desired.replacen(
                    "<network>",
                    &format!("<network><uuid>{}</uuid>", crate::xml::escape(uuid)),
                    1,
                );
            }
        }
        if active {
            virsh(uri, &["net-destroy", name])?;
            active = false;
        }
    }
    let tmp = tempfile_xml(&desired)?;
    let result = virsh(uri, &["net-define", &tmp]);
    let _ = fs::remove_file(tmp);
    result?;
    if !active {
        virsh(uri, &["net-start", name])?;
    }
    virsh(uri, &["net-autostart", name])?;
    Ok(())
}

/// Gateway eth0. `egress` is the USB dongle's host interface. Without it the
/// network stays isolated so traffic cannot leave through cable A.
pub fn ensure_wan_net(uri: &str, egress: Option<&str>) -> Result<()> {
    if let Some(dev) = egress {
        if !iface_ok(dev) {
            return Err(ForgeError::Host(format!(
                "refusing forge-wan egress on unexpected interface '{dev}'"
            )));
        }
    }
    let desired = wan_net_xml(egress);
    if let Ok(current) = virsh(uri, &["net-dumpxml", WAN_NET]) {
        if wan_xml_ok(&current, egress) {
            let _ = virsh(uri, &["net-start", WAN_NET]);
            return Ok(());
        }
        let _ = virsh(uri, &["net-destroy", WAN_NET]);
        let _ = virsh(uri, &["net-undefine", WAN_NET]);
    }
    let tmp = tempfile_xml(&desired)?;
    let defined = virsh(uri, &["net-define", &tmp]);
    let _ = fs::remove_file(&tmp);
    defined?;
    virsh(uri, &["net-start", WAN_NET])?;
    let _ = virsh(uri, &["net-autostart", WAN_NET]);
    Ok(())
}

fn wan_net_xml(egress: Option<&str>) -> String {
    match egress {
        Some(dev) => format!(
            "<network>\n  <name>{WAN_NET}</name>\n  <forward mode='nat'>\n    <interface dev='{dev}'/>\n  </forward>\n  <bridge name='virbr-forgewan' stp='off' delay='0'/>\n  <ip address='10.0.2.2' netmask='255.255.255.0'/>\n</network>\n"
        ),
        None => format!(
            "<network>\n  <name>{WAN_NET}</name>\n  <bridge name='virbr-forgewan' stp='off' delay='0'/>\n  <ip address='10.0.2.2' netmask='255.255.255.0'/>\n</network>\n"
        ),
    }
}

pub(crate) fn wan_xml_ok(xml: &str, egress: Option<&str>) -> bool {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return false;
    };
    let root = doc.root_element();
    let ips: Vec<_> = root.children().filter(|n| n.has_tag_name("ip")).collect();
    if !root.has_tag_name("network")
        || !root
            .children()
            .any(|n| n.has_tag_name("name") && n.text() == Some(WAN_NET))
        || !root
            .children()
            .any(|n| n.has_tag_name("bridge") && n.attribute("name") == Some("virbr-forgewan"))
        || ips.len() != 1
        || ips[0].attribute("address") != Some("10.0.2.2")
        || ips[0].attribute("netmask") != Some("255.255.255.0")
        || root.children().any(|n| n.has_tag_name("route"))
    {
        return false;
    }
    let forward: Vec<_> = root
        .children()
        .filter(|n| n.has_tag_name("forward"))
        .collect();
    match egress {
        Some(dev) => {
            forward.len() == 1
                && forward[0].attribute("mode") == Some("nat")
                && forward[0].attribute("dev").is_none_or(|s| s == dev)
                && {
                    let interfaces: Vec<_> =
                        forward[0].children().filter(|n| n.is_element()).collect();
                    interfaces.len() == 1
                        && interfaces[0].has_tag_name("interface")
                        && interfaces[0].attribute("dev") == Some(dev)
                }
        }
        None => {
            forward.is_empty()
                || (forward.len() == 1
                    && forward[0].attribute("mode") == Some("none")
                    && forward[0].attribute("dev").is_none()
                    && !forward[0].children().any(|n| n.is_element()))
        }
    }
}

fn iface_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 15
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && !name.contains("..")
}

pub fn qemu_img_create_overlay(base: &Path, overlay: &Path) -> Result<()> {
    if let Some(parent) = overlay.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| io_path(&err, parent, "cannot create overlay dir"))?;
    }
    let base_s = path_str(base)?;
    let overlay_s = path_str(overlay)?;
    cmd::run(
        "qemu-img",
        &[
            "create", "-f", "qcow2", "-F", "qcow2", "-b", &base_s, &overlay_s,
        ],
    )
    .map(|_| ())
}

pub fn qemu_img_convert(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    cmd::run(
        "qemu-img",
        &[
            "convert",
            "-p",
            "-O",
            "qcow2",
            &path_str(src)?,
            &path_str(dst)?,
        ],
    )
    .map(|_| ())
}

pub fn qemu_img_backing(overlay: &Path) -> Result<Option<String>> {
    let info = cmd::run_checked(
        "qemu-img",
        &[
            "info",
            "--force-share",
            "--output=json",
            &path_str(overlay)?,
        ],
    )?;
    Ok(json_string_field(&info, "backing-filename"))
}

fn json_string_field(json: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let i = json.find(&pat)?;
    let rest = json[i + pat.len()..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    if rest.starts_with("null") {
        return None;
    }
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].replace("\\/", "/"))
}

fn path_str(path: &Path) -> Result<String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| ForgeError::Image("non-utf8 path".to_owned()))
}

fn tempfile_xml(xml: &str) -> Result<String> {
    let path = std::env::temp_dir().join(format!(
        "forge-domain-{}-{}.xml",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    fs::write(&path, xml)?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_points_at_base() {
        let dir = tempfile_dir();
        let base = dir.join("base.qcow2");
        let overlay = dir.join("overlay.qcow2");
        cmd::run(
            "qemu-img",
            &["create", "-f", "qcow2", base.to_str().unwrap(), "8M"],
        )
        .expect("base");
        qemu_img_create_overlay(&base, &overlay).expect("overlay");
        let backing = qemu_img_backing(&overlay).expect("info").expect("backing");
        assert!(backing.ends_with("base.qcow2"), "{backing}");
        let _ = fs::remove_dir_all(&dir);
    }

    fn tempfile_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("forge-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}

#[cfg(test)]
mod reconnect_tests {
    use super::*;
    #[test]
    fn idle_disconnect_retries_reads_but_never_mutations() {
        for command in ["domstate", "dumpxml", "net-info", "start", "destroy", "define"] {
            let mut calls = 0;
            let result = retry_read(&[command], || {
                calls += 1;
                if calls == 1 { Err(ForgeError::Virt("Connection reset by peer".to_owned())) }
                else { Ok("ok".to_owned()) }
            });
            let read = matches!(command, "domstate" | "dumpxml" | "net-info");
            assert_eq!(result.is_ok(), read);
            assert_eq!(calls, if read { 2 } else { 1 });
        }
    }
    #[test]
    fn reconnect_is_bounded_and_does_not_retry_policy_errors() {
        for message in ["Connection reset by peer", "permission denied"] {
            let mut calls = 0;
            assert!(retry_read(&["list"], || { calls += 1; Err(ForgeError::Virt(message.to_owned())) }).is_err());
            assert_eq!(calls, if message == "permission denied" { 1 } else { 3 });
        }
    }
}
