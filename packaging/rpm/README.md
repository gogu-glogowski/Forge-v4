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
