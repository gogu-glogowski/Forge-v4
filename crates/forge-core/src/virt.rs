use std::fs;
use std::path::Path;

use crate::cmd::{self, command};
use crate::error::{ForgeError, Result};
use crate::profile::{FORGE_VMS_POOL, SYSTEM_URI, WHONIX_NET};
use crate::role::VmPower;

pub fn uri() -> Result<String> {
    if let Ok(from_env) = std::env::var("FORGE_LIBVIRT_URI") {
        if from_env.trim() != SYSTEM_URI {
            return Err(ForgeError::Host(format!(
                "FORGE_LIBVIRT_URI={from_env} — Forge 4.0 uses only {SYSTEM_URI}"
            )));
        }
    }
    try_connect(SYSTEM_URI)?;
    Ok(SYSTEM_URI.to_owned())
}

pub fn try_connect(uri: &str) -> Result<String> {
    let output = command("virsh")
        .args(["-c", uri, "uri"])
        .output()
        .map_err(|error| ForgeError::Virt(format!("cannot run virsh: {error}")))?;
    if output.status.success() {
        Ok(cmd::stdout(&output))
    } else {
        Err(ForgeError::Virt(cmd::stdout_or_stderr(&output)))
    }
}

pub fn virsh(uri: &str, args: &[&str]) -> Result<String> {
    let mut all = vec!["-c", uri];
    all.extend_from_slice(args);
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
    match virsh(uri, &["net-info", WHONIX_NET]) {
        Ok(_) => {
            let xml = virsh(uri, &["net-dumpxml", WHONIX_NET])?;
            let lower = xml.to_ascii_lowercase();
            if lower.contains("mode='nat'")
                || lower.contains("mode=\"nat\"")
                || lower.contains("virbr0")
            {
                return Err(ForgeError::Role(format!(
                    "{WHONIX_NET} must be forward=none, no NAT"
                )));
            }
            let _ = virsh(uri, &["net-start", WHONIX_NET]);
            Ok(())
        }
        Err(_) => {
            let xml = format!(
                "<network>\n  <name>{WHONIX_NET}</name>\n  <bridge name='virbr-forgewx' stp='off' delay='0'/>\n  <forward mode='none'/>\n</network>\n"
            );
            let tmp = tempfile_xml(&xml)?;
            let result = virsh(uri, &["net-define", &tmp]);
            let _ = fs::remove_file(&tmp);
            result?;
            virsh(uri, &["net-start", WHONIX_NET])?;
            let _ = virsh(uri, &["net-autostart", WHONIX_NET]);
            Ok(())
        }
    }
}

pub fn qemu_img_create_overlay(base: &Path, overlay: &Path) -> Result<()> {
    if let Some(parent) = overlay.parent() {
        fs::create_dir_all(parent)?;
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
    let info = cmd::run_checked("qemu-img", &["info", "--output=json", &path_str(overlay)?])?;
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
