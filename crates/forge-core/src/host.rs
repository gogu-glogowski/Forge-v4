//! Read-only host discovery. Recognizing a distribution is not a port readiness claim.
use std::fs;

use crate::cmd;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distribution {
    Fedora,
    Debian,
    Ubuntu,
    Pop,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsRelease {
    pub distribution: Distribution,
    pub id: String,
    pub version: String,
    pub pretty_name: String,
    pub id_like: Vec<String>,
}

impl OsRelease {
    pub fn parse(text: &str) -> Self {
        let id = config_value(text, "ID").unwrap_or_default();
        let distribution = match id.as_str() {
            "fedora" => Distribution::Fedora,
            "debian" => Distribution::Debian,
            "ubuntu" => Distribution::Ubuntu,
            "pop" => Distribution::Pop,
            _ => Distribution::Unknown,
        };
        Self {
            distribution,
            pretty_name: config_value(text, "PRETTY_NAME").unwrap_or_else(|| {
                if id.is_empty() {
                    "unknown".to_owned()
                } else {
                    id.clone()
                }
            }),
            id,
            version: config_value(text, "VERSION_ID").unwrap_or_default(),
            id_like: config_value(text, "ID_LIKE")
                .unwrap_or_default()
                .split_whitespace()
                .map(ToOwned::to_owned)
                .collect(),
        }
    }

    pub fn major_version(&self) -> Option<u32> {
        self.version.split('.').next()?.parse().ok()
    }

    pub fn install_hint(&self, dependency: Dependency) -> Option<String> {
        let package = match self.distribution {
            Distribution::Fedora => dependency.fedora_package,
            Distribution::Debian | Distribution::Ubuntu | Distribution::Pop => {
                if dependency.name == "7z" && self.legacy_seven_zip() {
                    "p7zip-full"
                } else {
                    dependency.debian_package
                }
            }
            Distribution::Unknown => return None,
        };
        let manager = if self.distribution == Distribution::Fedora {
            "dnf"
        } else {
            "apt"
        };
        Some(format!("sudo {manager} install {package}"))
    }

    fn legacy_seven_zip(&self) -> bool {
        let Some(major) = self.major_version() else {
            return false;
        };
        match self.distribution {
            Distribution::Debian => major < 13,
            Distribution::Ubuntu | Distribution::Pop => major < 24,
            _ => false,
        }
    }
}

/// Parse a literal assignment; never execute os-release or qemu.conf as shell code.
fn config_value(text: &str, key: &str) -> Option<String> {
    text.lines().rev().find_map(|line| {
        let (name, raw) = line.trim().split_once('=')?;
        if name.trim() != key {
            return None;
        }
        let raw = raw.trim();
        if let Some(quote) = raw.chars().next().filter(|c| *c == '\'' || *c == '"') {
            let rest = &raw[1..];
            let end = rest.find(quote)?;
            let suffix = rest[end + 1..].trim();
            if !suffix.is_empty() && !suffix.starts_with('#') {
                return None;
            }
            Some(rest[..end].to_owned())
        } else {
            Some(raw.split('#').next().unwrap_or_default().trim().to_owned())
        }
    })
}

#[derive(Debug, Clone, Copy)]
pub struct Dependency {
    pub name: &'static str,
    pub alternatives: &'static [&'static str],
    pub fedora_package: &'static str,
    pub debian_package: &'static str,
    pub required: bool,
}

