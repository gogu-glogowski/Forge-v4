# Forge 4.2.0 binary DEB

Build on amd64 Debian-family systems with Rust 1.89+, a C compiler, Python 3.11+,
`dpkg-dev`, `fakeroot` and `xz-utils`:

```bash
python3 packaging/debian/build.py
```

Output: **one** `dist/forge_4.2.0-1_amd64.deb`. No MP3 payload or music DEB is
needed. The package includes the small attribution/license/manifest files.
`dpkg-shlibdeps` derives actual minimum libc/libgcc versions. This build requires
libc6 >= 2.39; rebuild on an older host for older Debian/Ubuntu releases.

## Install and use

```bash
sudo apt install ./dist/forge_4.2.0-1_amd64.deb
sudo systemctl enable --now libvirtd.service
sudo usermod -aG libvirt,kvm "$USER"
```

For modular libvirt, use its existing socket family instead of a second daemon
family. Log out/in after group changes. Check access with
`virsh -c qemu:///system list --all`. Run Forge as your normal user.
A previous `~/.local/bin/forge` may shadow the DEB's `/usr/bin/forge`; check
`type -a forge` and remove/rename the old manual copy when migrating.

With all guests stopped, run `/usr/bin/forge setup` to activate/update the root
hook, then `/usr/bin/forge prepare --all`. Music starts automatically through mpv,
streaming MP3s from this public repository over HTTPS at 100% volume, shuffled and
looped. No separate music installation is required. Only a temporary buffered
stream is used; Forge does not download the whole collection before preparing VMs.
Local user/system/checkout audio libraries take priority when available.
Use `--no-music` or `--music-dir /path/to/library` to control playback.
Missing mpv, offline playback and network/audio failures do not stop preparation.

APT installation itself does not play audio, run setup, restart libvirt, change
groups, download images or start/stop guests. No package maintainer scripts exist.
Package upgrades can be installed while VMs run; activate a new root hook only
with guests stopped. Reinstall/upgrade uses the same `apt install ./...deb` command.

## Remove and test

```bash
sudo apt remove forge
```

Remove/purge deletes only package-owned program/documentation/attribution files.
It preserves `/var/lib/forge`, user data/music, VM definitions, networks, the
separately activated root hook and AppArmor helper overrides. Retained VMs keep
their protection. Removing the program is not lab destruction.

Lifecycle tests use real dpkg under fakeroot with an isolated root/database,
copying host status/file lists for dependency checks, never maintainer scripts.
They cover RC-fixture upgrade, reinstall, remove and purge while retaining lab
fixtures. The RC fixture uses this release's payload, not historical VM data.

```bash
fakeroot python3 packaging/debian/test_lifecycle.py --root /tmp/forge-deb-test-new \
  dist/forge_4.2.0-1_amd64.deb
```

This does not test apt's network resolver or real service activation.
Pop!_OS 24.04 Standard mode has live smoke-test evidence; Debian/Ubuntu and custom
firewalls remain pending. A/B hardware tests are deferred. AppImage is not built.
