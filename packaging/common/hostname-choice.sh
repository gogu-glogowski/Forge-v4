#!/usr/bin/env bash
# Shared installer UI. Sourcing this file never changes the hostname.
forge_valid_transient_hostname() {
  [[ ${#1} -le 63 && "$1" =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?$ ]]
}

forge_choose_hostname() {
  FORGE_HOSTNAME_ACTION=keep
  FORGE_TRANSIENT_HOSTNAME=
  [[ -t 0 ]] || { echo 'Wybór hostname wymaga terminala; nie zmieniono nazwy.' >&2; return 1; }
  local selection requested current
  current=$(hostnamectl --transient 2>/dev/null || hostname)
  echo "Obecna nazwa tymczasowa hosta: $current"
  echo '1) Zachowaj obecną nazwę hosta (domyślnie).'
  echo '2) Ustaw własną nazwę tymczasową (transient); nazwa statyczna pozostanie bez zmian.'
  while true; do
    read -r -p 'Hostname [1/2, Enter: zachowaj]: ' selection || return 1
    case "$selection" in
      ''|1) return 0 ;;
      2)
        while true; do
          read -r -p 'Nowa nazwa (małe litery, cyfry i myślniki; do 63 znaków; Enter: anuluj): ' requested || return 1
          [[ -n "$requested" ]] || return 0
          if forge_valid_transient_hostname "$requested"; then
            FORGE_HOSTNAME_ACTION=transient
            FORGE_TRANSIENT_HOSTNAME=$requested
            echo "Wybrano tymczasową nazwę: $requested."
            echo 'Nazwa transient może później zostać zastąpiona przez konfigurację sieci/systemu.'
            return 0
          fi
          echo 'Nieprawidłowa nazwa: bez spacji, kropek oraz myślnika na początku/końcu.'
        done ;;
      *) echo 'Wybierz 1 albo 2.' ;;
    esac
  done
}
