# Void Linux glibc x86_64: experimental XBPS port

Target installer experience: [shared port contract](../../PORT-CONTRACT.md).
The contract includes music-first selection, prerequisite checks/install, explicit
network selection and integrated image preparation with one authentication.
Existing manual instructions below are not proof that this complete flow is implemented.

Build with cargo and xbps-create on PATH:
`python3 packaging/void/build.py --output /tmp/forge-xbps`.
Index the directory with `xbps-rindex -a /tmp/forge-xbps/*.xbps`, then install from
that local repository with `sudo xbps-install -R /tmp/forge-xbps forge`.
`mpv` is installed as a required dependency for online music. This package is unsigned.

Use an updated glibc host (>=2.39 for this binary), not musl. Native dependency
resolution and physical guest operation have not been validated on Void yet.
This port probes runit's /etc/sv and /var/service instead of requiring systemd.
Enable dbus, NetworkManager, libvirtd, virtlogd and virtlockd using the normal Void
service symlinks; inspect existing services before adding links. Enable AppArmor
in the kernel and load the libvirt helper/QEMU profiles. Verify an AppArmor model
in `virsh -c qemu:///system capabilities` before `forge setup --network standard`.
Void's official libvirt recipe includes libapparmor support; this does not prove
that your running kernel/profiles enforce every VM.

Stop guests before installing the Forge hook; installation restarts libvirtd with
`sv restart`. Keep NetworkManager as the active desktop network manager. Dedicated
A/B uses busctl and is not included in this runit port's tested scope. No automatic
service, group, disk, network or AppArmor changes happen during XBPS installation.
