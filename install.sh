#!/usr/bin/env bash
# Full installer by default; --stage plan is the explicit offline preview.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
STAGE=install
if [[ ${1:-} == --stage ]]; then
  [[ $# -ge 2 ]] || { echo 'Brak etapu: plan, install, prerequisites, host, lab.' >&2; exit 2; }
  STAGE=$2
  shift 2
fi
case "$STAGE" in
  install) exec python3 "$ROOT/packaging/arch/install/workflow.py" "$@" ;;
  plan) exec bash "$ROOT/packaging/arch/install/03-lab.sh" --plan "$@" ;;
  lab) exec bash "$ROOT/packaging/arch/install/03-lab.sh" "$@" ;;
  prerequisites) exec bash "$ROOT/packaging/arch/install/01-prerequisites.sh" "$@" ;;
  host)
    [[ $# == 0 ]] || { echo 'Opcje sieci i obrazów należą do etapu plan/lab.' >&2; exit 2; }
    exec bash "$ROOT/packaging/arch/install/02-host.sh" ;;
  *) echo 'Nieznany etap: użyj plan, install, prerequisites, host lub lab.' >&2; exit 2 ;;
esac
