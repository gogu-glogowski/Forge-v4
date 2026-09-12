# Forge 4.0

Fedora-first KVM/libvirt lab. Greenfield after [v2](https://github.com/gogu-glogowski/Forge-v2) and [v3](https://github.com/gogu-glogowski/Forge-v3) — we keep fail-closed ownership and signed images, not the command maze.

**This cut is documentation only.** No engine yet. Contract: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

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
sudo dnf install @virtualization virt-manager virt-viewer
sudo dnf install git gcc rust cargo libvirt-devel
sudo systemctl enable --now libvirtd
sudo usermod -aG libvirt "$USER"
```

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
forge doctor
```

If `doctor` is not green, stop. It does not silently fix the host.

---

## Everyday commands

No `vm plan`. No `image inspect` / `image fetch`. No `profile list` + `image list` as two rituals.

```bash
forge pull kali          # download official qcow2 + verify (HTTPS + signed checksum)
forge create kali        # VM named kali, role osint-clearnet
forge start kali
forge status kali        # running + role/network proof
forge stop kali
forge clone kali kali-2  # clone by VM name, not by file
forge delete kali-2
```

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

## Developer commands

Kept because they earn their place:

| Command | Why |
|---------|-----|
| `forge doctor` | Host really fits Forge (Fedora 44, KVM, libvirtd, URI, no default NAT on our domains) |
| `forge delete <name>` | Fail-closed remove of an owned VM (needed in real use too) |

Dropped from v2 (overgrowth, not 4.0):

`vm plan`, `vm create --dry-run` as the happy path, `image inspect`, `image fetch`, `image prepare*`, Fedora Workstation **guest**, `fresh`, `adopt`, `rebuild`, `state recover` as daily tools, `profile list` / `image list` as a pair.

`--dry-run` may exist on `create` / `delete` for us. It is not in the README workflow.

---

## License

Apache-2.0