pub const DEPENDENCIES: &[Dependency] = &[
    Dependency {
        name: "lsattr",
        alternatives: &["lsattr"],
        fedora_package: "e2fsprogs",
        debian_package: "e2fsprogs",
        required: true,
    },
    Dependency {
        name: "ps",
        alternatives: &["ps"],
        fedora_package: "procps-ng",
        debian_package: "procps",
        required: true,
    },
    Dependency {
        name: "QEMU",
        alternatives: &["qemu-system-x86_64"],
        fedora_package: "qemu-kvm",
        debian_package: "qemu-system-x86",
        required: true,
    },
    Dependency {
        name: "qemu-img",
        alternatives: &["qemu-img"],
        fedora_package: "qemu-img",
        debian_package: "qemu-utils",
        required: true,
    },
    Dependency {
        name: "libvirt virsh",
        alternatives: &["virsh"],
        fedora_package: "libvirt-client libvirt-daemon-kvm",
        debian_package: "libvirt-clients libvirt-daemon-system",
        required: true,
    },
    Dependency {
        name: "GNOME Boxes",
        alternatives: &["gnome-boxes"],
        fedora_package: "gnome-boxes",
        debian_package: "gnome-boxes",
        required: true,
    },
    Dependency {
        name: "NetworkManager CLI",
        alternatives: &["nmcli"],
        fedora_package: "NetworkManager",
        debian_package: "network-manager",
        required: true,
    },
    Dependency {
        name: "systemctl",
        alternatives: &["systemctl"],
        fedora_package: "systemd",
        debian_package: "systemd",
        required: true,
    },
    Dependency {
        name: "D-Bus busctl",
        alternatives: &["busctl"],
        fedora_package: "systemd",
        debian_package: "systemd",
        required: true,
    },
    Dependency {
        name: "GnuPG",
        alternatives: &["gpg"],
        fedora_package: "gnupg2",
        debian_package: "gnupg",
        required: true,
    },
    Dependency {
        name: "curl",
        alternatives: &["curl"],
        fedora_package: "curl",
        debian_package: "curl",
        required: true,
    },
    Dependency {
        name: "7z",
        alternatives: &["7z", "7zz", "7za"],
        fedora_package: "7zip",
        debian_package: "7zip",
        required: true,
    },
    Dependency {
        name: "tar",
        alternatives: &["tar"],
        fedora_package: "tar",
        debian_package: "tar",
        required: true,
    },
    Dependency {
        name: "xz",
        alternatives: &["xz"],
        fedora_package: "xz",
        debian_package: "xz-utils",
        required: true,
    },
    Dependency {
        name: "chattr",
        alternatives: &["chattr"],
        fedora_package: "e2fsprogs",
        debian_package: "e2fsprogs",
        required: true,
    },
    Dependency {
        name: "ip",
        alternatives: &["ip"],
        fedora_package: "iproute",
        debian_package: "iproute2",
        required: true,
    },
    Dependency {
        name: "pidof",
        alternatives: &["pidof"],
        fedora_package: "psmisc",
        debian_package: "sysvinit-utils",
        required: true,
    },
    Dependency {
        name: "sudo",
        alternatives: &["sudo"],
        fedora_package: "sudo",
        debian_package: "sudo",
        required: true,
    },
    Dependency {
        name: "stdbuf",
        alternatives: &["stdbuf"],
        fedora_package: "coreutils",
        debian_package: "coreutils",
        required: true,
    },
    Dependency {
        name: "getent",
        alternatives: &["getent"],
        fedora_package: "glibc-common",
        debian_package: "libc-bin",
        required: true,
    },
    Dependency {
        name: "virt-manager",
        alternatives: &["virt-manager"],
        fedora_package: "virt-manager virt-viewer",
        debian_package: "virt-manager virt-viewer",
        required: false,
    },
];

#[derive(Debug, Clone)]
pub struct Tool {
    pub dependency: Dependency,
    pub executable: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub unit: String,
    pub load_state: String,
    pub active_state: String,
}

