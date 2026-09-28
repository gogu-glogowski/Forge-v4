# Forge v4

Forge prepares Tsurugi, SIFT, Kali and a Whonix Gateway/Workstation pair as KVM virtual machines on Fedora. It downloads and verifies upstream images, keeps base disks in `/var/lib/forge/bases`, and creates writable qcow2 overlays in `/var/lib/forge/vms`. **GNOME Boxes is the everyday GUI**; Forge handles image preparation, VM creation and the libvirt hook used by Boxes' Play button.

[![Ten kod brzmi lepiej z muzyką — odpal na YouTube](assets/music-banner.svg)](https://www.youtube.com/watch?v=CZfbJlhUxw8)

<sub>🎧 Otwórz link w nowej karcie, włącz muzykę i wróć do repo.</sub>

| Profile | VM names | RAM / vCPUs per VM | Network |
| --- | --- | --- | --- |
| `tsurugi` | `tsurugi` | 8 GiB / 4 | No NIC |
| `sift` | `sift` | 8 GiB / 4 | No NIC |
| `kali` | `kali` | 4 GiB / 2 | USB dongle B, attached after startup |
| `whonix` | `whonix-gateway`, `whonix-workstation` | 2 GiB / 2 + 4 GiB / 2 | Gateway exits through B; Workstation uses the private gateway link |

## 1. Prepare a fresh Fedora host

Use **Fedora Workstation 44 x86_64**, with CPU virtualization enabled in firmware, SELinux enabled and a local desktop session with sudo access. Doctor accepts Fedora 44 or newer; this workflow was checked on 44. Have enough RAM for the selected guests **plus Fedora**, and room for downloads, extracted images, bases and growing overlays. Pull keeps a cache under `${XDG_DATA_HOME:-$HOME/.local/share}/forge/cache`; large images can need hundreds of GiB across the cache and `/var/lib/forge`.

The intended network setup is onboard/PCI Ethernet **A** for Fedora and a separate **USB Ethernet B with IPv4 DHCP** for guest internet. A can stay connected. The gateway's B path creates an Ethernet connection, so USB Wi-Fi is not a substitute for B in this Whonix workflow. Tsurugi and SIFT need no B.

```bash
sudo dnf upgrade --refresh
sudo dnf install @virtualization gnome-boxes git gcc rust cargo \
  gnupg2 curl 7zip tar xz e2fsprogs policycoreutils-python-utils \
  NetworkManager iproute psmisc
```

Reboot after a kernel update. Use the **RPM version of Boxes** for this system-libvirt setup. On a fresh Fedora installation, enable the modular libvirt sockets:

```bash
for drv in qemu interface network nodedev nwfilter secret storage; do
  sudo systemctl enable --now "virt${drv}d.socket"
done
sudo systemctl enable --now virtlogd.socket virtlockd.socket
sudo usermod -aG libvirt "$USER"
```

**Log out and back in**, then check access:

```bash
virsh -c qemu:///system list --all
```

Forge always uses `qemu:///system`. Run all `forge` commands as your normal user; Forge requests sudo for its storage and hook installation. NetworkManager/firewalld may also ask for desktop authorization. Use one operator account for this lab.

## 2. Build and install Forge

```bash
git clone https://github.com/gogu-glogowski/Forge-v4.git
cd Forge-v4
cargo build --release --locked -p forge-cli
mkdir -p "$HOME/.local/bin"
install -m 755 target/release/forge "$HOME/.local/bin/forge"
export PATH="$HOME/.local/bin:$PATH"
forge --help
forge list
```

Rust 1.89 or newer is required. Ensure `~/.local/bin` is also on PATH in future terminals. `forge list` creates the Boxes source `~/.config/gnome-boxes/sources/QEMU System` (or under `XDG_CONFIG_HOME`). Fully quit and reopen Boxes if it was already open.

## 3. Pull the images you want

Keep host internet A connected. Run these from a terminal and enter sudo when requested at the beginning of each pull:

```bash
forge pull tsurugi
forge pull kali
forge pull whonix
```

These prepare the Tsurugi LAB OVA, Kali's current QEMU amd64 archive, and the pinned Whonix LXQt **18.2.1.9** libvirt bundle. Forge verifies OpenPGP signatures/checksums with the public keys included in `keys/`, then installs the base disks. Whonix is one download for both guests. A failed download can be resumed by repeating its pull before creating VMs.

**SIFT requires a manual download.** Sign in through the [SANS SIFT page](https://www.sans.org/tools/sift-workstation), download its OVA, then pass the actual file path:

```bash
FORGE_SIFT_OVA="$HOME/Downloads/SIFT-Workstation.ova" forge pull sift
```

Adjust the path to your download directory and filename. Forge compares it with the SHA-256 published on the SANS page and converts the disk. It does not sign in to SANS or download the OVA for you. No SANS credentials belong in this repository.

```bash
forge list
```

Selected profiles should show `ready`. You do not need to pull every profile to use one of them. Do not repeat `pull` as a routine guest update once overlays use that base: the current implementation replaces the base at the same path. Update the installed OS inside the guest instead; isolated guests intentionally have no network.

## 4. Install the hook and check the setup

Shut down all QEMU guests before installing or updating the hook:

```bash
forge dev hook
forge dev usb
```

This installs the root-owned executable `/etc/libvirt/hooks/qemu`, records the operator and prepares the shared network lock. It restarts the active QEMU libvirt daemon. Forge owns this hook path; if you already have a different qemu hook, resolve that conflict before running the installer.

With exactly one USB network adapter, B is selected automatically. With multiple adapters, copy its `vvvv:pppp` ID from `forge dev usb` into `${XDG_DATA_HOME:-$HOME/.local/share}/forge/dongle-b`. For example, after replacing the placeholder:

```bash
printf '%s\n' 'vvvv:pppp' > "${XDG_DATA_HOME:-$HOME/.local/share}/forge/dongle-b"
```

Use this persistent file for Boxes: an environment variable set in a terminal is not inherited by the libvirt hook. Keep the default Forge data location for this workflow.

With guests stopped, prepare B and run diagnostics:

```bash
forge dev cables
forge dev doctor
```

`cables` disconnects USB network interfaces from Fedora and disables their Ethernet profiles' autoconnect/IP configuration; use onboard A for the host. Doctor is expected to report missing storage/hook/Boxes setup if run before the preceding steps. Resolve every `FAIL` before continuing; a firewalld forwarding warning is handled by the first gateway start.

## 5. Create VMs and open them in Boxes

Create only the profiles you pulled:

```bash
forge create tsurugi
forge create sift
forge create kali
forge create whonix
```

Each `create` runs once. It defines the VM and overlay; it does not power it on. Whonix always creates the two fixed names shown above. Do not use Boxes' New VM wizard to recreate/import these disks.

Tsurugi, SIFT and Kali can now start with **Play in Boxes → QEMU System**. Plug B in before starting Kali to have the hook attach it automatically. Without B, Kali starts offline.

For Whonix, plug B into its DHCP router and perform the initial network preparation from the terminal:

```bash
forge start whonix-gateway
```

Forge waits for NetworkManager to install the route through B before enabling the B-only NAT network. If this fails, it leaves `forge-wan` isolated and reports the preparation error without starting the gateway; fix B/router connectivity and retry the same command.

Open the gateway in Boxes. Then Play `whonix-workstation`. If it reports that the B lease is not ready, wait a few seconds and retry. Complete the upstream guests' first-run setup inside Boxes; Forge does not provision guest accounts or change their passwords. Whonix also needs time to establish Tor connectivity.

## Everyday use

- Start and use the prepared VMs from Boxes. The hook checks base hashes, backing disks and role/network settings on startup; hashing large bases can take time.
- For Whonix, **start Gateway first**, wait for its B lease, then start Workstation. Shut down **Workstation first**, then Gateway. The workstation's hook currently requires a working B lease; the offline gateway alone can boot without B.
- Run only one internet-facing guest at a time: Kali **or** Whonix Gateway. Shut down the Whonix pair before switching to Kali. Keep custom Kali VMs/clones stopped while using the gateway too.
- Shut down through the guest OS or Boxes' shutdown action. Closing the Boxes window can save/pause a guest; it is not proof that it has shut down. Check `forge status` or `virsh -c qemu:///system list --all` before switching guests or updating the hook.
- If B was unplugged, changed interface, or the hook refuses stale XML/network settings: stop the Whonix pair and run `forge start whonix-gateway` once to prepare it again, then return to Boxes. Play validates the setup; it does not create or rewrite libvirt networks.

Useful terminal commands:

```bash
forge list
forge status
forge connect kali       # attach B if plugged in after Kali started
forge disconnect kali    # detach B; keep Kali running
forge stop kali          # request shutdown; wait until it is shut off
forge stop whonix-workstation
forge stop whonix-gateway
```

`connect`/`disconnect` control Kali's USB device. The gateway keeps B on the host as its network exit; use the stop/start sequence above for Whonix. Only CLI `forge stop whonix-gateway` refuses a normal shutdown while the workstation is active; the hook cannot veto that order in Boxes.

`forge clone <vm> <new-name>` copies an overlay (shut the source down first); Whonix cloning is refused. `forge delete <vm>` removes a stopped Forge VM and its overlay, preserving the base. Use Forge for deletion so its records remain consistent.

To update Forge itself, stop all guests, then in the checkout:

```bash
git pull --ff-only
cargo build --release --locked -p forge-cli
install -m 755 target/release/forge "$HOME/.local/bin/forge"
forge dev hook
forge dev doctor
```

Reinstall the hook after every binary update. Keep `/var/lib/forge` together with `${XDG_DATA_HOME:-$HOME/.local/share}/forge` when backing up the lab; the latter contains verification and VM ownership records. For diagnosis use `forge dev xml <vm>` and `sudo tail -n 50 /var/lib/forge/hook.log`. `virt-manager`/`virt-viewer` are optional fallback tools.

## Development checks

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Install `rustfmt` and `clippy` with DNF if needed. Tests use small temporary disk/archive fixtures and public signing keys; they do not replace a real Boxes/USB/Tor test.

References: [libvirt hooks](https://libvirt.org/hooks.html), [libvirt daemons](https://libvirt.org/daemons.html). License: [Apache-2.0](LICENSE).
