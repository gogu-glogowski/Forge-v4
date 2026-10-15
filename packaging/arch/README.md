# Arch Linux / Manjaro rolling packages

Target: current, fully updated x86_64 Arch Linux or Manjaro with systemd,
NetworkManager, KVM and enforcing AppArmor/SELinux through libvirt. `ID=arch` and
`ID=manjaro` are recognized even when VERSION_ID is absent. Partial upgrades and
arbitrary old rolling snapshots are not supported. Update with `sudo pacman -Syu`.

## Important confinement requirement

The official Arch libvirt 12.8.0 recipe checked on 2026-10-15 explicitly disables
AppArmor, AppArmor profiles, the AppArmor security driver and SELinux. Installing
only `apparmor` does not add a missing libvirt driver. Forge installs as a package
but deliberately refuses setup/start when libvirt advertises neither usable
AppArmor nor enforcing SELinux. The package does not silently weaken protection.
Manjaro's actual libvirt capabilities must also be checked; do not infer them
solely from ID_LIKE=arch.

For a working host, use a compatible libvirt build with AppArmor enabled, its
virt-aa-helper/profile installed, an AppArmor-enabled kernel, loaded profiles
and the AppArmor libvirt driver. `virsh -c qemu:///system capabilities` must list
`apparmor`. The provided `enable-libvirt-apparmor.py` modifies a local copy of the
official PKGBUILD to enable the three AppArmor options and add the dependency;
it does not install anything or claim the resulting libvirt build was tested.
Build that recipe on a native clean Arch build environment, preserving its upstream
checks. Manjaro must rebuild against its own branch, not install arbitrary Arch
libvirt binaries. Maintained secure libvirt packages and VM smoke tests remain
required before calling this port fully supported.

## Build and install Forge

On the native target install `base-devel rust cargo python`, then from Forge:

```bash
python3 packaging/arch/build.py
sudo pacman -U ./dist/arch/forge-4.2.0-1-x86_64.pkg.tar.zst
```

The builder generates a real checksum-pinned PKGBUILD and a source archive
without MP3s, then invokes makepkg. You may instead run `makepkg -si` beside the
generated PKGBUILD/source archive. AUR is a recipe repository, not a different
binary format; no AUR upload is performed here.

Arch does not install optdepends automatically; install `mpv` for music.
Enable NetworkManager/libvirt and AppArmor according to the host configuration,
join `libvirt,kvm` and log out/in. With guests stopped run `/usr/bin/forge setup`
and `/usr/bin/forge prepare --all`. New labs use Standard NAT. A/B is deferred.
The package includes credits/manifest but streams audio from the Forge repository.

No .install script is included: installation/removal does not alter groups,
services, hooks, networks or VM data. `sudo pacman -R forge` preserves lab data
and the independently activated hook. Check `type -a forge` for old manual copies.

`--cross-host` exists only for isolated packaging tests: it skips makepkg's
Arch-specific dependency checks. Such a build is not proof that an Arch/Manjaro
host can run confined guests. For this local build, glibc >= 2.39 is required.
