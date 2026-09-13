use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use crate::cmd::{self, exists};
use crate::error::Result;
use crate::profile::SYSTEM_URI;
use crate::virt;
use crate::xml;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone)]
pub struct Check {
    pub status: CheckStatus,
    pub name: String,
    pub detail: String,
    pub fix: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub uri: Option<String>,
    pub checks: Vec<Check>,
}

impl DoctorReport {
    #[must_use]
    pub fn ok(&self) -> bool {
        self.checks
            .iter()
            .all(|check| check.status != CheckStatus::Fail)
    }
}

const DNF_VIRT: &str = "sudo dnf install @virtualization gnome-boxes virt-manager virt-viewer";
const DNF_BUILD: &str = "sudo dnf install git gcc rust cargo libvirt-devel";

pub fn run() -> Result<DoctorReport> {
    let mut checks = Vec::new();
    checks.push(os_check());
    checks.push(bin_check("qemu-system-x86_64", "QEMU", Some(DNF_VIRT)));
    checks.push(bin_check("qemu-img", "qemu-img", Some(DNF_VIRT)));
    checks.push(bin_check("virsh", "libvirt virsh", Some(DNF_VIRT)));
    checks.push(kvm_check());
    checks.push(group_check());
    checks.push(selinux_check());
    checks.push(boxes_rpm_check());
    checks.push(virt_manager_check());
    checks.push(bin_check("gpg", "GnuPG", Some("sudo dnf install gnupg2")));

    let system = virt::try_connect(SYSTEM_URI);
    match &system {
        Ok(uri) => checks.push(Check {
            status: CheckStatus::Ok,
            name: "libvirt URI".to_owned(),
            detail: format!("{uri} (Forge uses system only; Boxes must point here)"),
            fix: None,
        }),
        Err(error) => checks.push(Check {
            status: CheckStatus::Fail,
            name: "libvirt URI".to_owned(),
            detail: error.to_string(),
            fix: Some(format!(
                "{DNF_VIRT}\nsudo systemctl enable --now libvirtd\nsudo usermod -aG libvirt \"$USER\"  # then log out"
            )),
        }),
    }

    let session = virt::try_connect("qemu:///session");
    if session.is_ok() {
        checks.push(Check {
            status: CheckStatus::Ok,
            name: "qemu:///session".to_owned(),
            detail: "present on this account — Forge will not use it; Boxes New-VM wizard would"
                .to_owned(),
            fix: None,
        });
    }

    if let Ok(uri) = &system {
        checks.push(nat_on_forge_domains(uri));
    }

    checks.push(Check {
        status: CheckStatus::Ok,
        name: "build deps".to_owned(),
        detail: if exists("cargo") {
            "cargo found".to_owned()
        } else {
            format!("cargo missing — {DNF_BUILD}")
        },
        fix: (!exists("cargo")).then(|| DNF_BUILD.to_owned()),
    });

    let uri = system.ok();
    Ok(DoctorReport { uri, checks })
}

pub fn os_check() -> Check {
    evaluate_os_release(&fs::read_to_string("/etc/os-release").unwrap_or_default())
}

pub fn evaluate_os_release(os: &str) -> Check {
    let id = value(os, "ID");
    let version = value(os, "VERSION_ID");
    let pretty = value(os, "PRETTY_NAME").unwrap_or_else(|| "unknown".to_owned());
    let fedora = id.as_deref() == Some("fedora");
    let major = version
        .as_deref()
        .and_then(|v| v.split('.').next())
        .and_then(|v| v.parse::<u32>().ok());
    if fedora && major.is_some_and(|n| n >= 44) {
        Check {
            status: CheckStatus::Ok,
            name: "Host".to_owned(),
            detail: pretty,
            fix: None,
        }
    } else if fedora {
        Check {
            status: CheckStatus::Fail,
            name: "Host".to_owned(),
            detail: format!("{pretty} — Forge 4.0 requires Fedora 44 or newer"),
            fix: Some("Upgrade the host. doctor does not change the OS.".to_owned()),
        }
    } else {
        Check {
            status: CheckStatus::Fail,
            name: "Host".to_owned(),
            detail: format!("{pretty} — not Fedora"),
            fix: Some("Forge 4.0 is Fedora-first.".to_owned()),
        }
    }
}

