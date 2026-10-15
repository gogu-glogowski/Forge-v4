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

## Concurrent guests

Standard must permit concurrent Kali, Tsurugi and Whonix where resources allow;
no arbitrary one-guest restriction. Do not allocate all host resources to every
guest. Preserve role isolation, signatures/hashes, immutable bases, confinement
and Workstation → Gateway-only topology. Workstation requires a running Gateway.
Dedicated may retain exclusive Kali/Gateway access to their shared USB uplink B.
Audit CLI, root hooks, Boxes starts, network policy, docs and tests together.

## Current gaps and acceptance

- Current AlmaLinux installer is an incomplete candidate: music-first flow,
  prerequisite orchestration, integrated preparation and one authentication across
  package install + preparation still require implementation and native validation.
- Audit/update each distro-specific guide and entrypoint separately; keep the
  central README largely intact. Never imply untested distributions are ready.
- Test new installation and upgrade, preserving data/mode/identity; music on/off;
  one cold-credential authentication despite long downloads; cancellation/failure;
  storage exhaustion; real Boxes start/stop; effective confinement; Standard
  concurrency; Dedicated ownership/topology on appropriate hardware.
- AlmaLinux bug: the hook recognized only qemu-system process names, while EL10
  uses /usr/libexec/qemu-kvm. Gateway was running but Workstation was denied as
  if it were off. Fix process detection without removing the Gateway requirement.
