# Active ports and integration policy

| Target | Current evidence | Development |
| --- | --- | --- |
| Pop!_OS 24.04 | Prior Standard-mode native Boxes, AppArmor, Kali/Tsurugi lifecycle and Whonix topology smoke checks | Existing DEB baseline; packaging/pop |
| Arch, including CachyOS validation host | Native candidate packaging and image verification; full installer/guest/A-B validation pending | arch-installer-4.2.2, draft PR #2 |
| Fedora 44+ | RPM packaging candidate; native confined guest/lifecycle/network validation pending | fedora-port |

Keep main unchanged while the ports are validated. Do not merge the Arch/Fedora
integration until both ports complete native end-to-end checks. Keep PR #2 a
draft in the meantime. Existing Pop evidence does not certify every firewall or
physical A/B configuration.

Only these three targets are in scope. Other distributions must not inherit
support from ID_LIKE. CachyOS is explicitly recognized as an Arch-family target.
Removed experiments remain recoverable from Git history; do not erase history.

The cleanup is proposed independently of the Arch installer so that removing
unsupported targets does not implicitly approve or merge the new installer.
