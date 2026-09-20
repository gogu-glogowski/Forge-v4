# Forge 4.0

Fedora-first KVM/libvirt lab. Greenfield after [v2](https://github.com/gogu-glogowski/Forge-v2) and [v3](https://github.com/gogu-glogowski/Forge-v3) — we keep fail-closed ownership and signed images, not the command maze.

Contract: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

**Now:** Tsurugi, Kali, and the Whonix pair `pull` / `create` / `start` / `stop` on Fedora 44. GNOME Boxes lists the overlays (`QEMU System` → `qemu:///system`). SIFT’s OVA is still behind SANS Portal: `FORGE_SIFT_OVA=/path/to.ova forge pull sift`.

Dongle **B** is optional to boot — without it guests have no WAN, and that is expected. Plug it later; `forge start kali` / `whonix-gateway` attaches it live if present (pin `FORGE_DONGLE_B=vvvv:pppp` or `forge dev usb`). Isolated never gets it. One VM at a time: gateway **or** Kali, never both. virt-manager is spare.

Five guests, four roles. Nothing else.

| You type | Role | Network |
|----------|------|---------|
| `tsurugi`, `sift` | `isolated` | no NIC |
| `whonix` (pair) | `whonix-gw` + `whonix-ws` | Gateway: USB dongle **B** only; Workstation: internal only |
| `kali` | `osint-clearnet` | USB dongle **B** only (not at the same time as Gateway) |

Host Fedora uses **provider A** (onboard Ethernet). Guests never use A. Human plugs and unplugs cables.

---

## Prerequisites (external user)

**Host: Fedora Workstation 44** (current stable; `doctor` will refuse older). Keep it updated.

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

**Start and stop from the CLI** (`forge start` / `forge stop`). That is the control plane: base digest, role XML, dongle B attach/detach, exclusive B, Whonix order. GNOME Boxes (rpm, not Flatpak) is the **display** — open the guest after `forge start`, work, close the window. Play/Stop in Boxes talks to libvirt directly and skips Forge. Do not create VMs from Boxes (session + NAT). `forge` writes `~/.config/gnome-boxes/sources/QEMU System` (`qemu:///system`); if Boxes was already open, quit it fully and reopen. **virt-manager** is the spare (XML).

Log out and back in so the `libvirt` group applies.

```bash
virsh -c qemu:///system list --all
```

Forge uses **`qemu:///system`**. Do not turn off SELinux. Do not give Forge NOPASSWD sudo.

**Hardware**

- Onboard Ethernet = **A** — only the host, only while installing/updating Fedora, libvirt, Rust, Forge, and while `forge pull` downloads images. Then unplug (or `nmcli device disconnect`).
- USB-C → Ethernet dongle (preferred) or USB Wi-Fi = **B** — only VMs, via passthrough. Cables are the default; Wi-Fi is the same role, worse radio.

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

If `forge dev doctor` is not green, stop. It does not silently fix the host.

`forge --help` shows **user** commands only. Diagnostics live under `forge dev`.

---

## User commands

No `vm plan`. No `image inspect` / `image fetch`. No background hash daemon.

Each VM is an **overlay** on a hashed **base**. Bases are **not** libvirt domains, not in a Boxes or virt-manager pool, and get `chattr +i` after `pull` — they must not appear as VMs and must not be started. Only overlays show up, start, clone, and delete. Your work changes the overlay; that is not an alarm. `start`/`status` re-check the base digest and the **role** (NICs, dongle). They do not hash the overlay.

```bash
forge pull kali          # download + verify (curl; resumes .part if the mirror drops)
forge create kali        # overlay VM named kali, role osint-clearnet
forge start kali         # refuses if base digest or role XML drifted
forge status kali        # running + role/network proof
forge stop kali
forge clone kali kali-2  # clone by VM name, not by file
forge delete kali-2
```

`pull` asks for **your** sudo password **immediately** (from a real terminal), then keeps **one root helper process** until the hashed qcow2 is installed (`chattr +i`). That is not a sudo timestamp and not NOPASSWD — the helper is already root, so a 16 GiB download will not prompt again at 5 a.m. Do **not** `sudo forge`. HTTP goes through curl (resume `.part`). A finished OVA/7z/bundle in cache is hashed and reused. Fetcher: `crates/forge-core/src/download.rs`.

Do **not** `sudo forge`. Root’s `secure_path` misses `~/.local/bin/forge` (`command not found`), and overlays would be owned by root (`Permission denied (os error 13)`). Stay yourself; let Forge call `sudo` only for those few install/chmod steps.

Same pattern: `tsurugi`, `sift`. Whonix is one pull and one create for the pair:

```bash
forge pull whonix
forge create whonix
forge start whonix-gateway
forge start whonix-workstation
```

`forge list` — one inventory (proposal, replaces v2 `profile list` + `image list`): which profiles exist, whether the qcow2 is on disk, VM names.

`forge status` with no name — whole lab, including “is this NIC allowed for this role?” and “is the dongle in two VMs?”.

---

## Developer mode

Not in default `--help`:

```bash
forge dev doctor           # host fit (Fedora 44, KVM, libvirtd, Boxes rpm, no NAT on our domains)
forge dev xml <vm>         # domain XML vs role
forge dev create --dry-run …
forge dev delete --dry-run …
```

`forge delete` itself is a **user** command (you need it). `--dry-run` is `dev`.

Dropped from v2 (overgrowth, not 4.0):

`vm plan`, `image inspect`, `image fetch`, `image prepare*`, Fedora Workstation **guest**, `fresh`, `adopt`, `rebuild`, `state recover` as daily tools, `profile list` / `image list` as a pair, a background image-hash daemon.

---

## License

Apache-2.0
