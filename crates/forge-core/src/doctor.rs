use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use crate::error::Result;
use crate::host::{Distribution, Host, OsRelease, SecurityState};
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

pub fn run() -> Result<DoctorReport> {
    let host = Host::discover();
    let mut checks = host_checks(&host);
    checks.push(kvm_check());
    checks.push(group_check());
    checks.push(lab_storage_check());
    checks.push(boxes_system_source_check());
    let network_mode = crate::network::Mode::load(&crate::paths::ForgePaths::discover())?;
    checks.push(Check {
        status: CheckStatus::Ok,
        name: "network mode".to_owned(),
        detail: network_mode.label().to_owned(),
        fix: None,
    });
    if network_mode == crate::network::Mode::Dedicated {
        checks.push(dongle_b_check());
        checks.push(host_cables_check());
    }
    checks.push(libvirt_forward_check(network_mode));
    checks.push(qemu_hook_check());

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
                "{}\nsudo usermod -aG libvirt \"$USER\"  # then log out",
                host.libvirt_activation_hint().unwrap_or_else(|| {
                    "Install the system libvirt daemon and inspect the host service manager"
                        .to_owned()
                })
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
        checks.push(nat_on_forge_domains(uri, network_mode));
        checks.extend(network_definitions(uri, network_mode));
    }

    let uri = system.ok();
    Ok(DoctorReport { uri, checks })
}

pub fn os_check() -> Check {
    evaluate_os_release(&fs::read_to_string("/etc/os-release").unwrap_or_default())
}

pub fn evaluate_os_release(os: &str) -> Check {
    distribution_check(&OsRelease::parse(os))
}

fn distribution_check(os: &OsRelease) -> Check {
    let (status, detail) = match os.distribution {
        Distribution::Fedora if os.major_version().is_some_and(|n| n >= 44) => {
            (CheckStatus::Ok, os.pretty_name.clone())
        }
        Distribution::Fedora => (
            CheckStatus::Fail,
            format!("{} — Forge requires Fedora 44 or newer", os.pretty_name),
        ),
        Distribution::Debian
        | Distribution::Ubuntu
        | Distribution::Pop
        | Distribution::Mint
        | Distribution::Zorin
        | Distribution::Alma
        | Distribution::Rocky
        | Distribution::Void
        | Distribution::Nixos => (
            CheckStatus::Ok,
            format!(
                "{} — recognized port target; runtime support checked separately",
                os.pretty_name
            ),
        ),
        Distribution::Arch | Distribution::Manjaro => (
            CheckStatus::Ok,
            format!(
                "{} — recognized rolling target; fully update with pacman -Syu, then verify host/security readiness",
                os.pretty_name
            ),
        ),
        Distribution::Unknown => (
            CheckStatus::Fail,
            format!(
                "{} — unrecognized distribution (ID={}, ID_LIKE={})",
                os.pretty_name,
                os.id,
                os.id_like.join(" ")
            ),
        ),
    };
    Check {
        status,
        name: "Host".to_owned(),
        detail,
        fix: (status == CheckStatus::Fail).then(|| {
            "Use a recognized distribution and inspect its port readiness and confinement checks.".to_owned()
        }),
    }
}

