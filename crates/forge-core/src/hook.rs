//! Libvirt qemu hook. Fail-closed gate for Forge domains.
//!
//! The daemon waits for this process. Calling virsh here deadlocks it, and
//! prepare/start ignore anything we print, so a drifted domain is refused.
//! `forge start` repairs XML before it asks libvirt to start. Live USB attach
//! is a later process, after this one has exited.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use crate::cmd::{self, SudoSession};
use crate::error::{ForgeError, Result};
use crate::hostnet;
use crate::ownership::{self, Ownership};
use crate::paths::{self, ForgePaths};
use crate::profile::{SYSTEM_URI, WHONIX_GW_NAME};
use crate::progress;
use crate::pull;
use crate::role::Role;
use crate::usb;
use crate::virt;
use crate::xml;

pub fn hook_path() -> String {
    format!("{}/hooks/qemu", crate::host::libvirt_config_dir())
}
const OPERATOR_PATH: &str = "/var/lib/forge/operator";
/// `/etc/libvirt` is mode 0700, so the operator cannot stat the hook itself.
/// The install writes this root-owned stamp beside the lab; only root can create it.
const HOOK_STAMP: &str = "/var/lib/forge/hook-installed";
fn network_xml(name: &str) -> String {
    format!(
        "{}/qemu/networks/{name}.xml",
        crate::host::libvirt_config_dir()
    )
}
const PID_DIR: &str = "/run/libvirt/qemu";
const HOOK_LOG: &str = "/var/lib/forge/hook.log";
const XML_CAP: usize = 2 * 1024 * 1024;
const KALI_NAME: &str = "kali";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Prepare,
    Start,
    Started,
    Stopped,
    Release,
    Migrate,
    Restore,
    Reconnect,
    Attach,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Effect {
    None,
    /// nmcli while libvirt is inside the hook deadlocks NetworkManager (it
    /// calls back into libvirt and nmcli times out). These run in a later process.
    LeaseAfter,
    ReleaseAfter,
    AttachAfter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dongle {
    Absent,
    Plugged,
}

struct Gate<'a> {
    phase: Phase,
    role: Role,
    xml: &'a str,
    gw_running: bool,
    kali_running: bool,
    wan_xml: Option<&'a str>,
    dongle: Dongle,
    dongle_iface: Option<&'a str>,
    dongle_held: bool,
    digest_ok: bool,
    backing_ok: bool,
    security_policy: crate::security::Policy,
    base_protected: bool,
    network_mode: crate::network::Mode,
    private_xml: Option<&'a str>,
    standard_xml: Option<&'a str>,
}

enum Class {
    Foreign,
    Forge(Ownership),
    Impostor(String),
}

pub enum HookDoctorKind {
    Ok,
    Fail,
}

pub struct HookDoctor {
    pub kind: HookDoctorKind,
    pub detail: String,
    pub fix: Option<String>,
}

