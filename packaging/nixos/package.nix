{ lib, rustPlatform, makeWrapper, coreutils, procps, psmisc, util-linux,
  qemu, libvirt, gnome-boxes, networkmanager, systemd, gnupg, curl, cacert,
  p7zip, gnutar, xz, e2fsprogs, iproute2, glibc, apparmor-utils, mpv, gnugrep, gnused, gawk, findutils }:
rustPlatform.buildRustPackage {
  pname = "forge";
  version = "4.2.0";
  src = lib.cleanSourceWith {
    src = ../..;
    filter = path: type:
      let name = baseNameOf path; in
      !(builtins.elem name [ ".git" "target" "dist" "packages" ])
      && !(lib.hasSuffix ".mp3" name);
  };
  cargoLock.lockFile = ../../Cargo.lock;
  cargoBuildFlags = [ "-p" "forge-cli" ];
  nativeBuildInputs = [ makeWrapper ];
  # Host-dependent tests are run separately; builders have no system libvirt.
  doCheck = false;
  postInstall = ''
    mkdir -p $out/share/forge/music $out/share/doc/forge
    cp assets/music/{CREDITS.md,LICENSE-CC-BY-4.0.txt,manifest.json} $out/share/forge/music/
    cp README.md CHANGELOG.md $out/share/doc/forge/
  '';
  postFixup = ''
    wrapProgram $out/bin/forge --prefix PATH : ${lib.makeBinPath [
      coreutils procps psmisc util-linux qemu libvirt gnome-boxes networkmanager
      systemd gnupg curl p7zip gnutar xz e2fsprogs iproute2 glibc apparmor-utils mpv gnugrep gnused gawk findutils
    ]} --set SSL_CERT_FILE ${cacert}/etc/ssl/certs/ca-bundle.crt
  '';
  meta = {
    description = "Verified KVM lab with GNOME Boxes (experimental NixOS port)";
    homepage = "https://github.com/gogu-glogowski/Forge-v4";
    license = with lib.licenses; [ asl20 cc-by-40 ];
    platforms = [ "x86_64-linux" ];
    mainProgram = "forge";
  };
}
