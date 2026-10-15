# Forge: instalacja i fizyczne testy systemów

ISO są oficjalnymi, niezmodyfikowanymi obrazami. GNOME jest domyślne; COSMIC/XFCE
są alternatywami zgodnie z dostępnością w repozytoriach. Konto: `major`, nazwa
wyświetlana `Major`. Język/klawiatura: polskie, strefa: Europe/Warsaw.

## Co jest automatyczne

1. Debian, Ubuntu oraz Alma/Rocky: dostarczone szablony plików odpowiedzi ustawiają
   język, konto i podstawowe pakiety. **Dysk zawsze wybierasz i zatwierdzasz ręcznie**.
2. Pozostałe instalatory: wykonaj ich zwykłą instalację, utwórz konto major.
3. Po instalacji wspólna konfiguracja przez Ansible instaluje zależności i wybrany
   pulpit. Nie formatuje dysków, nie zmienia hasła, nie włącza SSH ani trybu A/B.

To nie jest przetestowany instalator bezobsługowy dla dziewięciu systemów.
Szablony wymagają pierwszego fizycznego sprawdzenia. Calamares (Manjaro), Zorin,
Void i Arch mają różne mechanizmy instalacji; wspólny jest etap po instalacji.

## Start

Wybierz ISO w nowym katalogu `ISO/Forge-Lab` w menu lokalnego Ventoya. Jego istniejący
ventoy.json pozostaje bez zmian. Przełącznik widoku drzewa Ventoya pomaga przeglądać
podkatalogi; nie zmieniamy Twojego motywu ani istniejących aliasów.
iVentoy używa istniejącej konfiguracji: odśwież jego listę obrazów po dodaniu plików;
automatyczne wykrycie tego podkatalogu w iVentoy nie zostało sprawdzone.

Po instalacji doinstaluj `ansible-core` (apt/dnf/pacman; Void: pakiet `ansible`),
przejdź do tego katalogu i uruchom:

```
python3 configure.py --desktop gnome --check
python3 configure.py --desktop gnome
# Alternatywnie --desktop xfce lub --desktop cosmic
```

Tryb `--check` nie wykonuje instalacji. Właściwy przebieg prosi o potwierdzenie
KONFIGURUJ i hasło sudo. Uruchamiaj na zainstalowanym systemie, nie w sesji Live.
Dla COSMIC przygotowano mapowanie Archa/Manjaro; pozostałe rodziny odmawiają
instalacji tej opcji zamiast dodawać obce repozytoria. Pop!_OS ma własne COSMIC:
na nim nie zamieniaj pulpitu tym playbookiem. NixOS używa modułu deklaratywnego
z `packaging/nixos/`, a nie tego playbooka.

EL10 może wymagać repozytoriów dodatkowych dla Boxes, mpv lub XFCE. Nie są dodawane
automatycznie; brak pakietu zatrzymuje konfigurację. Arch/Manjaro nadal wymagają
libvirta z AppArmor. Void wymaga aktywnego AppArmor i działających profili.
Playbook nie uznaje samej instalacji pakietu za potwierdzenie izolacji VM.

## Prywatne pliki odpowiedzi

`python3 render-answers.py --output /ścieżka/do/nowego/prywatnego-katalogu`
prosi o hasło lokalnie i zapisuje tylko jego solony skrót. Zachowaj pliki prywatnie;
nie dodawaj ich do Git. Generator używa Pythona 3 i OpenSSL; hasło przekazuje przez stdin, nie argumenty procesu.
Dla Debiana dostępne jest też `--desktop xfce`. Ubuntu i EL10 startują z GNOME.

Ventoy wymaga wpisów pluginu auto_install, by samodzielnie podać te pliki instalatorowi.
**Nie dodano tych wpisów**, ponieważ istniejąca konfiguracja ma zostać zachowana.
Można użyć plików odpowiedzi z oficjalnymi parametrami instalatora (preseed/url,
Ubuntu NoCloud, inst.ks) i własnym serwerem HTTP w zaufanej sieci. Plik Kickstart
nie zawiera instrukcji partycjonowania; Ubuntu pozostawia sekcję storage interaktywną.
Bez podania plików odpowiedzi ISO uruchomi zwykły instalator.

## Test Forge po zalogowaniu

Zainstaluj odpowiednią paczkę z katalogu `packages-candidate`. Dla NixOS zaimportuj
moduł ze źródeł. Ponownie zaloguj się po zmianie grup. Następnie:

```
forge --version
forge doctor
forge setup --network standard
forge prepare --all
```

Najpierw sprawdź ostrzeżenia doctor i AppArmor/SELinux. Nowe porty są eksperymentalne,
a test fizycznego startu i zatrzymania VM jest nadal do wykonania. Nie uruchamiaj
setup przy działających gościach. Przetestuj start/stop także z GNOME Boxes → QEMU
System; Whonix Workstation ma mieć wyłącznie prywatne połączenie z Gateway.

Pulpit jest instalowany jako dostępna sesja. Zmiana aktywnego menedżera logowania
(GDM/LightDM/COSMIC Greeter) pozostaje ręczna, aby nie zamykać działającej sesji
w trakcie konfiguracji. Na Void po wyborze GNOME sprawdź usługę gdm i wyłącz
konfliktujący LightDM przed kolejnym uruchomieniem. Nie uruchamiaj dwóch greeterów.

NixOS: importuj `nixos/desktop.nix` i moduł Forge. Ustaw `forgeLab.desktop` na
`"gnome"` (domyślne), `"cosmic"` albo `"xfce"`. Zachowaj własne
`hardware-configuration.nix`, bootloader i fileSystems; szablon ich nie zastępuje.
Hasło konta major ustaw przez `passwd major` z uprawnieniami administratora.

**EL10: znany blokujący problem.** Na sprawdzonym DVD AlmaLinux 10.2 i w EPEL 10.2
nie znaleziono natywnego gnome-boxes. Ansible sprawdza dostępność i zatrzymuje
konfigurację przed instalowaniem pakietów. RPM EL10 jest artefaktem do dalszego
portowania, nie obietnicą działającego systemu Boxes. Alma przywróciła SPICE,
a w stosie RHEL 10 jest usunięte; Rocky wymaga osobnej weryfikacji.