pub fn root_entry() -> Result<()> {
    if !invoked_as_installed_hook() {
        return Err(ForgeError::Host(
            "run as yourself, not `sudo forge` (root PATH misses ~/.local/bin; overlays would be root-owned). pull/create prompt sudo only for /var/lib/forge".to_owned(),
        ));
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 2
        && matches!(
            args[0].as_str(),
            "--forge-attach" | "--forge-up" | "--forge-down"
        )
        && crate::network::Mode::installed()? == crate::network::Mode::Standard
    {
        return Ok(());
    }
    if args.len() == 2 && args[0] == "--forge-attach" {
        return attach_after(&args[1]);
    }
    if args.len() == 2 && args[0] == "--forge-up" {
        return lease_after(&args[1]);
    }
    if args.len() == 2 && args[0] == "--forge-down" {
        return release_after(&args[1]);
    }
    libvirt_event(&args)
}

pub fn ensure_installed(paths: &ForgePaths) -> Result<()> {
    ensure_installed_for(paths, crate::network::Mode::load(paths)?)
}

pub(crate) fn ensure_installed_for(paths: &ForgePaths, mode: crate::network::Mode) -> Result<()> {
    let policy = crate::security::Policy::discover()?;
    if hook_ready(paths)?
        && crate::security::Policy::trusted()? == policy
        && crate::network::Mode::installed()? == mode
    {
        return Ok(());
    }
    if guest_qemu_running() {
        return Err(ForgeError::Host(
            "a QEMU guest is running; stop it, then `forge start` again so the qemu hook can be installed"
                .to_owned(),
        ));
    }
    let exe = std::env::current_exe().map_err(|error| {
        ForgeError::Host(format!(
            "cannot see the forge binary to install the hook: {error}"
        ))
    })?;
    let exe_meta = fs::metadata(&exe)?;
    if !exe_meta.is_file() || exe_meta.permissions().mode() & 0o022 != 0 {
        return Err(ForgeError::Host(
            "refusing to install a group- or world-writable binary as the root qemu hook"
                .to_owned(),
        ));
    }
    let exe = safe_abs(
        exe.to_str()
            .ok_or_else(|| ForgeError::Host("forge binary path is not utf-8".to_owned()))?,
    )?;
    let meta = safe_abs(
        paths
            .meta
            .to_str()
            .ok_or_else(|| ForgeError::Host("Forge meta path is not utf-8".to_owned()))?,
    )?;
    let uid = paths::effective_uid();
    let security_setup = policy.helper_script(paths)?;
    let security_stamp = policy.install_stamp_script();
    let network_stamp = mode.install_script();
    let hook = hook_path();
    let hook_dir = format!("{}/hooks", crate::host::libvirt_config_dir());
    let hook_label = if policy == crate::security::Policy::Selinux {
        "restorecon -F /etc/libvirt/hooks /etc/libvirt/hooks/qemu"
    } else {
        ":"
    };
    let script = format!(
        "
set -eu
{security_setup}
install -d -m 0755 -o root -g root {hook_dir}
install -m 0755 -o root -g root '{exe}' {hook}.new
mv -f {hook}.new {hook}
chown root:root {hook}
chmod 0755 {hook}
{hook_label}
install -d -m 0755 -o root -g root /var/lib/forge
touch /var/lib/forge/network.lock
chown root:libvirt /var/lib/forge/network.lock
chmod 0660 /var/lib/forge/network.lock
umask 022
cat > /var/lib/forge/operator <<'EOF'
meta={meta}
uid={uid}
EOF
chown root:root /var/lib/forge/operator
chmod 0644 /var/lib/forge/operator
{security_stamp}
{network_stamp}
echo sha256=$(sha256sum {hook} | awk '{{print $1}}') > /var/lib/forge/hook-installed
chown root:root /var/lib/forge/hook-installed
chmod 0644 /var/lib/forge/hook-installed
if [ -d /run/systemd/system ] && systemctl is-active --quiet virtqemud.service; then
  systemctl restart virtqemud.service
elif [ -d /run/systemd/system ] && systemctl is-active --quiet libvirtd.service; then
  systemctl restart libvirtd.service
elif command -v sv >/dev/null && sv status /var/service/libvirtd >/dev/null 2>&1; then
  sv restart /var/service/libvirtd
else
  echo 'No active supported libvirt daemon; hook installed but restart failed' >&2
  exit 1
fi
"
    );
    if cmd::has_priv_session() {
        cmd::priv_script(&script)?;
    } else {
        let session = SudoSession::start()?;
        cmd::scope_priv(session, || cmd::priv_script(&script))?;
    }
    if !hook_ready(paths)? {
        return Err(ForgeError::Host(
            "qemu hook install did not land (root stamp /var/lib/forge/hook-installed missing or virtqemud still predates it)".to_owned(),
        ));
    }
    Ok(())
}

pub fn doctor() -> HookDoctor {
    let fail = |detail: String| HookDoctor {
        kind: HookDoctorKind::Fail,
        detail,
        fix: Some("forge dev hook   # installs the root qemu hook; does not start a VM".to_owned()),
    };
    let paths = ForgePaths::discover();
    match hook_ready(&paths) {
        Ok(true) => HookDoctor {
            kind: HookDoctorKind::Ok,
            detail: format!(
                "{} owns start/stop for Forge domains (digest, role, dongle B)", hook_path()
            ),
            fix: None,
        },
        Ok(false) => match trusted_stamp() {
            Ok(None) => fail(
                "missing — Play, virt-manager, and virsh would skip the Forge contract".to_owned(),
            ),
            Ok(Some(_)) => fail(
                "virtqemud started before the hook existed, or this forge binary is newer than the installed hook"
                    .to_owned(),
            ),
            Err(error) => fail(error.to_string()),
        },
        Err(error) => fail(error.to_string()),
    }
}

fn libvirt_event(args: &[String]) -> Result<()> {
    let name = args.first().map(String::as_str).unwrap_or("");
    let phase = phase_of(args.get(1).map(String::as_str).unwrap_or(""));
    let sub = args.get(2).map(String::as_str).unwrap_or("");
    let xml = read_xml()?;
    let Some(paths) = operator_paths() else {
        return if xml::is_forge_domain(&xml) || xml_uses_forge_store(&xml) {
            Err(ForgeError::Role(
                "qemu hook has no /var/lib/forge/operator; run `forge dev hook`".to_owned(),
            ))
        } else {
            Ok(())
        };
    };
    let own = load_own(&paths, name)?;
    match classify(name, &xml, own.as_ref()) {
        Class::Foreign => Ok(()),
        Class::Impostor(reason) => {
            note(&format!("deny {name} {}", phase_name(phase)));
            Err(ForgeError::Role(reason))
        }
        Class::Forge(own) => {
            if !subop_ok(phase, sub) {
                return Err(ForgeError::Role(format!(
                    "{name}: refusing libvirt hook phase {} {sub}",
                    phase_name(phase)
                )));
            }
            let role = own.role()?;
            let network_mode = crate::network::Mode::installed()?;
            let digest_ok = if needs_digest(phase) {
                pull::verify_file_digest(Path::new(&own.base), &own.base_digest, &progress::noop)?;
                true
            } else {
                false
            };
            let backing_ok = if needs_backing(phase) {
                backing_matches(&own)?
            } else {
                false
            };
            let (dongle, iface, held) = if network_mode == crate::network::Mode::Dedicated
                && matches!(phase, Phase::Prepare | Phase::Start | Phase::Restore)
            {
                dongle_view(&paths, role)?
            } else {
                (Dongle::Absent, None, false)
            };
            let wan = if role == Role::WhonixGw
                && (needs_policy(phase) || phase == Phase::Reconnect)
            {
                Some(fs::read_to_string(network_xml("forge-wan")).map_err(|error| {
                    ForgeError::Role(format!(
                        "cannot read Forge WAN network XML ({error}); `forge start {WHONIX_GW_NAME}` defines it"
                    ))
                })?)
            } else {
                None
            };
            if network_mode == crate::network::Mode::Dedicated
                && own.role()? == Role::WhonixWs
                && matches!(phase, Phase::Prepare | Phase::Start | Phase::Restore)
                && qemu_running(WHONIX_GW_NAME)
            {
                let iface = dongle_iface(&paths)?.unwrap_or_default();
                if iface.is_empty() || !hostnet::usb_lease_current(&iface) {
                    return Err(ForgeError::Role(
                        "whonix-gateway has no dongle B lease yet. Wait a few seconds and start whonix-workstation again."
                            .to_owned(),
                    ));
                }
            }
            let private_xml = if matches!(role, Role::WhonixGw | Role::WhonixWs)
                && (needs_policy(phase) || phase == Phase::Reconnect)
            {
                Some(fs::read_to_string(network_xml("forge-whonix"))?)
            } else {
                None
            };
            let standard_xml = if network_mode == crate::network::Mode::Standard
                && role == Role::OsintClearnet
                && (needs_policy(phase) || phase == Phase::Reconnect)
            {
                Some(fs::read_to_string(network_xml("forge-standard"))?)
            } else {
                None
            };
            let security_policy = if needs_policy(phase) || phase == Phase::Reconnect {
                crate::security::Policy::trusted()?
            } else {
                // Shutdown/release must still clean up the network if confinement was disabled.
                crate::security::Policy::Selinux
            };
            let effect = judge(&Gate {
                phase,
                role,
                xml: &xml,
                gw_running: qemu_running(WHONIX_GW_NAME),
                kali_running: qemu_running(KALI_NAME),
                wan_xml: wan.as_deref(),
                dongle,
                dongle_iface: iface.as_deref(),
                dongle_held: held,
                digest_ok,
                backing_ok,
                security_policy,
                network_mode,
                private_xml: private_xml.as_deref(),
                standard_xml: standard_xml.as_deref(),
                base_protected: crate::security::base_is_protected(Path::new(&own.base))
                    && (security_policy == crate::security::Policy::Apparmor
                        || crate::paths::selinux_type(Path::new(&own.base)).as_deref()
                            == Some("virt_content_t")),
            })
            .map_err(ForgeError::Role)?;
            apply(&paths, &own, effect)?;
            note(&format!("allow {name} {}", phase_name(phase)));
            Ok(())
        }
    }
}

fn attach_after(name: &str) -> Result<()> {
    let paths = operator_paths()
        .ok_or_else(|| ForgeError::Host("qemu hook operator file is missing".to_owned()))?;
    let forge = crate::lab::Forge {
        paths,
        uri: SYSTEM_URI.to_owned(),
    };
    forge.attach_kali_live(name)
}

fn apply(paths: &ForgePaths, own: &Ownership, effect: Effect) -> Result<()> {
    let _ = paths;
    match effect {
        Effect::None => {}
        Effect::LeaseAfter => schedule(&["--forge-up", &own.name]),
        Effect::ReleaseAfter => schedule(&["--forge-down", &own.name]),
        Effect::AttachAfter => schedule(&["--forge-attach", &own.name]),
    }
    Ok(())
}

fn lease_if_needed(paths: &ForgePaths) -> Result<()> {
    let Some(iface) = dongle_iface(paths)? else {
        return Ok(());
    };
    if hostnet::usb_lease_current(&iface) {
        return Ok(());
    }
    match hostnet::lease_usb(&iface) {
        Ok(router) => {
            note(&format!("dongle B {iface} leased via {router}"));
            Ok(())
        }
        Err(error) => {
            note(&format!(
                "dongle B lease failed ({error}); forge-wan stays pinned to {iface}"
            ));
            Ok(())
        }
    }
}

fn schedule(args: &[&str]) {
    if args
        .iter()
        .any(|arg| arg.contains('/') || arg.contains(' '))
    {
        return;
    }
    let mut child = Command::new(hook_path());
    child
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    if child.spawn().is_err() {
        note(&format!("could not schedule {}", args.join(" ")));
    }
}

fn lease_after(name: &str) -> Result<()> {
    if !wait_power(name, true)? {
        note(&format!("{name} did not stay up; dongle B lease skipped"));
        return Ok(());
    }
    let Some(paths) = operator_paths() else {
        return Err(ForgeError::Host(
            "qemu hook operator file is missing".to_owned(),
        ));
    };
    let Some(own) = load_own(&paths, name)? else {
        return Ok(());
    };
    if own.role()? != Role::WhonixGw {
        return Ok(());
    }
    let _ = hostnet::allow_libvirt_forward();
    lease_if_needed(&paths)
}

fn release_after(name: &str) -> Result<()> {
    let _ = wait_power(name, false);
    let Some(paths) = operator_paths() else {
        return Ok(());
    };
    let Some(own) = load_own(&paths, name)? else {
        return Ok(());
    };
    match own.role()? {
        Role::WhonixGw => {
            let _ = hostnet::release_usb();
            let _ = hostnet::enforce();
        }
        Role::OsintClearnet => {
            let _ = hostnet::enforce();
        }
        Role::WhonixWs | Role::Isolated => {}
    }
    Ok(())
}

fn wait_power(name: &str, active: bool) -> Result<bool> {
    for _ in 0..40 {
        match virt::domstate(crate::profile::SYSTEM_URI, name) {
            Ok(power) if power.is_active() == active => return Ok(true),
            Ok(_) | Err(_) => std::thread::sleep(std::time::Duration::from_millis(250)),
        }
    }
    Ok(false)
}

fn judge(gate: &Gate<'_>) -> std::result::Result<Effect, String> {
    if matches!(gate.phase, Phase::Migrate | Phase::Attach | Phase::Other) {
        return Err(
            "Forge domains are not migrated, not attached from an external QEMU, and not started by an unknown hook"
                .to_owned(),
        );
    }
    if needs_digest(gate.phase) && !gate.digest_ok {
        return Err("base digest was not verified".to_owned());
    }
    if needs_backing(gate.phase) && !gate.backing_ok {
        return Err("overlay backing was not verified".to_owned());
    }
    if needs_policy(gate.phase) || gate.phase == Phase::Reconnect {
        if !gate.base_protected {
            return Err(
                "base must be a root:root regular file with mode 0444 and the immutable flag"
                    .to_owned(),
            );
        }
        crate::network::check_domain(gate.xml, gate.role, gate.network_mode)
            .map_err(|error| error.to_string())?;
        if matches!(gate.role, Role::WhonixGw | Role::WhonixWs)
            && !gate.private_xml.is_some_and(crate::network::private_ok)
        {
            return Err("forge-whonix must be an isolated private link without host IP or forwarding; run forge setup".to_owned());
        }
        if gate.network_mode == crate::network::Mode::Standard {
            let valid_nat = match gate.role {
                Role::WhonixGw => gate.wan_xml.is_some_and(|xml| {
                    crate::network::standard_nat_ok(xml, crate::profile::WAN_NET)
                }),
                Role::OsintClearnet => gate.standard_xml.is_some_and(|xml| {
                    crate::network::standard_nat_ok(xml, crate::network::STANDARD_NET)
                }),
                _ => true,
            };
            if !valid_nat {
                return Err(
                    "standard NAT network missing or drifted; run forge setup with guests stopped"
                        .to_owned(),
                );
            }
        }
        crate::security::validate_domain(gate.xml, gate.security_policy)
            .map_err(|e| e.to_string())?;
    }
    if matches!(gate.phase, Phase::Prepare | Phase::Start | Phase::Restore) {
        match gate.role {
            Role::WhonixGw
                if gate.network_mode == crate::network::Mode::Dedicated && gate.kali_running =>
            {
                return Err(
                    "one WAN guest at a time; stop kali before starting the gateway".to_owned(),
                );
            }
            Role::OsintClearnet
                if gate.network_mode == crate::network::Mode::Dedicated && gate.gw_running =>
            {
                return Err(format!(
                    "one WAN guest at a time; stop {WHONIX_GW_NAME} before starting kali"
                ));
            }
            Role::WhonixWs if !gate.gw_running => {
                return Err(format!("start {WHONIX_GW_NAME} before whonix-workstation"));
            }
            _ => {}
        }
        if gate.network_mode == crate::network::Mode::Dedicated
            && matches!(gate.role, Role::WhonixGw | Role::OsintClearnet)
            && gate.dongle_held
        {
            return Err(
                "dongle B is already open by another QEMU process; stop that guest first"
                    .to_owned(),
            );
        }
        if gate.network_mode == crate::network::Mode::Dedicated && gate.role == Role::WhonixGw {
            let wan = gate.wan_xml.ok_or_else(|| {
                format!("forge-wan is not defined; `forge start {WHONIX_GW_NAME}` once")
            })?;
            match gate.dongle {
                Dongle::Plugged => {
                    let iface = gate.dongle_iface.unwrap_or("");
                    if iface.is_empty() || !virt::wan_xml_ok(wan, Some(iface)) {
                        return Err(format!(
                            "forge-wan must NAT only out dongle B ({iface}); `forge start {WHONIX_GW_NAME}`"
                        ));
                    }
                }
                Dongle::Absent => {
                    if !virt::wan_xml_ok(wan, None) {
                        return Err(format!(
                            "dongle B is unplugged and forge-wan still NATs; `forge start {WHONIX_GW_NAME}`"
                        ));
                    }
                }
            }
        }
    }
    Ok(effect_for(gate))
}

fn effect_for(gate: &Gate<'_>) -> Effect {
    if gate.network_mode == crate::network::Mode::Standard {
        return Effect::None;
    }
    match gate.phase {
        // NetworkManager calls back into libvirt, so nmcli stays out of this process.
        Phase::Started if gate.role == Role::WhonixGw => Effect::LeaseAfter,
        Phase::Started if gate.role == Role::OsintClearnet => Effect::AttachAfter,
        Phase::Stopped | Phase::Release
            if matches!(gate.role, Role::WhonixGw | Role::OsintClearnet) =>
        {
            Effect::ReleaseAfter
        }
        _ => Effect::None,
    }
}

fn classify(arg: &str, xml: &str, own: Option<&Ownership>) -> Class {
    let forge_bits = xml::is_forge_domain(xml) || xml_uses_forge_store(xml);
    let Some(own) = own else {
        return if forge_bits {
            Class::Impostor(format!(
                "{arg} uses Forge metadata or a /var/lib/forge disk without an ownership record"
            ))
        } else {
            Class::Foreign
        };
    };
    if !vm_name_ok(arg) || own.name != arg {
        return Class::Impostor(format!(
            "ownership for {arg} does not match the domain name"
        ));
    }
    if element_text(xml, "name").as_deref() != Some(arg) {
        return Class::Impostor(format!("{arg}: domain XML name does not match"));
    }
    if element_text(xml, "uuid").as_deref() != Some(own.uuid.as_str()) {
        return Class::Impostor(format!(
            "{arg}: UUID does not match Forge ownership — refusing"
        ));
    }
    if !xml::is_forge_domain(xml) {
        return Class::Impostor(format!("{arg}: missing Forge metadata"));
    }
    let facts = xml::inspect(xml);
    if facts.forge_profile.as_deref() != Some(own.profile.as_str())
        || facts.forge_role.as_deref() != Some(own.role.as_str())
        || facts.forge_digest.as_deref() != Some(own.base_digest.as_str())
        || facts.forge_overlay.as_deref() != Some(own.overlay.as_str())
    {
        return Class::Impostor(format!("{arg}: Forge metadata does not match ownership"));
    }
    if !facts.disk_files.iter().any(|file| file == &own.overlay)
        || !facts.disk_files.iter().any(|file| file == &own.base)
    {
        return Class::Impostor(format!(
            "{arg}: disk XML is not the recorded overlay and base"
        ));
    }
    if !own.libvirt_uri.is_empty() && own.libvirt_uri != SYSTEM_URI {
        return Class::Impostor(format!("{arg}: ownership URI is not {SYSTEM_URI}"));
    }
    Class::Forge(own.clone())
}

pub(crate) fn verify_owned_domain(xml: &str, own: &Ownership) -> Result<()> {
    match classify(&own.name, xml, Some(own)) {
        Class::Forge(_) => Ok(()),
        Class::Impostor(reason) => Err(ForgeError::Ownership(reason)),
        Class::Foreign => Err(ForgeError::Ownership(
            "refusing to replace a non-Forge domain during setup".to_owned(),
        )),
    }
}

fn needs_digest(phase: Phase) -> bool {
    matches!(phase, Phase::Start | Phase::Restore)
}

fn needs_backing(phase: Phase) -> bool {
    matches!(phase, Phase::Prepare | Phase::Start | Phase::Restore)
}

fn needs_policy(phase: Phase) -> bool {
    matches!(
        phase,
        Phase::Prepare | Phase::Start | Phase::Restore | Phase::Started
    )
}

fn dongle_view(paths: &ForgePaths, role: Role) -> Result<(Dongle, Option<String>, bool)> {
    if !matches!(role, Role::WhonixGw | Role::OsintClearnet) {
        return Ok((Dongle::Absent, None, false));
    }
    let iface = dongle_iface(paths)?;
    let held = usb_node_held(paths)?;
    if iface.is_some() {
        Ok((Dongle::Plugged, iface, held))
    } else {
        Ok((Dongle::Absent, None, held))
    }
}

fn dongle_iface(paths: &ForgePaths) -> Result<Option<String>> {
    match usb::resolve_plugged(paths)? {
        usb::Resolve::Plugged(id) => usb::iface_for(id),
        usb::Resolve::PinnedMissing(_) | usb::Resolve::None => Ok(None),
    }
}

fn usb_node_held(paths: &ForgePaths) -> Result<bool> {
    let usb::Resolve::Plugged(id) = usb::resolve_plugged(paths)? else {
        return Ok(false);
    };
    Ok(node_held(usb::device_node(id).as_deref()))
}

fn node_held(node: Option<&Path>) -> bool {
    let Some(node) = node else {
        return false;
    };
    let Ok(want) = fs::metadata(node) else {
        return false;
    };
    let Ok(entries) = fs::read_dir(PID_DIR) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("pid") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(pid) = text.trim().parse::<i32>() else {
            continue;
        };
        if !pid_is_qemu(pid) {
            continue;
        }
        if process_has_dev(pid, want.dev(), want.ino()) {
            return true;
        }
    }
    false
}

