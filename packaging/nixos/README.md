# NixOS: experimental native package and module

Use current stable NixOS x86_64. This is a source derivation, not a DEB/RPM or an
ISO. Native evaluation/build and physical guest start/stop remain required.
No claim of production readiness is made.

From a checkout: `nix-build -E 'with import <nixpkgs> {}; callPackage ./packaging/nixos/package.nix {}'`.
Import `./packaging/nixos/module.nix` from your configuration and set:

```nix
programs.forge.enable = true;
programs.forge.operator = "major";
```

Run `sudo nixos-rebuild test` first and inspect AppArmor/libvirt logs. The module
requires confinement, enables NetworkManager and libvirtd, and adds operator groups.
It changes the machine's declared service configuration; inspect it before applying.
After activation and a fresh login, stop all guests and run `forge setup --network standard`.
The Forge hook is a regular root-owned copy in `/var/lib/libvirt/hooks/qemu`, not
an environment-selected executable. Profiles use `/var/lib/apparmor.d`, matching
nixpkgs libvirt's sysconfdir. Per-guest confinement is still checked by Forge.
Other existing libvirt hooks must be reviewed before enabling this lab.
Dedicated A/B and native NixOS AppArmor profile generation are unvalidated.
