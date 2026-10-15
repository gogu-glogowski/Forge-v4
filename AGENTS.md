# Mandatory rules for every Forge port

## One authentication per operation

This is a hard product requirement from the project owner, applicable to every
distribution and packaging format.

- A complete installation or preparation invocation must require at most one
  privilege authentication in total. Count sudo and Polkit together, not separately.
- `forge prepare --all` must not ask again for each image, VM, download, storage
  step or libvirt call. The number of images must not determine the number of prompts.
- Group privileged work into a bounded authenticated operation. Long downloads
  must not lead to a new authentication prompt for each subsequent image.
- Cancellation or authentication failure must stop the privileged operation
  cleanly rather than repeatedly presenting prompts.
- Preserve privilege boundaries, image verification, ownership checks and host
  security policy. Do not achieve this by disabling Polkit, adding unrestricted
  passwordless sudo, running all of Forge as root or weakening SELinux/AppArmor.

## Acceptance and reporting

Verify each port with cold credentials, the full multi-image preparation path,
and a download long enough to exceed an ordinary credential timeout. Count all
authentication dialogs/prompts and check cancellation and failure handling.
Cover libvirt authorization as well as storage and hook operations.

A port that prompts per image violates this requirement, even if it eventually
prepares the images correctly. Do not mark it fully ready until this is fixed and
verified. Commit and PR descriptions for port work must explicitly report the
authentication behavior and whether this acceptance check passed, failed or
remains unverified. Documenting this rule alone does not implement the fix.

## Full feature parity and explicit network choice

Every port must implement and configure the complete current project contract,
not merely install a binary. The main README and the network/role/security code
are the reference; review them before porting and report any gap explicitly.

- Keep native GNOME Boxes connected to qemu:///system, libvirt/KVM, the appropriate
  host confinement, groups/access, storage ownership/labels, hook and network policy.
- Install music playback dependencies by default and verify actual audible playback
  during preparation. Music is part of the complete experience, not a dependency
  that may disappear with disabled weak/recommended packages. Explicit --no-music
  remains supported; playback failure must not break image verification/preparation.
- The user-facing installer must ask which network mode to configure before making
  network changes. Do not assume Standard silently. Default to Standard for a NEW
  lab; show and preserve the existing mode as the default on an existing lab.
- Standard uses the host connection through libvirt NAT for Kali and Whonix Gateway
  and requires no extra hardware.
- Dedicated is advanced: host uses interface A; guest internet uses USB Ethernet B.
  Separate interfaces/cables are required, not necessarily different providers.
- Apply the selected mode through Forge setup, with guests stopped. Preserve the
  project's isolated roles and Whonix Workstation-to-Gateway-only topology.
- Implement the selection in the interactive installer/configuration entrypoint,
  not blocking package-manager scriptlets without a terminal. Noninteractive
  installers must receive an explicit mode (or preserve an existing configured mode).
- A plain package-manager install is not evidence of full configuration. Document
  its remaining configuration entrypoint rather than claiming the port is complete.

Validate EACH distribution separately: authentication count, installer selection
and preservation of mode, both network modes, music/audio, Boxes, image verification,
storage, VM start/stop and effective confinement. A test/build on another distro
is not a substitute. Do not weaken or omit project requirements to make a port pass.

## Preserve the operator's identity and host configuration

Hard requirement for the entire official project and every port:

- Use the existing authenticated operator account. Do not hardcode the developer's
  username or hostname as another user's identity, paths or ownership.
- Never rename/create/replace the operator account as a side effect of installation,
  setup, downloads, preparation, updates or VM lifecycle operations.
- Preserve static, pretty and transient hostnames. Do not call set-hostname,
  sethostname, edit /etc/hostname or change NetworkManager/DHCP hostname policy.
- Do not add hostname-changing installer choices. The owner clarified that
  personal transient hostname changes belong outside the official Forge program.
- Names of guest VMs must not alter the host computer's or operator's identity.
- Test preservation of account identity and all hostname forms on every port.
  Fail and investigate any unrequested identity change; do not mark the port ready.


## Storage capacity, recovery and atomic image installation

- Show the actual filesystems, free space and selected destination for cache,
  immutable bases and overlays. Ask explicitly before selecting a new location.
  A large physical drive does not mean /var and /home share free space.
- Never fill the host root filesystem. Check destination capacity before copying,
  keep a host reserve, and budget both Whonix bases together. Explain conservative
  logical-size budgeting of sparse images rather than claiming unused virtual disk
  capacity is already physically allocated.
- Preserve sparse image layout. Copy into a private temporary file on the same
  destination filesystem, flush and apply ownership/labels, then atomically rename.
  On failure retain the previously verified canonical base and remove partial files.
- Storage outside /var/lib/forge must remain root-owned and protected from user
  path substitution. Preserve SELinux/AppArmor policy and libvirt backing paths;
  never use the unprivileged FORGE_DATA_DIR override as a production migration.
- Migrate only with guests stopped; verify the copy before switching/removing the
  original. Back up mount configuration, make the mount persistent and preserve
  image signatures/proofs, permissions, labels, immutable flags and VM definitions.
- Reuse verified bases and downloaded signed archives after recovery. Do not
  restart every download, delete complete guests or hide storage failures.