fn value(os: &str, key: &str) -> Option<String> {
    os.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix(&format!("{key}="))
            .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_owned())
    })
}

fn kvm_check() -> Check {
    let path = Path::new("/dev/kvm");
    if !path.exists() {
        return Check {
            status: CheckStatus::Fail,
            name: "/dev/kvm".to_owned(),
            detail: "missing (CPU virtualization off or kvm module not loaded)".to_owned(),
            fix: Some("sudo modprobe kvm_intel  # or kvm_amd".to_owned()),
        };
    }
    match fs::metadata(path) {
        Ok(meta) if meta.permissions().mode() & 0o006 != 0 || path_usable(path) => Check {
            status: CheckStatus::Ok,
            name: "/dev/kvm".to_owned(),
            detail: "present and accessible".to_owned(),
            fix: None,
        },
        Ok(_) => Check {
            status: CheckStatus::Fail,
            name: "/dev/kvm".to_owned(),
            detail: "present but this user cannot access it".to_owned(),
            fix: Some("sudo usermod -aG kvm \"$USER\"  # then log out".to_owned()),
        },
        Err(error) => Check {
            status: CheckStatus::Fail,
            name: "/dev/kvm".to_owned(),
            detail: error.to_string(),
            fix: None,
        },
    }
}

fn path_usable(path: &Path) -> bool {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .is_ok()
}