/// Pure evaluation of a snapshot; all host access lives in host::Host::discover.
fn host_checks(host: &Host) -> Vec<Check> {
    let mut checks = vec![distribution_check(&host.os)];
    checks.push(Check {
        status: if host.architecture == "x86_64" {
            CheckStatus::Ok
        } else {
            CheckStatus::Fail
        },
        name: "architecture".to_owned(),
        detail: host.architecture.clone(),
        fix: (host.architecture != "x86_64")
            .then(|| "Forge guests require an x86_64 host.".to_owned()),
    });
    checks.push(Check {
        status: if host.systemd || host.runit {
            CheckStatus::Ok
        } else {
            CheckStatus::Fail
        },
        name: if host.runit { "runit" } else { "systemd" }.to_owned(),
        detail: if host.systemd || host.runit {
            if host.runit {
                "runit running"
            } else {
                "systemd running"
            }
        } else {
            "not detected as host service manager"
        }
        .to_owned(),
        fix: None,
    });
    checks.push(Check {
        status: if host.desktop_session.is_some() {
            CheckStatus::Ok
        } else {
            CheckStatus::Warn
        },
        name: "desktop session".to_owned(),
        detail: host.desktop_session.clone().unwrap_or_else(|| {
            "no graphical session detected in this process; GNOME Boxes needs a desktop session"
                .to_owned()
        }),
        fix: None,
    });
    let nm_active = host
        .service("NetworkManager.service")
        .is_some_and(crate::host::Service::active);
    checks.push(Check {
        status: if nm_active { CheckStatus::Ok } else { CheckStatus::Fail },
        name: "NetworkManager service".to_owned(), detail: if nm_active { "active" } else { "not active or cannot be inspected; this port requires NetworkManager" }.to_owned(),
        fix: (!nm_active).then(|| if host.runit { "Inspect sv status /var/service/NetworkManager; enable the runit service and use NetworkManager.".to_owned() } else { "Inspect systemctl status NetworkManager.service; configure the desktop to use NetworkManager.".to_owned() }),
    });
    let daemon_units: Vec<_> = host
        .services
        .iter()
        .filter(|s| s.unit != "NetworkManager.service" && s.available())
        .collect();
    let daemon_active = daemon_units.iter().any(|s| {
        s.active()
            && matches!(
                s.unit.as_str(),
                "libvirtd.service" | "libvirtd.socket" | "virtqemud.service" | "virtqemud.socket"
            )
    });
    checks.push(Check {
        status: if daemon_active {
            CheckStatus::Ok
        } else {
            CheckStatus::Fail
        },
        name: "libvirt services".to_owned(),
        detail: if daemon_units.is_empty() {
            "no loaded libvirt units detected".to_owned()
        } else {
            daemon_units
                .iter()
                .map(|s| format!("{}={}", s.unit, s.active_state))
                .collect::<Vec<_>>()
                .join(", ")
        },
        fix: if daemon_active {
            None
        } else {
            host.libvirt_activation_hint()
        },
    });
    checks.push(Check {
        status: if host.qemu.configured { CheckStatus::Ok } else { CheckStatus::Warn },
        name: "QEMU identity".to_owned(),
        detail: format!("user={}, group={} ({})", host.qemu.user.as_deref().unwrap_or("unknown"), host.qemu.group.as_deref().unwrap_or("unknown"),
            if host.qemu.configured { "explicit qemu.conf values resolved through getent" } else { "unconfirmed candidates; not used for permissions" }),
        fix: (!host.qemu.configured).then(|| "Confirm the effective QEMU identity before configuring disk ownership; distribution defaults are not proof.".to_owned()),
    });
    checks.push(Check {
        status: if host.native_boxes { CheckStatus::Ok } else { CheckStatus::Fail },
        name: "native Boxes package".to_owned(),
        detail: if host.native_boxes { "system package installed; use qemu:///system" } else { "native gnome-boxes package not confirmed; Flatpak alone is insufficient for this workflow" }.to_owned(),
        fix: if host.native_boxes { None } else { crate::host::DEPENDENCIES.iter().find(|d| d.name == "GNOME Boxes").and_then(|d| host.os.install_hint(*d)) },
    });
    checks.extend(security_checks(host));
    for tool in &host.tools {
        checks.push(Check {
            status: if tool.executable.is_some() {
                CheckStatus::Ok
            } else if tool.dependency.required {
                CheckStatus::Fail
            } else {
                CheckStatus::Warn
            },
            name: tool.dependency.name.to_owned(),
            detail: tool.executable.as_ref().map_or_else(
                || format!("{} not found", tool.dependency.alternatives.join("/")),
                |bin| format!("{bin} found"),
            ),
            fix: if tool.executable.is_some() {
                None
            } else {
                host.os.install_hint(tool.dependency)
            },
        });
    }
    if matches!(
        host.os.distribution,
        Distribution::Debian
            | Distribution::Ubuntu
            | Distribution::Pop
            | Distribution::Arch
            | Distribution::Manjaro
            | Distribution::Mint
            | Distribution::Zorin
            | Distribution::Alma
            | Distribution::Rocky
            | Distribution::Void
            | Distribution::Nixos
    ) {
        checks.push(Check {
            status: if host.os.distribution == Distribution::Pop && host.os.version == "24.04" {
                CheckStatus::Warn
            } else {
                CheckStatus::Fail
            }, name: "port readiness".to_owned(),
            detail: if matches!(host.os.distribution, Distribution::Arch | Distribution::Manjaro) {
                "Rolling target recognized. Stock Arch libvirt disables AppArmor/SELinux drivers; a compatible confined libvirt build is required. Manjaro capabilities must be checked separately; firewall integration and live VM validation remain pending.".to_owned()
            } else if matches!(host.os.distribution, Distribution::Void | Distribution::Nixos | Distribution::Alma | Distribution::Rocky | Distribution::Mint | Distribution::Zorin) {
                "Experimental port: native dependency resolution, security confinement and physical start/stop validation are pending; run all host checks before preparing guests.".to_owned()
            } else {
                "Standard mode was exercised on Pop!_OS 24.04 with AppArmor, Boxes and Whonix; firewall integration across distributions and dedicated A/B hardware validation remain pending.".to_owned()
            },
            fix: matches!(host.os.distribution, Distribution::Arch | Distribution::Manjaro).then(|| "Use an AppArmor-enabled libvirt build with virt-aa-helper profiles; inspect virsh capabilities before setup.".to_owned()),
        });
    }
    checks
}

