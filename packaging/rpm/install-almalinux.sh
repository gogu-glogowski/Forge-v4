#!/usr/bin/env bash
set -Eeuo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
if (( EUID == 0 )); then
  echo 'Uruchom: bash install-almalinux.sh — bez sudo przed instalatorem.' >&2
  exit 1
fi
[[ -t 0 ]] || { echo 'Instalator wymaga terminala.' >&2; exit 1; }
(cd "$here" && sha256sum -c SHA256SUMS)
exec "$here/forge-installer" install --bundle "$here"