fn process_has_dev(pid: i32, dev: u64, ino: u64) -> bool {
    let Ok(entries) = fs::read_dir(format!("/proc/{pid}/fd")) else {
        return false;
    };
    for entry in entries.flatten() {
        let Ok(meta) = fs::metadata(entry.path()) else {
            continue;
        };
        if meta.dev() == dev && meta.ino() == ino {
            return true;
        }
    }
    false
}

fn backing_matches(own: &Ownership) -> Result<bool> {
    let overlay = PathBuf::from(&own.overlay);
    let backing = virt::qemu_img_backing(&overlay)?
        .ok_or_else(|| ForgeError::Verify(format!("{} has no backing file", own.name)))?;
    let backing_canon = fs::canonicalize(&backing).unwrap_or_else(|_| PathBuf::from(&backing));
    let base_canon = fs::canonicalize(&own.base).unwrap_or_else(|_| PathBuf::from(&own.base));
    if backing_canon != base_canon {
        return Err(ForgeError::Verify(format!(
            "{} overlay backing {} is not the recorded base {}",
            own.name,
            backing_canon.display(),
            base_canon.display()
        )));
    }
    Ok(true)
}

fn load_own(paths: &ForgePaths, name: &str) -> Result<Option<Ownership>> {
    if !vm_name_ok(name) {
        return Ok(None);
    }
    let path = paths.ownership_file(name);
    if !path.is_file() {
        return Ok(None);
    }
    let own: Ownership = ownership::read_json(&path)?;
    Ok(Some(own))
}

