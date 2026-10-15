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
(cd "$here" && sha256sum -c SHA256SUMS)
rpm -K "$here/libvirt-gconfig-5.0.0-4.el10.x86_64.rpm" "$here/libvirt-gobject-5.0.0-4.el10.x86_64.rpm"
if ! rpm -q epel-release >/dev/null 2>&1; then
  sudo dnf install epel-release
fi
sudo dnf --enablerepo=crb --enablerepo=epel --setopt=install_weak_deps=False install \
  "$here/libvirt-gconfig-5.0.0-4.el10.x86_64.rpm" \
  "$here/libvirt-gobject-5.0.0-4.el10.x86_64.rpm" \
  "$here/gnome-boxes-50.0-1.el10.x86_64.rpm" \
  "$here/forge-4.2.0-2.el10.x86_64.rpm" mpv
rpm -q gnome-boxes forge libvirt-gconfig libvirt-gobject
/usr/bin/gnome-boxes --version
echo 'Przekaż wynik poniższej diagnostyki w czacie.'
echo 'Port readiness nadal zgłasza eksperymentalny status do testu maszyny na sprzęcie.'
/usr/bin/forge setup --network "$mode"
echo 'Obrazy pobierz osobno przez forge prepare --all; maszyny nie są uruchamiane automatycznie.' 
/usr/bin/forge dev doctor
