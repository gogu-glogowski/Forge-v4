#!/usr/bin/env bash
# Build and test confined libvirt in a clean Arch chroot, then configure the host.
set -euo pipefail
(( EUID != 0 )) || { echo 'Uruchom jako użytkownik pulpitu, bez sudo.' >&2; exit 1; }
INSTALL_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO=$(cd -- "$INSTALL_DIR/../../.." && pwd)
WORK="$REPO/dist/arch-install"
mkdir -p "$WORK/libvirt"
exec > >(tee -a "$WORK/02-host.log") 2>&1
python3 "$INSTALL_DIR/host.py" preflight
for tool in extra-x86_64-build cargo makepkg; do command -v "$tool" >/dev/null; done
# Pinned upstream recipe plus a minimal, reviewable patch; upstream checks stay enabled.
python3 "$INSTALL_DIR/host.py" recipe "$WORK/libvirt/PKGBUILD"
(cd "$WORK/libvirt" && extra-x86_64-build)
shopt -s nullglob
packages=("$WORK"/libvirt/libvirt-[0-9]*-x86_64.pkg.tar.zst)
(( ${#packages[@]} == 1 )) || { echo 'Nie znaleziono dokładnie jednej paczki libvirt.' >&2; exit 1; }
python3 "$INSTALL_DIR/host.py" inspect "${packages[0]}"
# No automatic trust bypass, no disabled package/source checks.
sudo pacman -U "${packages[0]}"
# Native dependency checks remain enabled for Forge.
python3 "$REPO/packaging/arch/build.py"
forge_package=$(python3 - "$REPO" <<'PY'
import sys
from pathlib import Path
root = Path(sys.argv[1])
sys.path.insert(0, str(root / 'packaging/arch'))
from build import package_filename
print(root / 'dist/arch' / package_filename())
PY
)
sudo pacman -U "$forge_package"
# Run a fixed helper as root; no Forge command is run as root.
sudo python3 "$INSTALL_DIR/host.py" configure "$USER"
printf '\nEtap 2 zakończony. Uruchom ponownie komputer, następnie 03-lab.sh.\n'
printf 'Nie wykonuję automatycznego restartu. Log: %s/02-host.log\n' "$WORK"
