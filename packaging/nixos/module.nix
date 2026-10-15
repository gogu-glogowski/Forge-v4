{ config, lib, pkgs, ... }:
let
  cfg = config.programs.forge;
  runtime = with pkgs; [ coreutils procps psmisc util-linux iproute2 networkmanager
    systemd libvirt qemu e2fsprogs apparmor-utils glibc gnugrep gnused gawk findutils ];
in {
  options.programs.forge = {
    enable = lib.mkEnableOption "Forge experimental KVM lab";
    package = lib.mkOption { type = lib.types.package; default = pkgs.callPackage ./package.nix {}; };
    operator = lib.mkOption { type = lib.types.str; default = "major"; };
  };
  config = lib.mkIf cfg.enable {
    assertions = [{ assertion = pkgs.stdenv.hostPlatform.system == "x86_64-linux";
      message = "Forge currently supports x86_64 hosts only."; }];
    environment.systemPackages = [ cfg.package pkgs.gnome-boxes pkgs.mpv ] ++ runtime;
    networking.networkmanager.enable = true;
    virtualisation.libvirtd.enable = true;
    virtualisation.libvirtd.qemu.runAsRoot = false;
    virtualisation.libvirtd.qemu.verbatimConfig = ''
      user = "qemu-libvirtd"
      group = "qemu-libvirtd"
      security_driver = "apparmor"
      security_default_confined = 1
      security_require_confined = 1
    '';
    security.apparmor.enable = true;
    users.groups.libvirt = {};
    users.users.${cfg.operator}.extraGroups = [ "libvirtd" "libvirt" "kvm" ];
    systemd.services.libvirtd.path = runtime;
    systemd.services.libvirtd.requires = [ "forge-apparmor.service" ];
    systemd.services.libvirtd.after = [ "forge-apparmor.service" ];
    # Nixpkgs libvirt uses --sysconfdir=/var/lib. Forge preserves mutable,
    # root-owned hook/profile files here instead of replacing /etc symlinks.
    systemd.services.forge-apparmor = {
      description = "Prepare libvirt AppArmor profiles for Forge";
      requires = [ "apparmor.service" ];
      after = [ "apparmor.service" ];
      before = [ "libvirtd.service" ];
      path = [ pkgs.coreutils pkgs.apparmor-utils pkgs.gnugrep ];
      serviceConfig.Type = "oneshot";
      script = ''
        install -d -m 0755 /var/lib/apparmor.d/local /var/lib/apparmor.d/libvirt
        cp -r ${config.virtualisation.libvirtd.package}/var/lib/apparmor.d/. /var/lib/apparmor.d/
        chmod -R u+w /var/lib/apparmor.d
        profile=/var/lib/apparmor.d/usr.lib.libvirt.virt-aa-helper
        test -f "$profile"
        # Existing local helper rules survive rebuilds; guests get no global lab access.
        touch /var/lib/apparmor.d/local/usr.lib.libvirt.virt-aa-helper
        if ! grep -q 'local/usr.lib.libvirt.virt-aa-helper' "$profile"; then
          echo 'Expected upstream local profile include is missing' >&2; exit 1
        fi
        cat > /var/lib/apparmor.d/local/forge-nix-helper <<'RULES'
        /nix/store/** mr,
        ${pkgs.apparmor-utils}/bin/apparmor_parser Ux,
        RULES
        local=/var/lib/apparmor.d/local/usr.lib.libvirt.virt-aa-helper
        if ! grep -Fx '#include if exists <local/forge-nix-helper>' "$local"; then
          echo '#include if exists <local/forge-nix-helper>' >> "$local"
        fi
        apparmor_parser -I /var/lib/apparmor.d -r "$profile"
      '';
    };
  };
}
