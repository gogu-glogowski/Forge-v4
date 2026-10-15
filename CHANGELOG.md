## AlmaLinux 10 native Boxes candidate (2026-10-15)

- Add a builder for GNOME Boxes 50.0 using verified upstream source and AlmaLinux libraries.
- Document signed AlmaLinux Devel runtime dependencies without globally enabling Devel.
- EL10 Forge RPM release 2 retains GNOME Boxes and recognizes /usr/libexec/qemu-kvm.
- Keep experimental readiness status pending physical GUI/VM and confinement validation.

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
