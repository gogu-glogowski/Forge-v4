# 4.2.2 — Arch installer (native validation in progress)

- Full Arch-family installation: checked prerequisites, Standard/Extended choice,
  all automatic image downloads and verification, matching network/hook setup,
  four stopped VMs and final role/doctor checks.
- Separate `prepare --images-only` stage leaves hooks and VM definitions untouched.
- Extended is the installer name for Dedicated A/B; IPv4 and IPv6 host routes
  are checked again after downloads and before taking USB B.
- Resumable restart boundary after host prerequisites; installed `forge-install`
  includes matching installer sources. Python is an explicit package dependency.
- SIFT stays available for manual import, outside automatic installation.
- Arch package release 5 fixes the AppArmor capability XML path and requires the
  virt-aa-helper profile on disk, including after a libvirt package replacement.
- Dependency setup upgrades the system and explicitly installs only missing
  dependencies; it no longer downgrades a newer local confined libvirt by naming
  every already-satisfied dependency as a pacman install target.
- Host-stage and builder package paths follow the canonical PKGBUILD release.
- Native CachyOS image reuse/verification passed for all four bases. Guest
  lifecycle and physical A/B isolation validation remain pending.

# 4.2.1 — installer candidate

- Single `install.sh` entry point; offline plan by default.
- Standard/Dedicated policy summary, image selection and final confirmation.
- `forge prepare --profiles kali,whonix` reuses verified, resumable preparation.
- Whonix remains an inseparable Gateway/Workstation pair.
- Installer refuses older binaries without profile selection before network changes.
- Host deployment and physical A/B validation remain separate from local tests.

# Changelog

## Unreleased — physical installation lab and experimental ports

- Add Void glibc/runit discovery, XBPS packaging and isolated lifecycle checks.
- Add a NixOS source package/module, native libvirt layout and root-owned AppArmor helper setup.
- Recognize Mint, Zorin, AlmaLinux and Rocky; add an EL10 RPM target.
- Add Polish installation templates, desktop selection and post-install Ansible configuration.
- Keep disk choice interactive and existing Ventoy files unchanged; native hardware validation is pending.


## 4.2.0 — multi-distribution packages (2026-10-15)

- Add unsigned Fedora 44+ x86_64 binary RPM and native-build recipe.
- Add Arch/Manjaro rolling recognition, pacman dependency hints and native Boxes detection.
- Add checksum-pinned PKGBUILD/source archive and .pkg.tar.zst builder.
- Preserve confinement requirements: official Arch libvirt 12.8.0 disables both
  AppArmor and SELinux drivers. Document the required compatible libvirt build and
  provide a local recipe patch helper. Manjaro capabilities and native VM smoke
  tests remain pending; no silent DAC-only fallback is introduced.
- Publish DEB, RPM and Arch downloads with checksums and official distribution logos in README.

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
