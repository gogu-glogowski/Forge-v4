# Arch installer validation — 2026-10-20

Candidate: Forge 4.2.2-5, x86_64. Native host: CachyOS with GNOME/Wayland,
Limine, AppArmor and system libvirt. Physical guest isolation is not certified.

## Automated checks

- Rust workspace: 150 tests passed (134 core, 11 CLI integration, 5 music).
- Python installer: 34 tests passed, including local libvirt downgrade regression,
  AppArmor profile/capabilities readiness and canonical package release paths.
- Clippy: all workspace targets, warnings denied, passed.
- Native makepkg build: completed with runtime dependency checks and root-owned
  payload; the package contains forge-install and matching source archive.

## Native observations

- All four existing domains were shut off before testing.
- The images-only stage verified/reused Tsurugi, Kali and both Whonix bases.
- Release 4 prerequisites explicitly targeted every dependency, which downgraded
  local confined libvirt 1:12.8.0-1.2 to repository 1:12.8.0-1.1. A previously
  loaded daemon/profile masked the replacement until setup checked the profile
  on disk and stopped. Release 5 targets only missing dependencies during Syu
  and host readiness checks the installed helper profile as well as capabilities.
- A previously built confined libvirt package was inspected for its metadata,
  helper and profiles and retained for local host recovery.

## Still required

- Complete the release-5 installer after restoring confined libvirt.
- Confirm hook reload, all four domain roles and final doctor result.
- Start/stop guests in GNOME Boxes and inspect AppArmor on actual QEMU processes.
- Verify guest connectivity in Extended, loss of connectivity when USB B is
  physically unplugged, absence of guest fallback to PCI A, and restoration of
  Standard policy. Confirm Whonix Workstation uses Gateway and Tsurugi isolation.

Existing images/domains alone are not evidence that the new installer completed.
An ordinary future libvirt upgrade can also replace the custom build; retain
capability/profile checks and rebuild the confined package when needed.
