# Forge 4.0

Greenfield sibling of [Forge v2](https://github.com/gogu-glogowski/Forge-v2) and [Forge v3](https://github.com/gogu-glogowski/Forge-v3). Inspired by them; not a rewrite.

**This cut is documentation only.** There is no engine, CLI, or GUI yet.

Forge 4.0 is a Fedora-host KVM/libvirt manager for a small, role-locked lab:

| Profile | Role |
|---------|------|
| Tsurugi LAB | `isolated` |
| SIFT Workstation | `isolated` |
| Kali | `osint-clearnet` |
| Whonix Gateway | `whonix-gw` |
| Whonix Workstation | `whonix-ws` |

No other guests. No default NAT. No `type='user'` / passt uplink.

- Host onboard Ethernet: Fedora maintenance, then unplug. Later, the same port may be passed through to Kali (`osint-clearnet`) — not while the host is using it.
- USB-C Ethernet dongle (different provider): **only** Whonix Gateway, USB hostdev passthrough.
- Whonix Workstation: internal link to Gateway only.
- Tsurugi and SIFT: no NIC.

Architecture (operator contract, Polish): [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Status

| | |
|--|--|
| Engine | not started |
| Host target | Fedora, `qemu:///system` |
| License | Apache-2.0 |

## Lineage (what 4.0 deliberately drops)

- Forge 2.5 `default` NAT / `virbr0` for Kali and Fedora guests.
- Forge 3.0 QEMU user-mode NAT (`<interface type='user'>`) and Gateway `passt` uplink.

Whonix Workstation remains without any uplink, as in 2.5/3.0.