impl Service {
    pub fn available(&self) -> bool {
        self.load_state == "loaded"
    }
    pub fn active(&self) -> bool {
        self.available() && self.active_state == "active"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityState {
    Enforcing,
    Permissive,
    Enabled,
    Disabled,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Security {
    pub selinux: SecurityState,
    /// Kernel enablement only; does not prove a per-VM AppArmor profile is enforced.
    pub apparmor: SecurityState,
    pub libvirt_drivers: Option<String>,
    pub advertised_models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QemuIdentity {
    pub user: Option<String>,
    pub group: Option<String>,
    /// True only when both values are explicit in qemu.conf and resolved by NSS.
    pub configured: bool,
}

/// Small probe boundary permits fixture-based tests without host changes or sudo.
trait Probe {
    fn environment(&self, _key: &str) -> Option<String> {
        None
    }
    fn read(&self, path: &str) -> Option<String>;
    fn exists(&self, path: &str) -> bool;
    fn output(&self, program: &str, args: &[&str]) -> Option<String>;
    fn executable(&self, alternatives: &[&str]) -> Option<String>;
}

struct LiveProbe;
impl Probe for LiveProbe {
    fn environment(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
    fn read(&self, path: &str) -> Option<String> {
        fs::read_to_string(path).ok()
    }
    fn exists(&self, path: &str) -> bool {
        std::path::Path::new(path).exists()
    }
    fn output(&self, program: &str, args: &[&str]) -> Option<String> {
        cmd::run_checked(program, args).ok()
    }
    fn executable(&self, alternatives: &[&str]) -> Option<String> {
        cmd::first_existing(alternatives)
    }
}

#[derive(Debug, Clone)]
pub struct Host {
    pub os: OsRelease,
    pub architecture: String,
    pub systemd: bool,
    pub desktop_session: Option<String>,
    pub native_boxes: bool,
    pub services: Vec<Service>,
    pub security: Security,
    pub qemu: QemuIdentity,
    pub tools: Vec<Tool>,
}

impl Host {
    pub fn discover() -> Self {
        Self::probe(&LiveProbe)
    }

    pub fn service(&self, unit: &str) -> Option<&Service> {
        self.services.iter().find(|service| service.unit == unit)
    }

    pub fn libvirt_activation_hint(&self) -> Option<String> {
        // Prefer the mode already active. Do not suggest enabling both daemon families.
        let usable = |unit: &str| self.service(unit).is_some_and(Service::available);
        let active = |unit: &str| self.service(unit).is_some_and(Service::active);
        let modular = ["virtqemud.service", "virtqemud.socket"]
            .iter()
            .any(|u| active(u));
        let monolithic = ["libvirtd.service", "libvirtd.socket"]
            .iter()
            .any(|u| active(u));
        if monolithic || (!modular && (usable("libvirtd.service") || usable("libvirtd.socket"))) {
            let unit = if usable("libvirtd.service") {
                "libvirtd.service"
            } else {
                "libvirtd.socket"
            };
            Some(format!("sudo systemctl enable --now {unit}"))
        } else if usable("virtqemud.socket") {
            let units: Vec<_> = [
                "virtqemud.socket",
                "virtnetworkd.socket",
                "virtstoraged.socket",
                "virtnodedevd.socket",
                "virtnwfilterd.socket",
                "virtsecretd.socket",
                "virtinterfaced.socket",
                "virtlogd.socket",
                "virtlockd.socket",
            ]
            .into_iter()
            .filter(|u| usable(u))
            .collect();
            Some(format!("sudo systemctl enable --now {}", units.join(" ")))
        } else {
            None
        }
    }

    fn probe(probe: &impl Probe) -> Self {
        let release = probe
            .read("/etc/os-release")
            .or_else(|| probe.read("/usr/lib/os-release"))
            .unwrap_or_default();
        let os = OsRelease::parse(&release);
        let config = probe.read("/etc/libvirt/qemu.conf").unwrap_or_default();
        let selinux = match probe
            .read("/sys/fs/selinux/enforce")
            .as_deref()
            .map(str::trim)
        {
            Some("1") => SecurityState::Enforcing,
            Some("0") => SecurityState::Permissive,
            _ => match probe.output("getenforce", &[]).as_deref().map(str::trim) {
                Some("Enforcing") => SecurityState::Enforcing,
                Some("Permissive") => SecurityState::Permissive,
                Some("Disabled") => SecurityState::Disabled,
                _ => SecurityState::Unknown,
            },
        };
        let apparmor = match probe
            .read("/sys/module/apparmor/parameters/enabled")
            .as_deref()
            .map(str::trim)
        {
            Some("Y") => SecurityState::Enabled,
            Some("N") => SecurityState::Disabled,
            _ => SecurityState::Unknown,
        };
        let services = [
            "libvirtd.service",
            "libvirtd.socket",
            "virtqemud.service",
            "virtqemud.socket",
            "virtnetworkd.socket",
            "virtstoraged.socket",
            "virtnodedevd.socket",
            "virtnwfilterd.socket",
            "virtsecretd.socket",
            "virtinterfaced.socket",
            "virtlogd.socket",
            "virtlockd.socket",
            "NetworkManager.service",
        ]
        .into_iter()
        .map(|unit| {
            let text = probe
                .output(
                    "systemctl",
                    &[
                        "show",
                        unit,
                        "--property=LoadState,ActiveState",
                        "--no-pager",
                    ],
                )
                .unwrap_or_default();
            Service {
                unit: unit.to_owned(),
                load_state: config_value(&text, "LoadState")
                    .unwrap_or_else(|| "unknown".to_owned()),
                active_state: config_value(&text, "ActiveState")
                    .unwrap_or_else(|| "unknown".to_owned()),
            }
        })
        .collect();
        let user = config_value(&config, "user");
        let group = config_value(&config, "group");
        let resolve = |database: &str, value: &str| {
            let value = value.strip_prefix('+').unwrap_or(value);
            if value.is_empty() || value.starts_with('-') {
                return None;
            }
            probe.output("getent", &[database, value]).and_then(|text| {
                text.lines()
                    .next()?
                    .split(':')
                    .next()
                    .map(ToOwned::to_owned)
            })
        };
        let configured_user = user.as_deref().and_then(|u| resolve("passwd", u));
        let configured_group = group.as_deref().and_then(|g| resolve("group", g));
        let configured = configured_user.is_some() && configured_group.is_some();
        // Candidates are for diagnostics only. Never use these guesses for chown.
        let candidates = if os.distribution == Distribution::Fedora {
            ["qemu", "libvirt-qemu"]
        } else {
            ["libvirt-qemu", "qemu"]
        };
        let qemu = QemuIdentity {
            user: if user.is_some() {
                configured_user
            } else {
                candidates.iter().find_map(|u| resolve("passwd", u))
            },
            group: if group.is_some() {
                configured_group
            } else {
                ["qemu", "libvirt-qemu", "kvm"]
                    .iter()
                    .find_map(|g| resolve("group", g))
            },
            configured,
        };
        let native_boxes = match os.distribution {
            Distribution::Fedora => probe.output("rpm", &["-q", "gnome-boxes"]).is_some(),
            Distribution::Debian | Distribution::Ubuntu | Distribution::Pop => probe
                .output(
                    "dpkg-query",
                    &["-W", "-f=${db:Status-Status}", "gnome-boxes"],
                )
                .is_some_and(|status| status.trim() == "installed"),
            Distribution::Unknown => false,
        };
        Self {
            native_boxes,
            os,
            architecture: probe
                .output("uname", &["-m"])
                .unwrap_or_else(|| "unknown".to_owned())
                .trim()
                .to_owned(),
            systemd: probe.exists("/run/systemd/system"),
            desktop_session: probe
                .environment("XDG_SESSION_TYPE")
                .filter(|s| s == "x11" || s == "wayland")
                .or_else(|| {
                    probe
                        .environment("WAYLAND_DISPLAY")
                        .filter(|s| !s.is_empty())
                        .map(|_| "wayland".to_owned())
                })
                .or_else(|| {
                    probe
                        .environment("DISPLAY")
                        .filter(|s| !s.is_empty())
                        .map(|_| "x11".to_owned())
                }),
            services,
            security: Security {
                selinux,
                apparmor,
                libvirt_drivers: config_value(&config, "security_driver"),
                advertised_models: security_models(
                    &probe
                        .output("virsh", &["-c", crate::profile::SYSTEM_URI, "capabilities"])
                        .unwrap_or_default(),
                ),
            },
            qemu,
            tools: DEPENDENCIES
                .iter()
                .map(|dependency| Tool {
                    dependency: *dependency,
                    executable: probe.executable(dependency.alternatives),
                })
                .collect(),
        }
    }
}

// Only host security models from libvirt capabilities, not arbitrary device models.
fn security_models(capabilities: &str) -> Vec<String> {
    capabilities
        .split("<secmodel>")
        .skip(1)
        .filter_map(|section| {
            let section = section.split_once("</secmodel>")?.0;
            let model = section
                .split_once("<model>")?
                .1
                .split_once("</model>")?
                .0
                .trim();
            (!model.is_empty()).then(|| model.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Fixture {
        files: BTreeMap<String, String>,
        commands: BTreeMap<String, String>,
    }
    impl Fixture {
        fn file(mut self, path: &str, text: &str) -> Self {
            self.files.insert(path.to_owned(), text.to_owned());
            self
        }
        fn command(mut self, program: &str, args: &[&str], text: &str) -> Self {
            self.commands
                .insert(format!("{program} {}", args.join(" ")), text.to_owned());
            self
        }
        fn service(self, unit: &str, state: &str) -> Self {
            self.command(
                "systemctl",
                &[
                    "show",
                    unit,
                    "--property=LoadState,ActiveState",
                    "--no-pager",
                ],
                &format!("LoadState=loaded\nActiveState={state}\n"),
            )
        }
    }
    impl Probe for Fixture {
        fn read(&self, path: &str) -> Option<String> {
            self.files.get(path).cloned()
        }
        fn exists(&self, path: &str) -> bool {
            self.files.contains_key(path)
        }
        fn output(&self, program: &str, args: &[&str]) -> Option<String> {
            self.commands
                .get(&format!("{program} {}", args.join(" ")))
                .cloned()
        }
        fn executable(&self, alternatives: &[&str]) -> Option<String> {
            alternatives
                .iter()
                .find(|name| self.files.contains_key(**name))
                .map(|name| (*name).to_owned())
        }
    }

    #[test]
    fn distributions_and_derivatives_are_distinguished() {
        for (id, expected) in [
            ("fedora", Distribution::Fedora),
            ("debian", Distribution::Debian),
            ("ubuntu", Distribution::Ubuntu),
            ("pop", Distribution::Pop),
            ("mint", Distribution::Unknown),
        ] {
            let os = OsRelease::parse(&format!(
                "ID={id}\nID_LIKE=\"ubuntu debian\"\nVERSION_ID='24.04'\nPRETTY_NAME='Example OS'"
            ));
            assert_eq!(os.distribution, expected);
            assert_eq!(os.major_version(), Some(24));
            assert_eq!(os.id_like, ["ubuntu", "debian"]);
            assert_eq!(os.pretty_name, "Example OS");
        }
        assert_eq!(OsRelease::parse("").distribution, Distribution::Unknown);
    }

    #[test]
    fn package_hints_follow_distribution_and_release() {
        let zip = *DEPENDENCIES.iter().find(|d| d.name == "7z").unwrap();
        for (release, expected) in [
            ("ID=fedora\nVERSION_ID=44", "sudo dnf install 7zip"),
            ("ID=debian\nVERSION_ID=12", "sudo apt install p7zip-full"),
            ("ID=ubuntu\nVERSION_ID=24.04", "sudo apt install 7zip"),
            ("ID=pop\nVERSION_ID=22.04", "sudo apt install p7zip-full"),
        ] {
            assert_eq!(
                OsRelease::parse(release).install_hint(zip).as_deref(),
                Some(expected)
            );
        }
        assert_eq!(
            OsRelease::parse("ID=arch\nID_LIKE=debian").install_hint(zip),
            None
        );
    }

    #[test]
    fn active_socket_is_detected_without_running_daemon() {
        let host = Host::probe(
            &Fixture::default()
                .file("/usr/lib/os-release", "ID=debian\nVERSION_ID=13")
                .file("/run/systemd/system", "")
                .service("virtqemud.socket", "active")
                .service("virtqemud.service", "inactive")
                .service("virtnetworkd.socket", "inactive")
                .service("libvirtd.service", "inactive")
                .command("uname", &["-m"], "x86_64\n"),
        );
        assert_eq!(host.os.distribution, Distribution::Debian);
        assert!(host.systemd);
        assert!(host.service("virtqemud.socket").unwrap().active());
        let hint = host.libvirt_activation_hint().unwrap();
        assert!(hint.contains("virtqemud.socket"));
        assert!(hint.contains("virtnetworkd.socket"));
        assert!(!hint.contains("libvirtd"));
    }

    #[test]
    fn monolithic_socket_only_hint_does_not_name_an_absent_service() {
        let host = Host::probe(&Fixture::default().service("libvirtd.socket", "active"));
        assert_eq!(
            host.libvirt_activation_hint().as_deref(),
            Some("sudo systemctl enable --now libvirtd.socket")
        );
    }

    #[test]
    fn native_boxes_requires_installed_package_status() {
        let base = || {
            Fixture::default()
                .file("/etc/os-release", "ID=ubuntu\nVERSION_ID=24.04")
                .file("gnome-boxes", "")
        };
        let args = ["-W", "-f=${db:Status-Status}", "gnome-boxes"];
        assert!(Host::probe(&base().command("dpkg-query", &args, "installed")).native_boxes);
        assert!(!Host::probe(&base().command("dpkg-query", &args, "config-files")).native_boxes);
        assert!(!Host::probe(&base()).native_boxes);
    }

    #[test]
    fn active_monolithic_daemon_wins_over_installed_modular_units() {
        let host = Host::probe(
            &Fixture::default()
                .service("libvirtd.service", "active")
                .service("virtqemud.socket", "inactive"),
        );
        assert_eq!(
            host.libvirt_activation_hint().as_deref(),
            Some("sudo systemctl enable --now libvirtd.service")
        );
    }

    #[test]
    fn capabilities_report_actual_security_models() {
        let host = Host::probe(&Fixture::default().command("virsh", &["-c", "qemu:///system", "capabilities"],
            "<capabilities><host><secmodel><model>apparmor</model><doi>0</doi></secmodel><secmodel><model>dac</model></secmodel></host></capabilities>"));
        assert_eq!(host.security.advertised_models, ["apparmor", "dac"]);
        assert_eq!(
            security_models("<model>selinux</model>"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn unreadable_security_state_is_not_reported_as_enabled() {
        let host = Host::probe(&Fixture::default());
        assert_eq!(host.security.selinux, SecurityState::Unknown);
        assert_eq!(host.security.apparmor, SecurityState::Unknown);
        assert!(!host.qemu.configured);
        assert_eq!(host.libvirt_activation_hint(), None);
        assert!(host.tools.iter().all(|tool| tool.executable.is_none()));
    }

    #[test]
    fn security_and_explicit_numeric_identity_are_resolved() {
        let host = Host::probe(&Fixture::default()
            .file("/sys/fs/selinux/enforce", "0\n")
            .file("/sys/module/apparmor/parameters/enabled", "Y\n")
            .file("/etc/libvirt/qemu.conf", "# user = 'wrong'\nuser = '+64055' # local override\ngroup = 'kvm'\nsecurity_driver = 'apparmor'\n")
            .command("getent", &["passwd", "64055"], "libvirt-qemu:x:64055:108::/:/usr/sbin/nologin\n")
            .command("getent", &["group", "kvm"], "kvm:x:108:\n"));
        assert_eq!(host.security.selinux, SecurityState::Permissive);
        assert_eq!(host.security.apparmor, SecurityState::Enabled);
        assert_eq!(host.security.libvirt_drivers.as_deref(), Some("apparmor"));
        assert_eq!(host.qemu.user.as_deref(), Some("libvirt-qemu"));
        assert_eq!(host.qemu.group.as_deref(), Some("kvm"));
        assert!(host.qemu.configured);
    }

    #[test]
    fn missing_configured_account_does_not_fall_back_to_another_account() {
        let host = Host::probe(
            &Fixture::default()
                .file(
                    "/etc/libvirt/qemu.conf",
                    "user = 'missing'\ngroup = 'missing'",
                )
                .command(
                    "getent",
                    &["passwd", "libvirt-qemu"],
                    "libvirt-qemu:x:64055:108::/:/usr/sbin/nologin",
                ),
        );
        assert_eq!(host.qemu.user, None);
        assert!(!host.qemu.configured);
    }

    #[test]
    fn defaults_are_candidates_and_zip_alternatives_are_detected() {
        let host = Host::probe(
            &Fixture::default()
                .file("/etc/os-release", "ID=pop\nVERSION_ID=24.04")
                .file("7zz", "")
                .command(
                    "getent",
                    &["passwd", "libvirt-qemu"],
                    "libvirt-qemu:x:64055:108::/:/usr/sbin/nologin",
                ),
        );
        assert_eq!(host.qemu.user.as_deref(), Some("libvirt-qemu"));
        assert!(!host.qemu.configured);
        assert_eq!(
            host.tools
                .iter()
                .find(|t| t.dependency.name == "7z")
                .unwrap()
                .executable
                .as_deref(),
            Some("7zz")
        );
    }
}
