//! AlmaLinux adapter. Shared Forge orchestration/security remains authoritative.
use std::path::Path;
use crate::{cmd, Forge, ForgeError, NetworkMode, Result};
use crate::progress::Progress;

pub fn run(
    bundle: &Path,
    choose: impl FnOnce() -> Result<(NetworkMode, bool)>,
    progress: &Progress,
) -> Result<crate::PrepareReport> {
    let os = crate::host::OsRelease::parse(&std::fs::read_to_string("/etc/os-release")?);
    if os.distribution != crate::host::Distribution::Alma || os.major_version() != Some(10)
        || std::env::consts::ARCH != "x86_64" {
        return Err(ForgeError::Host("This installer supports AlmaLinux 10 x86_64 only".to_owned()));
    }
    let bundle = bundle.canonicalize()?;
    let bundle = bundle.to_str().ok_or_else(|| ForgeError::InvalidInput("bundle path is not UTF-8".to_owned()))?;
    let environment = cmd::run_checked("systemd-detect-virt", &["--vm"]);
    eprintln!("Environment: {}", environment.unwrap_or_else(|_| "physical host or unrecognized virtualization; KVM checked next".to_owned()));
    let memory = std::fs::read_to_string("/proc/meminfo")?;
    eprintln!("Resources: {} CPUs; {}", std::thread::available_parallelism().map(|v| v.get()).unwrap_or(1), memory.lines().find(|l| l.starts_with("MemAvailable:")).unwrap_or("available RAM unknown"));
    let operator = cmd::run_checked("id", &["-un"])?;
    let uid = crate::effective_uid();
    cmd::authenticated(|| {
        let running = cmd::priv_capture("if command -v virsh >/dev/null; then virsh -c qemu:///system list --name 2>/dev/null || true; fi")?;
        if !running.trim().is_empty() {
            return Err(ForgeError::Host("Stop all QEMU guests before installation; existing data will be preserved".to_owned()));
        }
        cmd::priv_script(&prerequisites_script(bundle, &operator, uid))?;
        let (mode, home_storage) = choose()?;
        if home_storage { cmd::priv_script(HOME_STORAGE)?; }
        cmd::priv_script(&format!("set -eu\ncd {}\nsha256sum -c SHA256SUMS >&2\ndnf -y --enablerepo=crb --enablerepo=epel --setopt=install_weak_deps=False install ./forge-*.rpm >&2\n", cmd::shell_quote(bundle)))?;
        let forge = Forge::open()?;
        forge.setup_network(Some(mode))?;
        let report = forge.prepare_all(None, progress)?;
        // Authenticated firewall and hook/network checks are performed by setup.
        // Domain metadata and power state are checked without starting guests.
        for vm in forge.status(None)? {
            vm.role_ok?;
            if vm.power.is_active() {
                return Err(ForgeError::Host("A guest started during installation; cannot declare completion".to_owned()));
            }
        }
        Ok(report)
    })
}

fn prerequisites_script(bundle: &str, operator: &str, uid: u32) -> String {
    format!(r#"set -eu
[ -c /dev/kvm ] || {{ echo 'KVM is unavailable; enable host/nested virtualization' >&2; exit 1; }}
[ -d /run/systemd/system ] || exit 1
[ "$(getenforce)" = Enforcing ] || {{ echo 'SELinux must remain Enforcing' >&2; exit 1; }}
cd {bundle}
sha256sum -c SHA256SUMS >&2
rpm -q epel-release >/dev/null 2>&1 || dnf -y install epel-release >&2
dnf -y --enablerepo=crb --enablerepo=epel --setopt=install_weak_deps=False install \
 ./libvirt-gconfig-*.rpm ./libvirt-gobject-*.rpm ./gnome-boxes-*.rpm \
 qemu-kvm qemu-img libvirt-client libvirt-daemon-kvm gnupg2 curl ca-certificates \
 7zip tar xz e2fsprogs NetworkManager iproute procps-ng psmisc systemd sudo \
 coreutils util-linux policycoreutils policycoreutils-python-utils libselinux-utils \
 selinux-policy-targeted mpv firewalld >&2
[ "$(id -u {operator})" = {uid} ] || exit 1
getent group libvirt >/dev/null || groupadd --system libvirt
usermod -aG libvirt,kvm {operator}
systemctl enable --now NetworkManager.service firewalld.service >&2
systemctl enable --now virtqemud.socket virtnetworkd.socket virtstoraged.socket \
 virtnodedevd.socket virtnwfilterd.socket virtsecretd.socket virtinterfaced.socket \
 virtlogd.socket virtlockd.socket >&2
"#, bundle=cmd::shell_quote(bundle), operator=cmd::shell_quote(operator))
}

const HOME_STORAGE: &str = r#"set -eu
if mountpoint -q /var/lib/forge; then exit 0; fi
[ ! -L /home ] && [ "$(stat -c %u /home)" = 0 ] && [ "$(stat -c %a /home)" = 755 ]
[ ! -L /var/lib/forge ]
if [ -d /var/lib/forge ] && [ -n "$(find /var/lib/forge -mindepth 1 -maxdepth 1 -print -quit)" ]; then
 echo 'Existing Forge data needs verified migration; preserve its current storage' >&2; exit 1
fi
[ ! -e /home/forge-storage ] && [ ! -L /home/forge-storage ]
install -d -o root -g root -m 0755 /home/forge-storage /var/lib/forge
semanage fcontext -a -e /var/lib/forge /home/forge-storage
restorecon -RF /home/forge-storage
mount --bind /home/forge-storage /var/lib/forge
printf '\n/home/forge-storage /var/lib/forge none bind,x-systemd.requires-mounts-for=/home/forge-storage 0 0\n' >> /etc/fstab
systemctl daemon-reload
"#;