fn operator_paths() -> Option<ForgePaths> {
    let text = fs::read_to_string(OPERATOR_PATH).ok()?;
    let meta = fs::symlink_metadata(OPERATOR_PATH).ok()?;
    if meta.uid() != 0 || meta.permissions().mode() & 0o022 != 0 || !meta.file_type().is_file() {
        return None;
    }
    let mut meta_path = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("meta=") {
            meta_path = Some(PathBuf::from(value.trim()));
        }
    }
    let meta_path = meta_path?;
    if !meta_path.is_absolute() || !meta_path.is_dir() {
        return None;
    }
    let mut paths = ForgePaths::discover();
    paths.meta = meta_path;
    Some(paths)
}

fn operator_matches(paths: &ForgePaths) -> Result<bool> {
    let text = fs::read_to_string(OPERATOR_PATH)
        .map_err(|_| ForgeError::Host(format!("{OPERATOR_PATH} is missing (forge dev hook)")))?;
    let meta = fs::symlink_metadata(OPERATOR_PATH)?;
    if meta.uid() != 0 || meta.permissions().mode() & 0o022 != 0 || !meta.file_type().is_file() {
        return Err(ForgeError::Host(format!(
            "{OPERATOR_PATH} must be a root-owned regular file"
        )));
    }
    let want_meta = paths.meta.to_str().unwrap_or("");
    let meta_ok = text.lines().any(|line| line == format!("meta={want_meta}"));
    let uid_ok = text
        .lines()
        .any(|line| line == format!("uid={}", paths::effective_uid()));
    Ok(meta_ok && uid_ok)
}

