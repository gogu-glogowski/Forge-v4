# Arch-family rolling package

Active target: current x86_64 Arch Linux with systemd, NetworkManager, KVM and
confined libvirt. CachyOS is the Arch-family native validation host. Other
distribution-specific ports are outside this delivery scope. Update the host
with `sudo pacman -Syu`; partial upgrades are not supported.

## Confinement

The official Arch libvirt 12.8.0 recipe inspected on 2026-10-15 disables
AppArmor, its profiles and driver, and SELinux. Installing only AppArmor does
not add the missing libvirt driver. Use a compatible confined libvirt build,
installed helper/profile files, an enabled kernel and loaded profiles.
`virsh -c qemu:///system capabilities` must advertise the actual security model.
Forge refuses unconfined setup/start; physical VM checks remain necessary.

`enable-libvirt-apparmor.py` patches a local upstream PKGBUILD to enable the
three AppArmor options and dependency. Build natively with upstream checks.
It does not install or certify the result. Inspect CachyOS capabilities as well.

## Build

Install `base-devel rust cargo python`, then run as a normal user:

```bash
python3 packaging/arch/build.py
```

Install the exact local package path printed by the builder using `sudo pacman -U`.
The generated source archive omits music; the package contains music attribution
and streams audio from the repository. Install optional `mpv` for playback.
AUR publication is outside scope.

With all guests stopped, enable the appropriate libvirt service family,
AppArmor and NetworkManager; join libvirt/kvm and log in again. Run `forge setup`
and `forge prepare --all`. See the separate Arch installer branch/PR for the
resumable guided installer under native validation.

Package installation/removal does not configure services, groups, hooks,
networks or VMs. Removing Forge preserves lab data and the activated hook.
Check `type -a forge` for older manual copies.

`--cross-host` skips native dependency checks only for packaging tests and is
not evidence of live host readiness. The binary requires glibc >= 2.39.
