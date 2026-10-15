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
echo 'Wybierz miejsce laboratorium (cache pobierania pozostaje w katalogu użytkownika):'
df -h /var/lib /home
echo '1) Zachowaj obecne miejsce /var/lib/forge (także istniejące montowanie).'
echo '2) Nowe laboratorium na /home/forge-storage, montowane jako /var/lib/forge.'
echo '   Istniejące obrazy wymagają osobnej, weryfikowanej migracji; instalator ich nie przenosi.'
while true; do
  read -r -p 'Miejsce [1/2; Enter: zachowaj]: ' storage_choice
  case "$storage_choice" in
    ''|1) storage=keep; break ;;
    2) storage=home; break ;;
    *) echo 'Wybierz 1 albo 2.' ;;
  esac
done
(cd "$here" && sha256sum -c SHA256SUMS)
rpm -K "$here/libvirt-gconfig-5.0.0-4.el10.x86_64.rpm" "$here/libvirt-gobject-5.0.0-4.el10.x86_64.rpm"
# One bounded privileged invocation for package installation only.
sudo bash -s -- "$here" "$storage" <<'FORGE_INSTALL_ROOT'
set -Eeuo pipefail
bundle=$1
storage=$2
[[ "$storage" == keep || "$storage" == home ]] || exit 1
if [[ "$storage" == home ]] && ! mountpoint -q /var/lib/forge; then
  if [[ -d /var/lib/forge ]] && [[ -n "$(find /var/lib/forge -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
    echo 'Istnieją dane Forge: użyj zweryfikowanej migracji zamiast ich przenoszenia w instalatorze.' >&2
    exit 1
  fi
  [[ ! -e /home/forge-storage && ! -L /home/forge-storage ]] || exit 1
  [[ ! -L /home && $(stat -c %u /home) == 0 && $(stat -c %a /home) == 755 ]] || exit 1
  [[ ! -L /var/lib/forge ]] || exit 1
fi
if ! rpm -q epel-release >/dev/null 2>&1; then
  dnf install epel-release
fi
dnf --enablerepo=crb --enablerepo=epel --setopt=install_weak_deps=False install \
  "$bundle/libvirt-gconfig-5.0.0-4.el10.x86_64.rpm" \
  "$bundle/libvirt-gobject-5.0.0-4.el10.x86_64.rpm" \
  "$bundle/gnome-boxes-50.0-1.el10.x86_64.rpm" \
  "$bundle/forge-4.2.0-4.el10.x86_64.rpm" mpv
if [[ "$storage" == home ]] && ! mountpoint -q /var/lib/forge; then
  install -d -o root -g root -m 0755 /home/forge-storage /var/lib/forge
  semanage fcontext -a -e /var/lib/forge /home/forge-storage
  restorecon -RF /home/forge-storage
  mount --bind /home/forge-storage /var/lib/forge
  printf '\n/home/forge-storage /var/lib/forge none bind,x-systemd.requires-mounts-for=/home/forge-storage 0 0\n' >> /etc/fstab
  systemctl daemon-reload
fi
if [[ -d /var/lib/forge ]]; then df -h /var/lib/forge /home; else df -h /var/lib /home; fi
FORGE_INSTALL_ROOT
rpm -q gnome-boxes forge libvirt-gconfig libvirt-gobject
/usr/bin/gnome-boxes --version
echo 'Przekaż wynik poniższej diagnostyki w czacie.'
echo 'Port readiness nadal zgłasza eksperymentalny status do testu maszyny na sprzęcie.'
/usr/bin/forge setup --network "$mode"
echo 'Obrazy pobierz osobno przez forge prepare --all; maszyny nie są uruchamiane automatycznie.' 
/usr/bin/forge dev doctor