fn hook_ready(paths: &ForgePaths) -> Result<bool> {
    let Some(stamp) = trusted_stamp()? else {
        return Ok(false);
    };
    if !Path::new(OPERATOR_PATH).is_file()
        || !Path::new(crate::security::POLICY_PATH).exists()
        || !Path::new(crate::network::MODE_PATH).exists()
    {
        return Ok(false);
    }
    let exe = std::env::current_exe().map_err(|error| {
        ForgeError::Host(format!("cannot read the running forge binary: {error}"))
    })?;
    if file_sha256(&exe)? != stamp {
        return Ok(false);
    }
    crate::security::Policy::trusted()?;
    Ok(operator_matches(paths)? && daemon_sees_hook()?)
}

/// sha256 hex from the root stamp, or `None` when the hook was never installed.
fn trusted_stamp() -> Result<Option<String>> {
    trusted_stamp_at(Path::new("/var/lib/forge"))
}

fn trusted_stamp_at(root: &Path) -> Result<Option<String>> {
    let dir = match fs::symlink_metadata(root) {
        Ok(dir) => dir,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !dir.is_dir() || dir.uid() != 0 || dir.permissions().mode() & 0o022 != 0 {
        return Err(ForgeError::Host(
            "/var/lib/forge must stay root-owned and not writable by group or other, or the hook stamp cannot be trusted".to_owned(),
        ));
    }
    let stamp = root.join("hook-installed");
    let meta = match fs::symlink_metadata(&stamp) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ForgeError::Host(format!(
                "cannot stat {HOOK_STAMP}: {error}"
            )));
        }
    };
    if !meta.file_type().is_file() || meta.uid() != 0 || meta.permissions().mode() & 0o022 != 0 {
        return Err(ForgeError::Host(format!(
            "{HOOK_STAMP} must be a root-owned regular file"
        )));
    }
    let text = fs::read_to_string(stamp)?;
    Ok(stamp_sha256(&text).map(str::to_owned))
}

fn stamp_sha256(text: &str) -> Option<&str> {
    let hex = text.lines().find_map(|line| line.strip_prefix("sha256="))?;
    let hex = hex.trim();
    if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(hex)
    } else {
        None
    }
}

fn invoked_path_ok() -> bool {
    let Ok(meta) = fs::symlink_metadata(hook_path()) else {
        return false;
    };
    meta.file_type().is_file() && meta.uid() == 0 && meta.permissions().mode() & 0o022 == 0
}

fn invoked_as_installed_hook() -> bool {
    if !invoked_path_ok() {
        return false;
    }
    let Ok(exe) = fs::read_link("/proc/self/exe") else {
        return false;
    };
    if exe != Path::new(&hook_path()) {
        return false;
    }
    let Ok(hook_meta) = fs::metadata(hook_path()) else {
        return false;
    };
    let Ok(exe_meta) = fs::metadata("/proc/self/exe") else {
        return false;
    };
    hook_meta.dev() == exe_meta.dev() && hook_meta.ino() == exe_meta.ino()
}

fn daemon_sees_hook() -> Result<bool> {
    let Some(pid) = virtqemud_pid() else {
        return Ok(true);
    };
    let elapsed = cmd::run_checked("ps", &["-o", "etimes=", "-p", &pid.to_string()])?;
    let elapsed: u64 = elapsed
        .trim()
        .parse()
        .map_err(|_| ForgeError::Host(format!("cannot read virtqemud age ({elapsed})")))?;
    let hook_mtime = fs::metadata(HOOK_STAMP)?
        .modified()
        .map_err(|error| ForgeError::Host(format!("cannot stat {HOOK_STAMP}: {error}")))?;
    let daemon_start = SystemTime::now()
        .checked_sub(Duration::from_secs(elapsed))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    Ok(daemon_start + Duration::from_secs(2) >= hook_mtime)
}

fn virtqemud_pid() -> Option<i32> {
    let text = cmd::run_checked("pidof", &["virtqemud"])
        .or_else(|_| cmd::run_checked("pidof", &["libvirtd"]))
        .ok()?;
    text.split_whitespace().next()?.parse().ok()
}

