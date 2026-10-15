{ lib, config, ... }:
let desktop = config.forgeLab.desktop; in {
  options.forgeLab.desktop = lib.mkOption {
    type = lib.types.enum [ "gnome" "cosmic" "xfce" ];
    default = "gnome";
  };
  config = {
    time.timeZone = "Europe/Warsaw";
    i18n.defaultLocale = "pl_PL.UTF-8";
    console.keyMap = "pl";
    services.xserver.xkb.layout = "pl";
    users.users.major = {
      isNormalUser = true;
      description = "Major";
      extraGroups = [ "wheel" "networkmanager" ];
      # Set password interactively using passwd; no password/autologin is embedded.
    };
    services.xserver.enable = desktop == "xfce";
    services.desktopManager.gnome.enable = desktop == "gnome";
    services.displayManager.gdm.enable = desktop == "gnome";
    services.desktopManager.cosmic.enable = desktop == "cosmic";
    services.displayManager.cosmic-greeter.enable = desktop == "cosmic";
    services.xserver.desktopManager.xfce.enable = desktop == "xfce";
    services.xserver.displayManager.lightdm.enable = desktop == "xfce";
  };
}