fn group_check() -> Check {
    let user = env::var("USER").unwrap_or_default();
    let groups = Command::new("id")
        .args(["-nG"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .unwrap_or_default();
    let in_libvirt = groups.split_whitespace().any(|g| g == "libvirt");
    if in_libvirt {
        Check {
            status: CheckStatus::Ok,
            name: "libvirt group".to_owned(),
            detail: format!("{user} is in libvirt (qemu:///system)"),
            fix: None,
        }
    } else {
        Check {
            status: CheckStatus::Fail,
            name: "libvirt group".to_owned(),
            detail: format!("{user} is not in libvirt"),
            fix: Some("sudo usermod -aG libvirt \"$USER\" && log out".to_owned()),
        }
    }
}

fn selinux_check() -> Check {
    let output = Command::new("getenforce").output().ok();
    let mode = output
        .as_ref()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned());
    if mode.eq_ignore_ascii_case("disabled") {
        Check {
            status: CheckStatus::Fail,
            name: "SELinux".to_owned(),
            detail: "Disabled — Forge will not run on a host with SELinux off".to_owned(),
            fix: Some("Re-enable SELinux. Do not turn it off for Forge.".to_owned()),
        }
    } else {
        Check {
            status: CheckStatus::Ok,
            name: "SELinux".to_owned(),
            detail: mode,
            fix: None,
        }
    }
}

fn boxes_rpm_check() -> Check {
    match cmd::run_checked("rpm", &["-q", "gnome-boxes"]) {
        Ok(pkg) => Check {
            status: CheckStatus::Ok,
            name: "GNOME Boxes".to_owned(),
            detail: format!("{pkg} (daily GUI; point at qemu:///system, do not New-VM)"),
            fix: None,
        },
        Err(_) => Check {
            status: CheckStatus::Fail,
            name: "GNOME Boxes".to_owned(),
            detail: "rpm gnome-boxes missing (Flatpak does not see system domains)".to_owned(),
            fix: Some("sudo dnf install gnome-boxes".to_owned()),
        },
    }
}

fn virt_manager_check() -> Check {
    if exists("virt-manager") {
        Check {
            status: CheckStatus::Ok,
            name: "virt-manager".to_owned(),
            detail: "present (spare: XML, USB dongle B)".to_owned(),
            fix: None,
        }
    } else {
        Check {
            status: CheckStatus::Warn,
            name: "virt-manager".to_owned(),
            detail: "missing — daily work uses Boxes; install for XML/USB spare".to_owned(),
            fix: Some("sudo dnf install virt-manager virt-viewer".to_owned()),
        }
    }
}

fn bin_check(bin: &str, name: &str, fix: Option<&str>) -> Check {
    if exists(bin) {
        Check {
            status: CheckStatus::Ok,
            name: name.to_owned(),
            detail: format!("{bin} found"),
            fix: None,
        }
    } else {
        Check {
            status: CheckStatus::Fail,
            name: name.to_owned(),
            detail: format!("{bin} not found"),
            fix: fix.map(ToOwned::to_owned),
        }
    }
}

fn nat_on_forge_domains(uri: &str) -> Check {
    let names = match virt::list_all_names(uri) {
        Ok(n) => n,
        Err(error) => {
            return Check {
                status: CheckStatus::Warn,
                name: "Forge NAT".to_owned(),
                detail: format!("cannot list domains: {error}"),
                fix: None,
            };
        }
    };
    let mut bad = Vec::new();
    for name in names {
        let Ok(xml) = virt::dumpxml(uri, &name) else {
            continue;
        };
        if !xml::is_forge_domain(&xml) {
            continue;
        }
        let facts = xml::inspect(&xml);
        if facts.has_default_network || facts.has_user_net || facts.has_passt || facts.has_virbr0 {
            bad.push(name);
        }
    }
    if bad.is_empty() {
        Check {
            status: CheckStatus::Ok,
            name: "Forge NAT".to_owned(),
            detail: "no default/user/passt/virbr0 on Forge domains".to_owned(),
            fix: None,
        }
    } else {
        Check {
            status: CheckStatus::Fail,
            name: "Forge NAT".to_owned(),
            detail: format!("NAT leaked on: {}", bad.join(", ")),
            fix: Some("forge status; fix XML or recreate. Forge does not NAT guests.".to_owned()),
        }
    }
}

#[must_use]
pub fn format_report(report: &DoctorReport) -> String {
    let mut out = String::from("Forge doctor\n");
    if let Some(uri) = &report.uri {
        out.push_str(&format!("Using libvirt: {uri}\n"));
    }
    out.push('\n');
    for check in &report.checks {
        let mark = match check.status {
            CheckStatus::Ok => "ok  ",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "FAIL",
        };
        out.push_str(&format!("{mark}  {}: {}\n", check.name, check.detail));
        if let Some(fix) = &check.fix {
            for line in fix.lines() {
                out.push_str(&format!("      {line}\n"));
            }
        }
    }
    if report.ok() {
        out.push_str("\nHost is ready for Forge.\n");
    } else {
        out.push_str(
            "\nHost is not ready. Install the packages above; Forge will not change the host for you.\n",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fedora_44_is_ok() {
        let os =
            "ID=fedora\nVERSION_ID=44\nPRETTY_NAME=\"Fedora Linux 44 (Workstation Edition)\"\n";
        assert_eq!(evaluate_os_release(os).status, CheckStatus::Ok);
    }

    #[test]
    fn fedora_43_is_fail() {
        let os = "ID=fedora\nVERSION_ID=43\nPRETTY_NAME=\"Fedora Linux 43\"\n";
        assert_eq!(evaluate_os_release(os).status, CheckStatus::Fail);
    }

    #[test]
    fn arch_is_fail() {
        let os = "ID=arch\nPRETTY_NAME=\"Arch Linux\"\n";
        assert_eq!(evaluate_os_release(os).status, CheckStatus::Fail);
    }

    #[test]
    fn live_host_doctor_runs() {
        let report = run().expect("doctor");
        let text = format_report(&report);
        assert!(text.contains("Forge doctor"));
        assert!(text.contains("Host"));
    }
}