fn security_checks(host: &Host) -> Vec<Check> {
    let selinux_ok = host.security.selinux == SecurityState::Enforcing;
    let fedora = matches!(
        host.os.distribution,
        Distribution::Fedora | Distribution::Alma | Distribution::Rocky
    );
    vec![
        Check {
            status: if selinux_ok {
                CheckStatus::Ok
            } else if fedora {
                CheckStatus::Fail
            } else {
                CheckStatus::Warn
            },
            name: "SELinux".to_owned(),
            detail: format!("{:?}", host.security.selinux),
            fix: (fedora && !selinux_ok).then(|| {
                "Fedora requires verified SELinux enforcing mode; inspect getenforce.".to_owned()
            }),
        },
        Check {
            status: CheckStatus::Warn,
            name: "AppArmor".to_owned(),
            detail: format!(
                "{:?} (kernel state; per-VM profile enforcement is not verified)",
                host.security.apparmor
            ),
            fix: None,
        },
        Check {
            status: if host.security.advertised_models.is_empty() {
                CheckStatus::Warn
            } else {
                CheckStatus::Ok
            },
            name: "libvirt security driver".to_owned(),
            detail: format!(
                "configured: {}; libvirt advertises: {} (per-VM enforcement not verified)",
                host.security
                    .libvirt_drivers
                    .as_deref()
                    .unwrap_or("default or unreadable"),
                if host.security.advertised_models.is_empty() {
                    "unknown".to_owned()
                } else {
                    host.security.advertised_models.join(", ")
                }
            ),
            fix: None,
        },
    ]
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

fn qemu_hook_check() -> Check {
    let report = crate::hook::doctor();
    let status = match report.kind {
        crate::hook::HookDoctorKind::Ok => CheckStatus::Ok,
        crate::hook::HookDoctorKind::Fail => CheckStatus::Fail,
    };
    Check {
        status,
        name: "qemu hook".to_owned(),
        detail: report.detail,
        fix: report.fix,
    }
}

fn boxes_system_source_check() -> Check {
    let path = crate::boxes::source_path();
    let text = fs::read_to_string(&path).unwrap_or_default();
    if crate::boxes::has_system_uri(&text) {
        Check {
            status: CheckStatus::Ok,
            name: "Boxes system URI".to_owned(),
            detail: format!(
                "{} → qemu:///system (restart Boxes if it was already open)",
                path.display()
            ),
            fix: None,
        }
    } else {
        Check {
            status: CheckStatus::Fail,
            name: "Boxes system URI".to_owned(),
            detail: "Boxes is on qemu:///session only — Forge overlays live on qemu:///system"
                .to_owned(),
            fix: Some(format!(
                "forge list  # writes {}\nthen fully quit and reopen GNOME Boxes",
                path.display()
            )),
        }
    }
}

fn libvirt_forward_check(mode: crate::network::Mode) -> Check {
    match crate::hostnet::libvirt_forwards() {
        Some(true) => Check {
            status: CheckStatus::Ok,
            name: "libvirt forward".to_owned(),
            detail: "firewalld zone libvirt forwards the gateway NAT".to_owned(),
            fix: None,
        },
        Some(false) => Check {
            status: CheckStatus::Warn,
            name: "libvirt forward".to_owned(),
            detail: "firewalld zone libvirt drops forwarded packets".to_owned(),
            fix: Some(
                if mode == crate::network::Mode::Standard {
                    "forge setup"
                } else {
                    "forge start whonix-gateway"
                }
                .to_owned(),
            ),
        },
        None => Check {
            status: CheckStatus::Warn,
            name: "libvirt forward".to_owned(),
            detail: "live firewall configuration not inspected: doctor does not request privilege authentication; setup/prepare verifies forwarding in its authenticated helper".to_owned(),
            fix: Some("forge prepare --all".to_owned()),
        },
    }
}

fn host_cables_check() -> Check {
    match crate::hostnet::audit() {
        Err(error) => Check {
            status: CheckStatus::Fail,
            name: "host cables".to_owned(),
            detail: error.to_string(),
            fix: None,
        },
        Ok(audit) if !audit.usb_defaults.is_empty() => Check {
            status: CheckStatus::Fail,
            name: "host cables".to_owned(),
            detail: format!(
                "dongle B is the host's default route ({})",
                audit.usb_defaults.join(", ")
            ),
            fix: Some("forge dev cables".to_owned()),
        },
        Ok(audit) if !audit.held.is_empty() => Check {
            status: CheckStatus::Warn,
            name: "host cables".to_owned(),
            detail: format!(
                "dongle B has an address ({}) so the gateway can reach the router. The host's default route stays on cable A.",
                audit.held.join(", ")
            ),
            fix: None,
        },
        Ok(audit) if !audit.foreign_defaults.is_empty() => Check {
            status: CheckStatus::Warn,
            name: "host cables".to_owned(),
            detail: format!(
                "default route is not cable A: {}",
                audit.foreign_defaults.join(", ")
            ),
            fix: None,
        },
        Ok(audit) => Check {
            status: CheckStatus::Ok,
            name: "host cables".to_owned(),
            detail: one_line(&audit.summary),
            fix: None,
        },
    }
}

fn one_line(summary: &str) -> String {
    summary
        .lines()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

fn dongle_b_check() -> Check {
    let paths = crate::paths::ForgePaths::discover();
    let pin = crate::usb::load_pin(&paths).ok().flatten();
    let plugged = crate::usb::scan_usb_net().unwrap_or_default();
    match (pin, plugged.len()) {
        (Some(id), _) if plugged.iter().any(|d| d.id == id) => Check {
            status: CheckStatus::Ok,
            name: "Dongle B".to_owned(),
            detail: format!(
                "{} plugged (gateway exit for whonix, USB device for kali)",
                id.display()
            ),
            fix: None,
        },
        (Some(id), _) => Check {
            status: CheckStatus::Warn,
            name: "Dongle B".to_owned(),
            detail: format!(
                "{} pinned but not plugged — start without WAN",
                id.display()
            ),
            fix: None,
        },
        (None, 0) => Check {
            status: CheckStatus::Ok,
            name: "Dongle B".to_owned(),
            detail: "not plugged (human cable B). Isolated never gets it.".to_owned(),
            fix: None,
        },
        (None, 1) => Check {
            status: CheckStatus::Ok,
            name: "Dongle B".to_owned(),
            detail: format!("auto {} {}", plugged[0].id.display(), plugged[0].label),
            fix: Some("optional: echo vvvv:pppp > ~/.local/share/forge/dongle-b".to_owned()),
        },
        (None, n) => Check {
            status: CheckStatus::Fail,
            name: "Dongle B".to_owned(),
            detail: format!("{n} USB net devices; pin one as B"),
            fix: Some("FORGE_DONGLE_B=vvvv:pppp  or  forge dev usb".to_owned()),
        },
    }
}

fn lab_storage_check() -> Check {
    let paths = crate::paths::ForgePaths::discover();
    if !paths.privileged_bases {
        return Check {
            status: CheckStatus::Ok,
            name: "lab storage".to_owned(),
            detail: format!("{} (FORGE_DATA_DIR, unprivileged)", paths.root.display()),
            fix: None,
        };
    }
    let dirs_ok = crate::security::storage_directories_protected(&paths);
    let vms_ok = dirs_ok && crate::paths::dir_writable(&paths.vms);
    let unprotected: Vec<_> = crate::pull::unprotected_bases(&paths).collect();
    let mut unread = Vec::new();
    if let Ok(entries) = fs::read_dir(&paths.bases) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("qcow2")
                && !crate::paths::is_readable(&path)
            {
                unread.push(
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                );
            }
        }
    }
    let selinux_wrong = bases_wrong_selinux(&paths);
    if vms_ok && unread.is_empty() && unprotected.is_empty() && !selinux_wrong {
        return Check {
            status: CheckStatus::Ok,
            name: "lab storage".to_owned(),
            detail: format!(
                "{} writable; bases readable (qemu + operator)",
                paths.vms.display()
            ),
            fix: None,
        };
    }
    let mut detail = Vec::new();
    if !vms_ok {
        detail.push(format!("{} missing or not writable", paths.vms.display()));
    }
    if !unprotected.is_empty() {
        detail.push("bases must be root:root, mode 0444, immutable and not symlinks".to_owned());
    }
    if !unread.is_empty() {
        detail.push(format!(
            "unreadable bases (need root:root mode 0444): {}",
            unread.join(", ")
        ));
    }
    if selinux_wrong {
        detail
            .push("bases need virt_content_t (libvirt backing); overlays virt_image_t".to_owned());
    }
    Check {
        status: CheckStatus::Fail,
        name: "lab storage".to_owned(),
        detail: detail.join("; "),
        fix: Some(
            "forge pull <profile>  # sudo prepares /var/lib/forge; do not sudo forge".to_owned(),
        ),
    }
}

