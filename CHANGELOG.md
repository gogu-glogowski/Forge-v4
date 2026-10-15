# Changelog

## 4.2.0 — 2026-10-15

- Detect Fedora, Debian, Ubuntu and Pop!_OS hosts, libvirt services, QEMU accounts,
  confinement and runtime tools; provide distribution-specific dependency hints.
- Default new labs to Standard NAT, with no second network adapter required.
  Retain Dedicated A/B mode for existing labs; hardware validation is deferred.
- Support enforcing AppArmor alongside SELinux; protect immutable bases without
  libvirt DAC ownership changes or inappropriate base relabeling.
- Integrate native GNOME Boxes with QEMU System and enforce Forge policies through
  libvirt hooks, including start and stop paths initiated outside the CLI.
- Add resumable `forge prepare --all`, verified-base reuse and preservation of
  existing VM overlays. Skip unavailable SIFT explicitly; accept `--sift-ova`.
- Add default GitHub music streaming via mpv, shuffled/looped at 100%, with
  `--no-music`, `--music-dir`, cleanup on end/error/interrupt and nonfatal failures.
- Bundle Forge Calm: 18 Kevin MacLeod recordings (20 playback files), about three hours,
  separately licensed CC BY 4.0 with attribution, hashes and source links.
- Provide one small amd64 `forge` DEB with runtime dependencies, music attribution,
  library ABI checks and isolated package lifecycle tests. Audio stays online in
  the repository; no separate music DEB is required. Local libraries remain usable.
- Live smoke checks on Pop!_OS 24.04: Boxes start/stop hooks, Kali/Tsurugi guests,
  AppArmor confinement and Whonix Gateway NAT/private Workstation topology.
  The user confirmed Tor Browser and updates in Workstation. This is not a
  complete Tor leak audit or full Debian/Ubuntu/firewall validation.
- AppImage is not part of this release. Existing A/B workflow remains experimental.