fn guest_qemu_running() -> bool {
    let Ok(entries) = fs::read_dir(PID_DIR) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("pid") {
            return false;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            return false;
        };
        text.trim()
            .parse::<i32>()
            .is_ok_and(|pid| pid > 1 && pid_is_qemu(pid))
    })
}

fn qemu_running(name: &str) -> bool {
    if !vm_name_ok(name) {
        return false;
    }
    let Ok(text) = fs::read_to_string(format!("{PID_DIR}/{name}.pid")) else {
        return false;
    };
    text.trim()
        .parse::<i32>()
        .is_ok_and(|pid| pid > 1 && pid_is_qemu(pid))
}

fn pid_is_qemu(pid: i32) -> bool {
    let Ok(bytes) = fs::read(format!("/proc/{pid}/cmdline")) else {
        return false;
    };
    qemu_cmdline(&bytes)
}

fn qemu_cmdline(bytes: &[u8]) -> bool {
    let Some(executable) = bytes.split(|byte| *byte == 0).next() else {
        return false;
    };
    let Ok(executable) = std::str::from_utf8(executable) else {
        return false;
    };
    let name = Path::new(executable).file_name().and_then(|name| name.to_str());
    matches!(name, Some("qemu-kvm"))
        || name.is_some_and(|name| name.starts_with("qemu-system-") && name.len() > "qemu-system-".len())
}

fn xml_uses_forge_store(xml: &str) -> bool {
    xml::inspect(xml)
        .disk_files
        .iter()
        .any(|file| file.starts_with("/var/lib/forge/"))
}

fn element_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].trim().to_owned())
}

fn phase_of(op: &str) -> Phase {
    match op {
        "prepare" => Phase::Prepare,
        "start" => Phase::Start,
        "started" => Phase::Started,
        "stopped" => Phase::Stopped,
        "release" => Phase::Release,
        "migrate" => Phase::Migrate,
        "restore" => Phase::Restore,
        "reconnect" => Phase::Reconnect,
        "attach" => Phase::Attach,
        _ => Phase::Other,
    }
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Prepare => "prepare",
        Phase::Start => "start",
        Phase::Started => "started",
        Phase::Stopped => "stopped",
        Phase::Release => "release",
        Phase::Migrate => "migrate",
        Phase::Restore => "restore",
        Phase::Reconnect => "reconnect",
        Phase::Attach => "attach",
        Phase::Other => "other",
    }
}

fn subop_ok(phase: Phase, sub: &str) -> bool {
    match phase {
        Phase::Stopped | Phase::Release => sub == "end",
        Phase::Other => false,
        _ => sub == "begin",
    }
}

fn vm_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 48
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn safe_abs(path: &str) -> Result<&str> {
    if !path.starts_with('/')
        || path.contains('\'')
        || path.contains('\n')
        || path.contains('\r')
        || path.contains(' ')
    {
        return Err(ForgeError::Host(format!(
            "refusing to pass '{path}' through the root install script"
        )));
    }
    Ok(path)
}

fn file_sha256(path: &Path) -> Result<String> {
    crate::hash::sha256_file(path, &progress::noop)
}

