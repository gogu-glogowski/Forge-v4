# AlmaLinux 10 x86_64 — integrated installer candidate

The revision 10 bundle replaces the former package-only AlmaLinux entrypoint.
Run `bash install-almalinux.sh` as the normal operator from the extracted bundle.
Do not prefix it with sudo. Keep all QEMU guests off for setup/hook replacement.

Flow: music on/off first; one persistent sudo helper; host/KVM/SELinux checks and
prerequisites; explicit Standard or ADVANCED/Dedicated selection; protected
storage choice; Forge installation; shared setup/hook and preparation of Tsurugi,
Kali, Whonix Gateway and Workstation; role/power checks and JSON report.
Existing verified bases/cache and VM overlays are reused. SIFT stays manual.
Music remains optional and failure does not bypass guest verification.

The selected mode is applied through the existing Rust engine. Standard permits
concurrent guests with role isolation. Dedicated retains A/B USB ownership and
requires suitable interfaces/cabling. Neither mode is permission to drop host
confinement. Host identity is compared before/after installation.

The helper remains alive across package installation and downloads; privileged
libvirt operations also use it, including before new groups take effect in the
desktop session. Job output is framed separately from helper control responses.
No global Polkit bypass or passwordless sudo rule is installed. Downloads/audio
and user metadata stay under the operator account. The QEMU hook is a separate
root-owned copy installed before guests can start.

A new empty lab can explicitly select /home/forge-storage. Existing populated
storage is preserved; migration is not performed implicitly. Capacity checks and
atomic sparse installs remain in the shared engine.

Report: ~/.local/share/forge/installation-report.json. Guests stay off. Restart
or re-login activates new desktop group membership; no automatic reboot occurs.
Actual post-reboot behavior still requires a separate physical acceptance check.

## Build one bundle

Build native Forge with the RPM builder, then assemble with the previously
validated native Boxes and signed AlmaLinux libvirt-gconfig/libvirt-gobject RPMs:

```bash
python3 packaging/rpm/build.py --target el10 --output dist/el10-rpm
python3 packaging/rpm/build_bundle.py \
  --rpm dist/el10-rpm/forge-4.2.0-10.el10.x86_64.rpm \
  --dependencies /path/to/native-boxes-rpms \
  --output dist/forge-almalinux-4.2.0-10
```

Use a new output directory for each candidate. A native runtime needs no Rust
compiler. SHA256SUMS detects changed bundle files but is not publisher signing;
Forge/Boxes RPMs remain unsigned local packages. Package trust must not be confused
with upstream VM OpenPGP verification.

## Acceptance status

152 workspace tests passed before physical installer acceptance, including helper
output framing, continued operation after a failed job and existing role/network,
image and storage tests. These do not replace four complete native installer runs.

- Music on + Standard: pending.
- Music off + Standard: pending.
- Music on + Dedicated: pending.
- Music off + Dedicated: pending.
- One cold-credential authentication across a long download: pending.
- Boxes start/stop, effective confinement, networks and post-reboot checks: pending
  for the integrated installer, even though earlier individual fixes worked.

USB B was connected on 2026-10-16 (Realtek RTL8153). Physical Dedicated tests can
now be attempted; do not mark them passed merely because the interface is present.
Do not run concurrent host diagnostics during prompt-count acceptance.

Revision 9 Standard/music completed with one sudo, zero Polkit and audible music.
Revision 9 Standard/silence failed on a libvirt read after idle daemon shutdown.
Revision 10 retries only bounded transient read disconnects; mutations never retry.
The storage question now explicitly concerns Forge disk placement, never deletion
of other guests. Dedicated asks for both cables, identifies active A/B, checks A
owns the host default route, applies the existing quarantine/lease/routing logic
through the shared root helper and checks B's DHCP/policy routing. Link/DHCP checks
are not guest internet or physical-path independence tests; those remain pending.
The later private strict-independent-provider variant is deferred in PORT-CONTRACT.md.
