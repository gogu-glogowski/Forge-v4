# Forge 4.2.0

Forge downloads and verifies Tsurugi, SIFT, Kali and Whonix images, prepares KVM
virtual machines and exposes them through **GNOME Boxes → QEMU System**. Bases
are immutable in `/var/lib/forge/bases`; writable qcow2 overlays live in
`/var/lib/forge/vms`. Forge uses `qemu:///system` and a root-owned libvirt hook to
check image hashes, disk backing chains, confinement and network roles at startup.

[![Ten kod brzmi lepiej z muzyką — odpal na YouTube](assets/music-banner.svg)](https://www.youtube.com/watch?v=CZfbJlhUxw8)

## Install the DEB release

For amd64 Pop!_OS 24.04 / Ubuntu 24.04 and newer compatible systems, download
the release package:

```bash
sudo apt install ./forge_4.2.0-1_amd64.deb
```

APT installs the declared runtime dependencies, including virsh; no Rust toolchain
is required for a binary installation. Music streams automatically from this repository during `prepare --all`; no music package is needed.
This build requires **libc6 >= 2.39**. Debian 12 and Ubuntu/Pop!_OS 22.04 require a
build on an older compatible host; Debian/Ubuntu live validation remains pending.
See [DEB installation, upgrade and removal](packaging/debian/README.md).

Enable the appropriate libvirt daemon/socket family, join `libvirt,kvm`, then log
out/in as described below. With guests stopped, run `/usr/bin/forge setup` and
`/usr/bin/forge prepare --all`. Package installation itself does not reconfigure
a running lab. A previous `~/.local/bin/forge` may shadow the DEB binary;
check `type -a forge`. Music attribution is in `/usr/share/forge/music`; local audio is optional.

## Choose a network mode

| Mode | Internet for Kali and Whonix Gateway | Extra hardware |
| --- | --- | --- |
| **Standard** (`standard`) — default for new labs | Uses the host's normal connection through libvirt NAT | None |
| **Dedicated network** (`dedicated`) — advanced | Host uses A; guest internet uses USB Ethernet adapter B | Separate interfaces/cables; not necessarily separate internet providers |

In **both modes**, Tsurugi and SIFT have no NIC. Whonix Workstation has exactly
one NIC on the private `forge-whonix` link and reaches the internet only through
Whonix Gateway. The gateway must start first. Do not add a direct internet NIC to
the workstation in Boxes or another libvirt tool.

Standard mode does not disconnect adapters, change NetworkManager profiles or
require dongle B. Kali uses the DHCP-enabled `forge-standard` NAT network; Whonix
Gateway uses its own `forge-wan` NAT network at `10.0.2.2`, preserving the network
layout expected by the upstream image. Kali and Gateway can run concurrently.
Standard mode shares the host uplink; it does not provide the separate physical
network path of dedicated mode. Normal NAT permits guest access to reachable LAN
resources, subject to the host and guest firewalls.

Existing labs with ownership records and no saved mode retain **dedicated**
semantics. An upgrade never silently moves an A/B lab onto the host connection.

## 1. Prepare the host

The current scope is an **x86_64 desktop with systemd, NetworkManager and KVM**.
Enable CPU virtualization in firmware. Install native GNOME Boxes rather than
using only its Flatpak build, and keep SELinux/AppArmor enabled.

For Fedora 44+:

```bash
sudo dnf install @virtualization gnome-boxes git gcc rust cargo \
  gnupg2 curl 7zip tar xz e2fsprogs policycoreutils-python-utils \
  NetworkManager iproute psmisc
for drv in qemu interface network nodedev nwfilter secret storage; do
  sudo systemctl enable --now "virt${drv}d.socket"
done
sudo systemctl enable --now virtlogd.socket virtlockd.socket
sudo usermod -aG libvirt,kvm "$USER"
```

For Debian / Ubuntu / Pop!_OS, the runtime package names are:

```bash
sudo apt install qemu-system-x86 qemu-utils libvirt-clients libvirt-daemon-system \
  gnome-boxes apparmor gnupg curl 7zip tar xz-utils e2fsprogs \
  network-manager iproute2 sysvinit-utils sudo
sudo systemctl enable --now libvirtd.service
sudo usermod -aG libvirt,kvm "$USER"
```

On Debian 12 or Ubuntu/Pop!_OS 22.04, use `p7zip-full` instead of `7zip`. If the
installation uses modular libvirt daemons, inspect their sockets instead of
turning on a second daemon family; doctor reports the detected units. The desktop
must already use NetworkManager. Forge does not replace another network manager.

**Log out and back in** after changing groups. Check access with:

```bash
virsh -c qemu:///system list --all
```

Compilation separately requires **Rust 1.89+**, Cargo, Git and a C compiler. Use a
sufficiently recent toolchain; the Rust package in an older distribution may not
meet this requirement. Optional fallback tools are `virt-manager`/`virt-viewer`.

## 2. Build and configure Forge

```bash
git clone https://github.com/gogu-glogowski/Forge-v4.git
cd Forge-v4
cargo build --release --locked -p forge-cli
mkdir -p "$HOME/.local/bin"
install -m 755 target/release/forge "$HOME/.local/bin/forge"
export PATH="$HOME/.local/bin:$PATH"
forge setup
```

This branch contains the port changes; apply the port patch before building an
unmodified upstream checkout. Keep `~/.local/bin` on PATH in future terminals.
Run Forge as your normal user. Forge asks for sudo only for privileged setup and
storage operations; do not run `sudo forge`.

`forge setup` uses **Standard** for a new lab and preserves the saved mode on
subsequent runs. To select a mode explicitly:

```bash
forge setup --network standard
# Advanced alternative, only when you want the separate A/B network:
forge setup --network dedicated
```

Stop **all QEMU guests**, including paused guests, before setup or switching mode.
Keep them stopped until setup finishes. Setup records the mode in the trusted
root-owned `/var/lib/forge/network-mode`, installs the hook, prepares Forge's
networks and regenerates the network XML of existing owned VMs. It verifies VM
identity before rewriting definitions. Disks and guest data are retained.

If setup fails after saving the mode, old VM XML is rejected by the hook until it
matches the selected mode. Fix the reported problem and repeat setup with guests
stopped. Mode switching does not restore NetworkManager settings changed during
earlier dedicated use; configure those host connections explicitly if needed.

Forge owns `/etc/libvirt/hooks/qemu`. Resolve conflicts with an existing custom
hook before installing Forge. Reopen Boxes after setup so its QEMU System source
appears. Forge manages only its named networks, not libvirt's `default` network.
It validates definitions before reuse and refuses to rebuild a drifted standard
network while guests are active. Its current NAT subnets are `192.168.124.0/24`
and `10.0.2.0/24`; resolve conflicts with existing host networks before use.

Libvirt manages NAT rules. Forge retains its firewalld forwarding preparation
when firewalld is active. It does not disable UFW or change UFW's global defaults;
if UFW or custom nftables/iptables rules block guest forwarding, those rules need
scoped integration and testing. No internet availability is assumed from XML
validation alone.

## Prepare everything in one command

The DEB recommends mpv. For a source installation, install it with
`sudo apt install mpv` or `sudo dnf install mpv`, then run:

```bash
forge prepare --all
```

`prepare --all` performs setup, downloads/verifies Tsurugi, Kali and both Whonix
bases, and creates their default VM overlays. It preserves the saved network
mode; new labs use Standard. Guests must remain stopped. Nothing auto-starts.
Completed bases are verified and reused; existing matching owned VMs keep their
overlays. Cached downloads can resume. A missing/incomplete base referenced by
an existing VM causes a refusal rather than replacing that VM's backing disk.
If a verification fails, fix the reported problem before rerunning.

SIFT is explicitly skipped when its base is unavailable, because SANS requires a
manual download. To include it, supply the local OVA:

```bash
forge prepare --all --sift-ova "$HOME/Downloads/SIFT-Workstation.ova"
```

Background music starts by default during preparation: 18 calm instrumental
tracks, **2 h 59 min 51.5 s**, streamed from this GitHub repository by mpv at 100% volume with shuffle and
repeat. Local user libraries are reused when present. Streaming is buffered and
stops with preparation; it does not download the entire library in advance. `--no-music` disables playback; `--music-dir /path/to/music` selects a
custom library. Missing player/library or sound errors never stop the image
workflow. Playback stops on completion, error or Ctrl+C. See
[the music library](assets/music/README.md) for installation and adding tracks.

The bundle is by Kevin MacLeod (incompetech.com), under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/), with
[full attribution](assets/music/CREDITS.md). Preserve those notices when
redistributing it. It is licensed separately from Forge code.