fn read_xml() -> Result<String> {
    let mut stdin = io::stdin();
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let n = stdin.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        if buf.len() + n > XML_CAP {
            return Err(ForgeError::InvalidInput(
                "domain XML exceeds 2 MiB".to_owned(),
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    String::from_utf8(buf)
        .map_err(|_| ForgeError::InvalidInput("domain XML is not utf-8".to_owned()))
}

fn note(text: &str) {
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(HOOK_LOG) else {
        return;
    };
    let _ = writeln!(file, "{text}");
    let _ = file.flush();
}

#[cfg(test)]
mod tests {
    #[test]
    fn qemu_process_detection_supports_el10_without_matching_guest_arguments() {
        assert!(super::qemu_cmdline(b"/usr/libexec/qemu-kvm\0-name\0guest=whonix-gateway\0"));
        assert!(super::qemu_cmdline(b"/usr/bin/qemu-system-x86_64\0-name\0guest=kali\0"));
        assert!(!super::qemu_cmdline(b"/usr/bin/editor\0qemu-system-x86_64\0"));
        assert!(!super::qemu_cmdline(b"/usr/bin/not-qemu-kvm\0"));
        assert!(!super::qemu_cmdline(b"\0"));
    }
    use super::*;
    use crate::xml::{DomainSpec, domain_xml};

    const UUID: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

    #[test]
    fn fresh_host_has_no_hook_stamp() {
        let missing = std::env::temp_dir().join(format!("forge-fresh-{}", uuid::Uuid::new_v4()));
        assert_eq!(trusted_stamp_at(&missing).unwrap(), None);
        assert!(!missing.exists());
    }

    fn own_kali() -> Ownership {
        Ownership {
            name: "kali".to_owned(),
            uuid: UUID.to_owned(),
            profile: "kali".to_owned(),
            role: "osint-clearnet".to_owned(),
            overlay: "/var/lib/forge/vms/kali.qcow2".to_owned(),
            base: "/var/lib/forge/bases/kali.qcow2".to_owned(),
            base_digest: "sha256:abc".to_owned(),
            libvirt_uri: SYSTEM_URI.to_owned(),
        }
    }

    fn kali_xml() -> String {
        domain_xml(&DomainSpec {
            name: "kali",
            uuid: UUID,
            profile: "kali",
            role: Role::OsintClearnet,
            overlay: "/var/lib/forge/vms/kali.qcow2",
            base: "/var/lib/forge/bases/kali.qcow2",
            base_digest: "sha256:abc",
            memory_mib: 2048,
            vcpus: 2,
        })
    }

    fn gw_xml() -> String {
        domain_xml(&DomainSpec {
            name: "whonix-gateway",
            uuid: UUID,
            profile: "whonix",
            role: Role::WhonixGw,
            overlay: "/var/lib/forge/vms/whonix-gateway.qcow2",
            base: "/var/lib/forge/bases/whonix-gateway.qcow2",
            base_digest: "sha256:abc",
            memory_mib: 2048,
            vcpus: 2,
        })
    }

    fn ws_xml() -> String {
        domain_xml(&DomainSpec {
            name: "whonix-workstation",
            uuid: UUID,
            profile: "whonix",
            role: Role::WhonixWs,
            overlay: "/var/lib/forge/vms/whonix-workstation.qcow2",
            base: "/var/lib/forge/bases/whonix-workstation.qcow2",
            base_digest: "sha256:abc",
            memory_mib: 2048,
            vcpus: 2,
        })
    }

    fn gate(phase: Phase, role: Role, xml: &str) -> Gate<'_> {
        Gate {
            phase,
            role,
            xml,
            gw_running: false,
            kali_running: false,
            wan_xml: None,
            dongle: Dongle::Absent,
            dongle_iface: None,
            dongle_held: false,
            digest_ok: true,
            backing_ok: true,
            security_policy: crate::security::Policy::Selinux,
            base_protected: true,
            network_mode: crate::network::Mode::Dedicated,
            private_xml: Some(
                "<network><name>forge-whonix</name><bridge name='virbr-forgewx'/></network>",
            ),
            standard_xml: None,
        }
    }

    #[test]
    fn apparmor_start_is_accepted_only_with_matching_policy() {
        let xml = kali_xml()
            .replace(
                "model='selinux' relabel='yes'",
                "model='apparmor' relabel='yes'",
            )
            .replace("          <seclabel model='selinux' relabel='no'/>\n", "");
        let mut gate = gate(Phase::Prepare, Role::OsintClearnet, &xml);
        gate.security_policy = crate::security::Policy::Apparmor;
        assert!(judge(&gate).is_ok());
        gate.security_policy = crate::security::Policy::Selinux;
        assert!(judge(&gate).is_err());
    }

    #[test]
    fn mutable_base_blocks_start_but_does_not_block_release() {
        let xml = kali_xml();
        let mut gate = gate(Phase::Prepare, Role::OsintClearnet, &xml);
        gate.base_protected = false;
        assert!(judge(&gate).is_err());
        gate.phase = Phase::Release;
        assert!(judge(&gate).is_ok());
    }

    fn standard_domain(role: Role) -> String {
        crate::xml::domain_xml_with(
            &DomainSpec {
                name: "fixture",
                uuid: UUID,
                profile: "fixture",
                role,
                overlay: "/var/lib/forge/vms/fixture.qcow2",
                base: "/var/lib/forge/bases/fixture.qcow2",
                base_digest: "sha256:abc",
                memory_mib: 2048,
                vcpus: 2,
            },
            crate::security::Policy::Selinux,
            crate::network::Mode::Standard,
        )
    }

    #[test]
    fn standard_kali_needs_no_dongle_and_can_run_with_gateway() {
        let xml = standard_domain(Role::OsintClearnet);
        let nat = crate::network::standard_nat_xml(crate::network::STANDARD_NET);
        let mut gate = gate(Phase::Prepare, Role::OsintClearnet, &xml);
        gate.network_mode = crate::network::Mode::Standard;
        gate.standard_xml = Some(&nat);
        gate.gw_running = true;
        gate.dongle_held = true;
        assert_eq!(judge(&gate).unwrap(), Effect::None);
        for phase in [Phase::Started, Phase::Stopped, Phase::Release] {
            gate.phase = phase;
            assert_eq!(
                judge(&gate).unwrap(),
                Effect::None,
                "standard mode must never schedule USB or NetworkManager workers"
            );
        }
        gate.phase = Phase::Prepare;
        gate.standard_xml = None;
        assert!(judge(&gate).is_err());
    }

    #[test]
    fn standard_gateway_uses_host_nat_and_no_b_lease() {
        let xml = standard_domain(Role::WhonixGw);
        let nat = crate::network::standard_nat_xml(crate::profile::WAN_NET);
        let mut gate = gate(Phase::Prepare, Role::WhonixGw, &xml);
        gate.network_mode = crate::network::Mode::Standard;
        gate.wan_xml = Some(&nat);
        gate.kali_running = true;
        assert_eq!(judge(&gate).unwrap(), Effect::None);
        gate.phase = Phase::Reconnect;
        assert_eq!(judge(&gate).unwrap(), Effect::None);
        gate.private_xml = None;
        assert!(judge(&gate).is_err());
    }

    #[test]
    fn standard_workstation_still_requires_gateway_and_private_link() {
        let xml = standard_domain(Role::WhonixWs);
        let mut gate = gate(Phase::Prepare, Role::WhonixWs, &xml);
        gate.network_mode = crate::network::Mode::Standard;
        assert!(judge(&gate).is_err());
        gate.gw_running = true;
        assert!(judge(&gate).is_ok());
        gate.private_xml = Some(
            "<network><name>forge-whonix</name><bridge name='virbr-forgewx'/><forward mode='nat'/></network>",
        );
        assert!(judge(&gate).is_err());
    }

    #[test]
    fn a_different_configured_mode_cannot_silently_change_vm_connectivity() {
        let xml = standard_domain(Role::Isolated);
        assert!(judge(&gate(Phase::Prepare, Role::Isolated, &xml)).is_err());
    }

    #[test]
    fn foreign_domain_is_not_ours() {
        let xml =
            "<domain><name>win11</name><uuid>bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb</uuid></domain>";
        assert!(matches!(classify("win11", xml, None), Class::Foreign));
    }

    #[test]
    fn forge_disk_without_ownership_is_refused() {
        let xml = "<domain><name>extra</name><disk><source file='/var/lib/forge/vms/kali.qcow2'/></disk></domain>";
        assert!(matches!(classify("extra", xml, None), Class::Impostor(_)));
    }

    #[test]
    fn uuid_mismatch_is_refused() {
        let mut own = own_kali();
        own.uuid = "cccccccc-cccc-cccc-cccc-cccccccccccc".to_owned();
        assert!(matches!(
            classify("kali", &kali_xml(), Some(&own)),
            Class::Impostor(_)
        ));
    }

    #[test]
    fn matching_kali_is_forge() {
        let own = own_kali();
        assert!(matches!(
            classify("kali", &kali_xml(), Some(&own)),
            Class::Forge(_)
        ));
    }

    #[test]
    fn start_without_digest_is_refused() {
        let xml = kali_xml();
        let mut gate = gate(Phase::Start, Role::OsintClearnet, &xml);
        gate.digest_ok = false;
        assert!(judge(&gate).is_err());
    }

    #[test]
    fn kali_prepare_quarantines_usb_and_does_not_lease() {
        let xml = kali_xml();
        let mut gate = gate(Phase::Prepare, Role::OsintClearnet, &xml);
        gate.dongle = Dongle::Plugged;
        gate.dongle_iface = Some("enp2s0f0u1");
        assert_eq!(judge(&gate).expect("kali"), Effect::None);
    }

    #[test]
    fn workstation_prepare_does_not_touch_the_lease() {
        let xml = ws_xml();
        let mut gate = gate(Phase::Prepare, Role::WhonixWs, &xml);
        gate.gw_running = true;
        assert_eq!(judge(&gate).expect("ws"), Effect::None);
    }

    #[test]
    fn workstation_requires_gateway() {
        let xml = ws_xml();
        let gate = gate(Phase::Prepare, Role::WhonixWs, &xml);
        assert!(judge(&gate).unwrap_err().contains(WHONIX_GW_NAME));
    }

    #[test]
    fn gateway_and_kali_are_not_simultaneous() {
        let xml = kali_xml();
        let mut gate = gate(Phase::Prepare, Role::OsintClearnet, &xml);
        gate.gw_running = true;
        assert!(judge(&gate).is_err());
    }

    #[test]
    fn gateway_nats_only_out_the_dongle() {
        let xml = gw_xml();
        let wan = "<network><name>forge-wan</name><forward mode='nat'><interface dev='enp2s0f0u1'/></forward><bridge name='virbr-forgewan'/><ip address='10.0.2.2' netmask='255.255.255.0'/></network>";
        let mut gate = gate(Phase::Prepare, Role::WhonixGw, &xml);
        gate.dongle = Dongle::Plugged;
        gate.dongle_iface = Some("enp2s0f0u1");
        gate.wan_xml = Some(wan);
        assert_eq!(judge(&gate).expect("gw"), Effect::None);
        gate.phase = Phase::Started;
        assert_eq!(judge(&gate).expect("gw later"), Effect::LeaseAfter);
    }

    #[test]
    fn gateway_refuses_nat_without_a_usb_iface() {
        let xml = gw_xml();
        let wan = "<network><forward mode='nat'/><bridge name='virbr-forgewan'/><ip address='10.0.2.2'/></network>";
        let mut gate = gate(Phase::Prepare, Role::WhonixGw, &xml);
        gate.dongle = Dongle::Plugged;
        gate.dongle_iface = Some("enp2s0f0u1");
        gate.wan_xml = Some(wan);
        let err = judge(&gate).unwrap_err();
        assert!(err.contains("NAT only"), "{err}");
    }

    #[test]
    fn unplugged_gateway_must_not_keep_nat() {
        let xml = gw_xml();
        let wan = "<network><forward mode='nat'><interface dev='enp2s0f0u1'/></forward><bridge name='virbr-forgewan'/><ip address='10.0.2.2'/></network>";
        let mut gate = gate(Phase::Prepare, Role::WhonixGw, &xml);
        gate.wan_xml = Some(wan);
        assert!(judge(&gate).is_err());
    }

    #[test]
    fn unplugged_gateway_with_isolated_wan_releases_the_lease() {
        let xml = gw_xml();
        let wan = "<network><name>forge-wan</name><bridge name='virbr-forgewan'/><ip address='10.0.2.2' netmask='255.255.255.0'/></network>";
        let mut gate = gate(Phase::Prepare, Role::WhonixGw, &xml);
        gate.wan_xml = Some(wan);
        assert_eq!(judge(&gate).expect("isolated"), Effect::None);
    }

    #[test]
    fn pci_hostdev_is_refused() {
        let mut xml = gw_xml();
        xml = xml.replace(
            "</devices>",
            "<hostdev mode='subsystem' type='pci' managed='yes'/></devices>",
        );
        let wan = "<network><name>forge-wan</name><bridge name='virbr-forgewan'/><ip address='10.0.2.2' netmask='255.255.255.0'/></network>";
        let mut gate = gate(Phase::Prepare, Role::WhonixGw, &xml);
        gate.wan_xml = Some(wan);
        let err = judge(&gate).unwrap_err();
        assert!(err.contains("PCI"), "{err}");
    }

    #[test]
    fn virbr0_is_refused() {
        let xml = kali_xml().replace(
            "</devices>",
            "<interface type='bridge'><source bridge='virbr0'/></interface></devices>",
        );
        let gate = gate(Phase::Prepare, Role::OsintClearnet, &xml);
        assert!(judge(&gate).unwrap_err().contains("NAT"));
    }

    #[test]
    fn stopping_the_workstation_does_not_drop_the_gateway_lease() {
        let xml = ws_xml();
        let gate = gate(Phase::Stopped, Role::WhonixWs, &xml);
        assert_eq!(judge(&gate).expect("stop ws"), Effect::None);
    }

    #[test]
    fn stopping_the_gateway_releases_b() {
        let xml = gw_xml();
        let gate = gate(Phase::Stopped, Role::WhonixGw, &xml);
        assert_eq!(judge(&gate).expect("stop gw"), Effect::ReleaseAfter);
    }

    #[test]
    fn migrate_is_refused() {
        let xml = kali_xml();
        let gate = gate(Phase::Migrate, Role::OsintClearnet, &xml);
        assert!(judge(&gate).is_err());
    }

    #[test]
    fn reconnect_does_not_rehash_or_touch_usb() {
        let xml = kali_xml();
        let mut gate = gate(Phase::Reconnect, Role::OsintClearnet, &xml);
        gate.digest_ok = false;
        gate.backing_ok = false;
        assert_eq!(judge(&gate).expect("reconnect"), Effect::None);
    }

    #[test]
    fn held_dongle_blocks_kali() {
        let xml = kali_xml();
        let mut gate = gate(Phase::Start, Role::OsintClearnet, &xml);
        gate.dongle_held = true;
        gate.dongle = Dongle::Plugged;
        assert!(judge(&gate).unwrap_err().contains("another QEMU"));
    }

    #[test]
    fn stamp_sha_is_64_hex() {
        assert_eq!(
            stamp_sha256(
                "sha256=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n"
            ),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert_eq!(stamp_sha256("sha256=short\n"), None);
        assert_eq!(stamp_sha256("nope\n"), None);
    }

    #[test]
    fn safe_abs_rejects_spaces_and_quotes() {
        assert!(safe_abs("/home/major/.local/bin/forge").is_ok());
        assert!(safe_abs("/tmp/evil'").is_err());
        assert!(safe_abs("relative").is_err());
    }

    #[test]
    fn subop_begin_and_end() {
        assert!(subop_ok(Phase::Prepare, "begin"));
        assert!(!subop_ok(Phase::Prepare, "end"));
        assert!(subop_ok(Phase::Stopped, "end"));
        assert!(!subop_ok(Phase::Other, "begin"));
    }
}
