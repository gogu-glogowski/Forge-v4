#!/usr/bin/env bash
set -Eeuo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
. /etc/os-release
if [[ "$ID" != almalinux || "${VERSION_ID%%.*}" != 10 || "$(uname -m)" != x86_64 ]]; then
  echo 'Ten zestaw jest przygotowany dla AlmaLinux 10 x86_64.' >&2
  exit 1
fi
if (( EUID == 0 )); then
  echo 'Uruchom jako zwykły użytkownik, bez sudo przed bash.' >&2
  exit 1
fi
default_mode=standard
if [[ -f /var/lib/forge/network-mode ]]; then
  default_mode=$(cat /var/lib/forge/network-mode)
  [[ "$default_mode" == standard || "$default_mode" == dedicated ]] || { echo 'Nieprawidłowy zapisany tryb sieci.' >&2; exit 1; }
fi
[[ -t 0 ]] || { echo 'Uruchom instalator w terminalu, aby wybrać tryb sieci.' >&2; exit 1; }
echo 'Wybierz tryb sieci Forge:'
echo '1) Standard — połączenie hosta przez NAT; bez dodatkowego sprzętu.'
echo '2) Dedicated — host: A, internet gości: adapter USB Ethernet B; osobne interfejsy/kable.'
echo '   Dedicated nie wymaga osobnych dostawców internetu.'
while true; do
  read -r -p "Tryb [1/2, Enter: $default_mode]: " selection
  case "$selection" in
    1|standard) mode=standard; break ;;
    2|dedicated) mode=dedicated; break ;;
    '') mode=$default_mode; break ;;
    *) echo 'Wybierz 1 albo 2.' ;;
  esac
done
echo "Wybrano: $mode. Forge setup wymaga zatrzymania wszystkich maszyn."
# Shared helper is shipped beside this script in release bundles.
hostname_helper="$here/hostname-choice.sh"
[[ -f "$hostname_helper" ]] || hostname_helper="$here/../common/hostname-choice.sh"
[[ -f "$hostname_helper" ]] || { echo 'Brak modułu wyboru hostname; instalacja przerwana.' >&2; exit 1; }
source "$hostname_helper"
forge_choose_hostname
(cd "$here" && sha256sum -c SHA256SUMS)
rpm -K "$here/libvirt-gconfig-5.0.0-4.el10.x86_64.rpm" "$here/libvirt-gobject-5.0.0-4.el10.x86_64.rpm"
# One bounded privileged invocation for packages and the optional hostname.
sudo bash -s -- "$here" "$FORGE_HOSTNAME_ACTION" "$FORGE_TRANSIENT_HOSTNAME" <<'FORGE_INSTALL_ROOT'
set -Eeuo pipefail
bundle=$1
hostname_action=$2
hostname_name=$3
case "$hostname_action" in
  keep) [[ -z "$hostname_name" ]] || exit 1 ;;
  transient)
    [[ ${#hostname_name} -le 63 && "$hostname_name" =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?$ ]] || exit 1 ;;
  *) exit 1 ;;
esac
if ! rpm -q epel-release >/dev/null 2>&1; then
  dnf install epel-release
fi
dnf --enablerepo=crb --enablerepo=epel --setopt=install_weak_deps=False install \
  "$bundle/libvirt-gconfig-5.0.0-4.el10.x86_64.rpm" \
  "$bundle/libvirt-gobject-5.0.0-4.el10.x86_64.rpm" \
  "$bundle/gnome-boxes-50.0-1.el10.x86_64.rpm" \
  "$bundle/forge-4.2.0-2.el10.x86_64.rpm" mpv
if [[ "$hostname_action" == transient ]]; then
  hostnamectl --transient set-hostname "$hostname_name"
fi
FORGE_INSTALL_ROOT
rpm -q gnome-boxes forge libvirt-gconfig libvirt-gobject
/usr/bin/gnome-boxes --version
echo 'Przekaż wynik poniższej diagnostyki w czacie.'
echo 'Port readiness nadal zgłasza eksperymentalny status do testu maszyny na sprzęcie.'
/usr/bin/forge setup --network "$mode"
echo 'Obrazy pobierz osobno przez forge prepare --all; maszyny nie są uruchamiane automatycznie.' 
/usr/bin/forge dev doctor
