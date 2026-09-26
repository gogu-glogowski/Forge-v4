# Forge 4.0

Fedora KVM lab. Similar idea to Qubes OS: separate guests, separate roles, separate networks.

Greenfield after [v2](https://github.com/gogu-glogowski/Forge-v2) and [v3](https://github.com/gogu-glogowski/Forge-v3).

**Now:** Tsurugi, Kali, and the Whonix pair `pull` / `create` / `start` / `stop` on Fedora 44. GNOME Boxes lists the guests (`QEMU System` → `qemu:///system`). SIFT's OVA is behind SANS Portal: `FORGE_SIFT_OVA=/path/to.ova forge pull sift`.

Dongle **B** is optional to boot. Plug it later; `forge start kali` / `whonix-gateway` uses it if present (pin `FORGE_DONGLE_B=vvvv:pppp` or `forge dev usb`). Isolated guests never get it. One WAN guest at a time: gateway **or** Kali, never both. virt-manager is spare.

Five guests, four roles.

| You type | Role | Network |
|----------|------|---------|
| `tsurugi`, `sift` | `isolated` | no NIC |
| `whonix` (pair) | `whonix-gw` + `whonix-ws` | Gateway: USB dongle **B** only; Workstation: internal only |
| `kali` | `osint-clearnet` | USB dongle **B** only (not at the same time as Gateway) |

Host Fedora uses **provider A** (onboard Ethernet). Guests never use A. You plug and unplug cables.

---

## Prerequisites

**Host: Fedora Workstation 44** (current stable; `forge dev doctor` will refuse older). Keep it updated.

```bash
sudo dnf upgrade --refresh
```

Reboot if the kernel changed.

**Virtualization + build deps:**

```bash
sudo dnf install @virtualization gnome-boxes virt-manager virt-viewer
sudo dnf install git gcc rust cargo libvirt-devel
sudo systemctl enable --now libvirtd
sudo usermod -aG libvirt "$USER"
```

Start and stop from the CLI (`forge start` / `forge stop`). GNOME Boxes (rpm, not Flatpak) is the display — open the guest after `forge start`, work, close the window. Do not create VMs from Boxes. After `forge` writes `~/.config/gnome-boxes/sources/QEMU System`, if Boxes was already open, quit it fully and reopen. **virt-manager** is the spare.

Log out and back in so the `libvirt` group applies.

```bash
virsh -c qemu:///system list --all
```

Forge uses **`qemu:///system`**. Do not run `sudo forge`.

**Hardware**

- Onboard Ethernet = **A** — only the host, only while installing/updating Fedora, libvirt, Rust, Forge, and while `forge pull` downloads images. Then unplug (or `nmcli device disconnect`).
- USB-C → Ethernet dongle (preferred) or USB Wi-Fi = **B** — only VMs. Cables are the default; Wi-Fi is the same role.

Build from source (no COPR yet):

```bash
git clone https://github.com/gogu-glogowski/Forge-v4.git
cd Forge-v4
cargo build --release -p forge-cli
mkdir -p ~/.local/bin
install -m 755 target/release/forge ~/.local/bin/forge
command -v forge
forge dev doctor
```

If `forge dev doctor` is not green, stop.

`forge --help` shows **user** commands. Extra tools live under `forge dev`.

---

## Commands

```bash
forge pull kali          # fetch image into a base
forge create kali        # overlay VM named kali, role osint-clearnet
forge start kali
forge status kali
forge stop kali
forge clone kali kali-2
forge delete kali-2
```

Same pattern: `tsurugi`, `sift`. Whonix is one pull and one create for the pair:

```bash
forge pull whonix
forge create whonix
forge start whonix-gateway
forge start whonix-workstation
```

`forge list` — profiles, whether the image is on disk, VM names.

`forge status` with no name — whole lab.

Do **not** `sudo forge`. Stay yourself; Forge asks for sudo only when it needs `/var/lib/forge`.

---

## Developer mode

Not in default `--help`:

```bash
forge dev doctor           # host fit
forge dev xml <vm>         # domain dump
forge dev create --dry-run …
forge dev delete --dry-run …
forge dev usb              # dongle B pin
```

`forge delete` is a user command. `--dry-run` is `dev`.

---

## License

Apache-2.0
