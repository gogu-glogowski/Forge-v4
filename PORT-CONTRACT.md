# Native port installation contract

Owner requirements, recorded on 2026-10-15. This is the target contract, not a claim
that the current installers already implement it.

1. One installer per distribution. Its first interactive question selects music
   on/off for the whole installation and image preparation.
2. Inspect the actual host, including whether it is a VM, CPU virtualization and
   KVM/nested access, architecture, RAM/CPU availability, disk filesystems/free
   space, repositories, native Boxes, libvirt, firewall and enforcing confinement.
   Install missing supported prerequisites and configure access/services safely.
   Unsupported conditions must stop with useful diagnostics, not false success.
3. Announce: prerequisites checked and required components installed; proceeding
   to Forge installation. Ask Standard or Dedicated before network changes.
   Standard uses host NAT without extra hardware. Dedicated uses host A and USB
   Ethernet B for guest internet; separate interfaces/cables, not necessarily ISPs.
   Preserve the existing mode as the default for an existing lab.
4. Install Forge, configure selected mode and protected storage, then invoke the
   preparation workflow as the existing operator. Reuse verified bases/cache and
   existing overlays. For this operator's current lab prepare Tsurugi, Kali and
   Whonix; skip SIFT unless a licensed local SANS OVA was explicitly supplied.
5. Verify results and leave guests off. Explain any necessary login/reboot step;
   after that Boxes → QEMU System should expose the prepared guests. Never claim
   completion on a failed setup/download/verification. Never reboot automatically.

At most one privilege authentication covers the entire invocation, including
packages and preparation. Keep a bounded privileged helper without running the
entire CLI as root, disabling Polkit or adding global passwordless sudo. Protected
firewall inspection belongs in the helper too. Noninteractive installation needs
explicit choices or documented preservation of the existing configuration.

## All installer combinations are mandatory

| Music | Network | Required result |
| --- | --- | --- |
| On | Standard | Audible preparation music, native dependencies, verified guests, host NAT, active Forge hook |
| Off | Standard | No playback process/audio, otherwise the same complete Standard installation |
| On | Dedicated A/B | Audible music, complete dependencies/guests, A/B network configuration and required USB/network hook integration |
| Off | Dedicated A/B | No playback, otherwise the same complete Dedicated installation |

Music choice must never change dependency completeness, confinement, guest roles,
network configuration or hook coverage. Every combination requires its own native
acceptance test on each supported distribution, including one initial sudo
authentication and no additional sudo/Polkit prompts during the whole operation.

Resolve prerequisites from the actual distro/package metadata and project code,
not assumptions from earlier conversations. Native binary installs need runtime
dependencies; Rust, Python and other build tools are installed when the selected
distribution workflow actually builds or uses them, not indiscriminately.

The installer prepares four VMs for the current no-SIFT selection: Tsurugi, Kali,
Whonix Gateway and Whonix Workstation. Whonix is one profile with two guests.
SIFT remains an explicitly supplied licensed local OVA, never an invented download.
Download/verify/configure guests in the selected network mode within the installer.

Install and activate required root hooks automatically before declaring success;
the user must not need a separate `forge dev hook`. The QEMU security/role/digest
hook is required in BOTH modes and must cover starts from Boxes, virsh and Forge.
Dedicated additionally needs its A/B ownership and network integration. Preserve
safe ordering: all guests off for hook updates, hooks in place before any guest
can start, no nested virsh calls inside the synchronous QEMU hook. Never defer
security hooks until after permitting an unguarded start.

## Concurrent guests

Standard must permit concurrent Kali, Tsurugi and Whonix where resources allow;
no arbitrary one-guest restriction. Do not allocate all host resources to every
guest. Preserve role isolation, signatures/hashes, immutable bases, confinement
and Workstation → Gateway-only topology. Workstation requires a running Gateway.
Dedicated may retain exclusive Kali/Gateway access to their shared USB uplink B.
Audit CLI, root hooks, Boxes starts, network policy, docs and tests together.

## Current gaps and acceptance

- AlmaLinux revision 9 implements the integrated candidate flow in Rust with a
  thin bundle entrypoint. Full native four-combination authentication/network/audio
  acceptance is pending; other distribution adapters remain to be implemented.
- Audit/update each distro-specific guide and entrypoint separately; keep the
  central README largely intact. Never imply untested distributions are ready.
- Test new installation and upgrade, preserving data/mode/identity; music on/off;
  one cold-credential authentication despite long downloads; cancellation/failure;
  storage exhaustion; real Boxes start/stop; effective confinement; Standard
  concurrency; Dedicated ownership/topology on appropriate hardware.
- AlmaLinux bug: the hook recognized only qemu-system process names, while EL10
  uses /usr/libexec/qemu-kvm. Gateway was running but Workstation was denied as
  if it were off. Fix process detection without removing the Gateway requirement.

## Deferred private variant — after public ports

Owner request recorded 2026-10-16: later create a separate private variant with
strict independent-uplink requirements for A/B and fail-closed guest internet
when independence cannot be established. Private repository/location is not yet
chosen; do not create one or enable this policy now. Public Dedicated continues
to allow separate interfaces/cables on the same provider/router.
Two cables, different addresses/subnets or different public IPs do not by
themselves prove different providers or independent physical paths. Define
credible evidence and tests before claiming this stronger property. Keep this
future policy separate from current public port acceptance and default behavior.