fn bases_wrong_selinux(paths: &crate::paths::ForgePaths) -> bool {
    let Ok(entries) = fs::read_dir(&paths.bases) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("qcow2") {
            continue;
        }
        let output = Command::new("ls")
            .args(["-Z", path.to_str().unwrap_or("")])
            .output()
            .ok();
        let text = output
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        if text.contains("var_lib_t") || text.contains("virt_image_t") {
            return true;
        }
    }
    false
}

fn nat_on_forge_domains(uri: &str, mode: crate::network::Mode) -> Check {
    let names = match virt::list_all_names(uri) {
        Ok(n) => n,
        Err(error) => {
            return Check {
                status: CheckStatus::Warn,
                name: "Forge network roles".to_owned(),
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
        let role = facts
            .forge_role
            .as_deref()
            .and_then(crate::role::Role::parse);
        if role.is_none_or(|role| crate::network::check_domain(&xml, role, mode).is_err()) {
            bad.push(name);
        }
    }
    if bad.is_empty() {
        Check {
            status: CheckStatus::Ok,
            name: "Forge network roles".to_owned(),
            detail: format!("Forge domains match {mode} network policy"),
            fix: None,
        }
    } else {
        Check {
            status: CheckStatus::Fail,
            name: "Forge network roles".to_owned(),
            detail: format!("network policy mismatch on: {}", bad.join(", ")),
            fix: Some(
                "Stop all guests and run forge setup to regenerate the selected mode.".to_owned(),
            ),
        }
    }
}

fn network_definitions(uri: &str, mode: crate::network::Mode) -> Vec<Check> {
    let names: &[&str] = if mode == crate::network::Mode::Standard {
        &[
            crate::network::STANDARD_NET,
            crate::profile::WAN_NET,
            crate::profile::WHONIX_NET,
        ]
    } else {
        &[crate::profile::WHONIX_NET]
    };
    let mut checks: Vec<_> = names
        .iter()
        .map(|name| {
            let valid = crate::virt::virsh(uri, &["net-dumpxml", name]).is_ok_and(|xml| {
                if *name == crate::profile::WHONIX_NET {
                    crate::network::private_ok(&xml)
                } else {
                    crate::network::standard_nat_ok(&xml, name)
                }
            });
            Check {
                status: if valid {
                    CheckStatus::Ok
                } else {
                    CheckStatus::Fail
                },
                name: format!("network {name}"),
                detail: if valid {
                    "matches selected network policy"
                } else {
                    "missing or differs from selected network policy"
                }
                .to_owned(),
                fix: (!valid)
                    .then(|| format!("Stop all guests, then forge setup --network {mode}")),
            }
        })
        .collect();
    if fs::read_to_string("/etc/ufw/ufw.conf")
        .is_ok_and(|text| text.lines().any(|line| line.trim() == "ENABLED=yes"))
    {
        checks.push(Check { status: CheckStatus::Warn, name: "UFW forwarding".to_owned(),
            detail: "UFW is enabled; guest forwarding still needs verification. Forge does not disable UFW or change its global defaults.".to_owned(), fix: None });
    }
    checks
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
            "\nHost is not ready. Doctor does not change the host. Run forge setup to prepare the selected network mode.\n",
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
    fn arch_rolling_is_recognized_but_requires_separate_readiness_checks() {
        let os = "ID=arch\nPRETTY_NAME=\"Arch Linux\"\n";
        assert_eq!(evaluate_os_release(os).status, CheckStatus::Ok);
    }

    fn snapshot(release: &str) -> Host {
        Host {
            os: OsRelease::parse(release),
            architecture: "x86_64".to_owned(),
            systemd: true,
            runit: false,
            desktop_session: Some("wayland".to_owned()),
            native_boxes: true,
            services: vec![
                crate::host::Service {
                    unit: "NetworkManager.service".to_owned(),
                    load_state: "loaded".to_owned(),
                    active_state: "active".to_owned(),
                },
                crate::host::Service {
                    unit: "libvirtd.service".to_owned(),
                    load_state: "loaded".to_owned(),
                    active_state: "active".to_owned(),
                },
            ],
            security: crate::host::Security {
                selinux: SecurityState::Enforcing,
                apparmor: SecurityState::Unknown,
                libvirt_drivers: None,
                advertised_models: vec!["selinux".to_owned()],
            },
            qemu: crate::host::QemuIdentity {
                user: Some("qemu".to_owned()),
                group: Some("qemu".to_owned()),
                configured: true,
            },
            tools: crate::host::DEPENDENCIES
                .iter()
                .map(|dependency| crate::host::Tool {
                    dependency: *dependency,
                    executable: Some(dependency.alternatives[0].to_owned()),
                })
                .collect(),
        }
    }

    #[test]
    fn port_targets_are_recognized_but_not_falsely_ready() {
        for id in ["debian", "ubuntu", "pop", "arch", "manjaro"] {
            let host = snapshot(&format!("ID={id}\nVERSION_ID=24.04"));
            let checks = host_checks(&host);
            assert_eq!(checks[0].status, CheckStatus::Ok);
            let report = DoctorReport { uri: None, checks };
            let readiness = report
                .checks
                .iter()
                .find(|check| check.name == "port readiness")
                .unwrap();
            assert_eq!(
                readiness.status,
                if id == "pop" {
                    CheckStatus::Warn
                } else {
                    CheckStatus::Fail
                }
            );
            assert_eq!(report.ok(), id == "pop");
            assert!(format_report(&report).contains("firewall integration"));
        }
    }

    #[test]
    fn unsupported_architecture_and_missing_services_fail() {
        let mut host = snapshot("ID=fedora\nVERSION_ID=44");
        host.architecture = "aarch64".to_owned();
        host.systemd = false;
        host.services.clear();
        let checks = host_checks(&host);
        for name in [
            "architecture",
            "systemd",
            "NetworkManager service",
            "libvirt services",
        ] {
            assert_eq!(
                checks.iter().find(|c| c.name == name).unwrap().status,
                CheckStatus::Fail
            );
        }
    }

    #[test]
    fn unknown_or_permissive_selinux_does_not_pass_fedora() {
        let mut host = snapshot("ID=fedora\nVERSION_ID=44");
        for state in [
            SecurityState::Unknown,
            SecurityState::Disabled,
            SecurityState::Permissive,
        ] {
            host.security.selinux = state;
            assert_eq!(security_checks(&host)[0].status, CheckStatus::Fail);
        }
        host.security.selinux = SecurityState::Enforcing;
        assert_eq!(security_checks(&host)[0].status, CheckStatus::Ok);
    }

    #[test]
    fn missing_tools_have_apt_hints_without_fedora_commands() {
        let mut host = snapshot("ID=ubuntu\nVERSION_ID=24.04");
        for tool in &mut host.tools {
            tool.executable = None;
        }
        let text = format_report(&DoctorReport {
            uri: None,
            checks: host_checks(&host),
        });
        assert!(text.contains("sudo apt install qemu-utils"));
        assert!(text.contains("sudo apt install network-manager"));
        assert!(!text.contains("dnf"));
        assert!(!text.contains("rpm"));
    }

    #[test]
    fn live_host_doctor_runs() {
        let report = run().expect("doctor");
        let text = format_report(&report);
        assert!(text.contains("Forge doctor"));
        assert!(text.contains("Host"));
        assert!(text.contains("lab storage"));
        assert!(text.contains("Boxes system URI") || text.contains("GNOME Boxes"));
    }
}