The individual `pull`/`create` commands below remain available.

## 3. Pull verified images

```bash
forge pull tsurugi
forge pull kali
forge pull whonix
```

Pull verifies upstream signatures/checksums with the public keys in `keys/`.
Whonix uses the pinned LXQt **18.2.1.9** libvirt bundle for both guests. Downloads
are cached under `${XDG_DATA_HOME:-$HOME/.local/share}/forge/cache` and can be
resumed. Allow room for archives, extraction, bases and growing overlays; large
profiles may need hundreds of GiB.

SIFT requires a manual OVA download after signing in through the
[SANS SIFT page](https://www.sans.org/tools/sift-workstation):

```bash
FORGE_SIFT_OVA="$HOME/Downloads/SIFT-Workstation.ova" forge pull sift
```

Forge checks the published SHA-256 and converts the disk. It does not sign in to
SANS. Do not repeat `pull` as a routine guest update after overlays use a base;
the current implementation replaces the base at the same path. Update guest
operating systems inside their VMs instead. Isolated guests have no network.

## 4. Create and use VMs

| Profile | VM names | RAM / vCPUs per VM |
| --- | --- | --- |
| `tsurugi` | `tsurugi` | 8 GiB / 4 |
| `sift` | `sift` | 8 GiB / 4 |
| `kali` | `kali` | 4 GiB / 2 |
| `whonix` | `whonix-gateway`, `whonix-workstation` | 2 GiB / 2 + 4 GiB / 2 |

Have enough RAM for the selected guests plus the host. Create only profiles you
pulled; each `create` runs once and does not start the VM:

```bash
forge create kali
forge create whonix
forge list
forge dev doctor
```

Open VMs in **Boxes → QEMU System** and use Play. Do not import the same disks
through Boxes' New VM wizard. For Whonix, start Gateway first, wait for it to be
ready, then start Workstation. Shut down Workstation first, then Gateway. The
upstream guests have their own first-run setup and passwords; Forge does not
provision guest accounts. Tor can take time to establish connectivity.

```bash
forge start whonix-gateway
forge start whonix-workstation
forge status
forge stop whonix-workstation
forge stop whonix-gateway
```

In Standard mode, no cable preparation, USB pin or B lease is required. USB uplink
`connect`/`disconnect` and `forge dev cables` are refused in this mode so they
cannot accidentally reconfigure the host connection.

Closing a Boxes window may pause/save a guest rather than shutting it down. Use
the guest shutdown action and verify `forge status` or `virsh ... list --all`.
The hook checks base hashes at startup; hashing large bases takes time.

`forge clone <vm> <new-name>` copies a stopped overlay; Whonix cloning is refused.
`forge delete <vm>` removes a stopped owned VM and its overlay, retaining the base.
Use Forge for deletion to keep ownership records consistent.

## Optional dedicated A/B mode

This advanced path is retained for later hardware validation; Standard mode does
not depend on it. In Dedicated mode, host A is onboard/PCI Ethernet and B is a
separate USB Ethernet adapter connected to an IPv4 DHCP router. USB Wi-Fi is not
a substitute in the existing Whonix workflow.

`forge dev usb` lists B candidates. With multiple adapters, put the chosen
`vvvv:pppp` ID in `${XDG_DATA_HOME:-$HOME/.local/share}/forge/dongle-b` for Boxes;
a terminal environment variable is not inherited by the root hook.

With guests stopped, `forge dev cables` disconnects USB network interfaces from
the host and disables their Ethernet profiles' autoconnect/IP configuration. Use
host A while doing this. Run only Kali **or** Whonix Gateway at a time. Kali gets
B as a USB device; Gateway instead uses B as the host exit of `forge-wan`.
Workstation remains on the private gateway link.

Run `forge start whonix-gateway` to prepare/recover B before returning to Boxes.
The existing dedicated workstation check requires a current B lease. Without B,
Gateway can boot offline and `forge-wan` must remain isolated; it never falls back
to host A. `forge connect kali` and `forge disconnect kali` control live USB
attachment in Dedicated mode only.

## Confinement and port status

Bases are root-owned `0444` files with the immutable flag. Their XML skips DAC
ownership changes and, on SELinux, base relabeling. Writable overlays inherit
libvirt's dynamic ownership/confinement. Forge selects verified enforcing
SELinux or libvirt's enabled AppArmor driver and stores the policy in
`/var/lib/forge/security-policy`. Unknown confinement is not treated as success.

AppArmor setup adds narrow image-read rules to the local `virt-aa-helper`
override and reloads that helper profile. Per-VM profiles remain generated by
libvirt. Existing local rules are preserved; global guest access is not added.
The helper profile needs its standard local include and `apparmor_parser`.
SELinux uses `semanage`/`restorecon`; preparation failures stop setup.

The hook validates the saved mode, VM role and confinement without calling back
into libvirt. Standard mode schedules no USB/NetworkManager workers. Network mode
and security policy files must remain root-owned and not writable by other users.

**Pop!_OS 24.04 Standard mode has live smoke-test evidence:** native Boxes,
Kali/Tsurugi start/stop hooks, enforcing AppArmor, and the active Whonix topology.
Gateway has NAT plus the private link; Workstation has only the private link.
The user confirmed Tor Browser and updates in Workstation. This is not a full
Tor leak audit. Doctor reports a port-readiness warning on Pop!_OS 24.04;
Debian/Ubuntu retain a failure until separately validated. Complete firewall
integration across distributions remains pending. A/B hardware tests are deferred;
Standard mode needs no second adapter. AppImage packaging is not included.

## Updating, backups and development

Stop all guests before updating Forge and reinstalling its hook:

```bash
git pull --ff-only
cargo build --release --locked -p forge-cli
install -m 755 target/release/forge "$HOME/.local/bin/forge"
forge setup
forge dev doctor
```

Keep `/var/lib/forge` together with `${XDG_DATA_HOME:-$HOME/.local/share}/forge`
when backing up the lab. Use `forge dev xml <vm>` and
`sudo tail -n 50 /var/lib/forge/hook.log` for diagnosis. Use one operator account.

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Tests use small disk/archive fixtures, signing keys and network-policy cases.
They do not install hooks or alter the test host's network.

References: [libvirt network XML](https://libvirt.org/formatnetwork.html),
[hooks](https://libvirt.org/hooks.html),
[security drivers](https://libvirt.org/drvqemu.html), and
[Ubuntu AppArmor integration](https://ubuntu.com/server/docs/how-to/virtualisation/libvirt/).
License: [Apache-2.0](LICENSE).
