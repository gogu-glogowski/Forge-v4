# Fedora RPM

Target: Fedora 44+ Workstation, x86_64, systemd, NetworkManager, KVM and enforcing
SELinux. This is an unsigned third-party binary RPM, not a Fedora repository submission.

Build on Fedora with Rust 1.89+, a C compiler, Python 3.11+, `rpm-build` and xz:

```bash
python3 packaging/rpm/build.py
sudo dnf install ./dist/rpm/forge-4.2.0-1.x86_64.rpm
```

DNF resolves Fedora runtime dependencies; RPM records ELF library requirements.
This local package was assembled on Pop!_OS using an isolated RPM toolchain;
native Fedora installation and running-VM validation remain separate checks.
For a native Fedora build, run the same builder there. Check the generated
filename (a native build can include a Fedora release suffix).

Enable the existing modular libvirt sockets as described in the main README,
join libvirt/kvm groups and log out/in. With guests stopped, run `/usr/bin/forge setup`,
then `/usr/bin/forge prepare --all` as the normal user. SELinux remains enforcing.
MP3s stream from the Forge repository; credits/license are packaged locally.
`mpv` is recommended, and music failures never stop image preparation.

The RPM has no installation/removal scriptlets. It does not modify networks,
activate a hook, restart libvirt or start/stop guests. Remove/purge leaves the lab,
user data, separately activated root hook, libvirt definitions and networks intact.
Use `sudo dnf remove forge` to remove package-owned files. Activate an upgraded
root hook only after stopping guests. A manual ~/.local/bin/forge can shadow it.

Do not treat this RPM as an openSUSE/RHEL port: their packages, confinement and
libvirt service defaults must be checked independently. Silverblue/Kinoite and
other immutable desktops need additional host-integration validation.

## AlmaLinux / Rocky Linux 10+ candidate

`python3 packaging/rpm/build.py --target el10 --output /tmp/forge-el10`
builds a separate `1.el10` RPM requiring AlmaLinux or Rocky release >=10. It does
not reuse Fedora's release dependency. The same glibc >=2.39 floor applies.
Package availability (notably Boxes, mpv and 7zip) must be checked in the enabled
EL10 repositories; no EPEL/CRB repository is silently added. Native dependency
resolution and physical VM start/stop are pending. Do not install this on EL9.

### Known EL10 blocker (2026-10-15)

The inspected AlmaLinux 10.2 DVD and EPEL 10.2 repositories do not contain
native gnome-boxes. The candidate RPM is therefore not a normally installable
complete port with those repositories alone. RHEL 10 also removed SPICE support;
AlmaLinux 10 explicitly restored it, so Alma and Rocky need separate runtime audits.
Forge currently emits SPICE graphics/channel XML. Do not bypass dependencies or
replace native Boxes with Flatpak and assume equivalent system-libvirt integration.
References: https://almalinux.org/blog/2025-05-27-welcoming-almalinux-10/ and
https://docs.redhat.com/en/documentation/red_hat_enterprise_linux/10/html-single/configuring_and_managing_linux_virtual_machines/index
