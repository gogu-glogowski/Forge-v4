# Forge 4.2.2 — kompletny instalator Standard / Extended

Wydanie paczki 4.2.2-5. Walidacja na CachyOS jest w toku; testy fizycznej
izolacji A/B i pełnego cyklu VM nie zostały jeszcze zakończone.

## Uruchamianie

Ze źródeł: `bash install.sh` (równoważne `bash install.sh --stage install`).
Po zbudowaniu i instalacji paczki: `forge-install`.
Uruchamiaj jako zwykły użytkownik pulpitu; wymagane operacje same pytają o sudo.

Pełna instalacja wykonuje kolejno:

1. Sprawdzenie platformy x86_64 z rodziny Arch, pacmana i sudo. Odczyt wymaganych
   zależności z tego samego PKGBUILD, z którego powstaje paczka. Kontrola braków
   przez pacman -T, pełna aktualizacja pacman -Syu --needed z jawną listą tylko
   brakujących zależności, ponowna kontrola
   zależności i narzędzi. Pacman pyta przed instalacją; nie ma częściowej aktualizacji.
2. Wybór Standard albo Extended. Extended to dotychczasowy Dedicated A/B;
   alias dedicated pozostaje obsługiwany. Extended wymaga jawnego USB ID adaptera B.
   Instalator pokazuje plan i prosi o jego zastosowanie.
3. Jeśli host nie ma aktywnego AppArmor/libvirt i nowego Forge, etap host przygotowuje
   libvirt z AppArmor, Forge, usługi i grupy. Wymagany jest ręczny restart; kod 75
   oznacza oczekiwanie na restart. Wybór zapisuje się w pending-install.json.
   Po restarcie uruchom instalator ponownie — wznowi wybrany wariant.
4. Pobranie i weryfikacja wszystkich automatycznych obrazów: Tsurugi, Kali, Whonix
   Gateway i Workstation. Nowa komenda prepare --images-only nie instaluje hooków,
   nie zmienia sieci i nie definiuje VM. Istniejące poprawne bazy są weryfikowane
   i ponownie wykorzystywane; brakująca baza używana przez VM nie jest nadpisywana.
5. Dopiero po sukcesie obrazów: instalacja/weryfikacja wspólnego hooka z polityką
   wybranego wariantu oraz konfiguracja sieci. Extended dodatkowo egzekwuje A/B.
   Nie ma oddzielnego wyboru hooka niezgodnego z wariantem. W Extended trasy IPv4
   i IPv6 są kontrolowane przed pobieraniem i ponownie przed zmianą sieci.
6. Utworzenie/sprawdzenie czterech VM, zgodność ról, stan shutoff i doctor.
   Żadna VM nie jest automatycznie uruchamiana. Końcowy sukces wymaga wszystkich
   kontroli; błąd pozostawia zapis umożliwiający ponowienie instalacji.

SIFT pozostaje w kodzie i jest dostępny do ręcznego importu OVA SANS; zgodnie
z ustaleniem nie jest automatycznie pobierany ani częścią domyślnej instalacji.

Standard: Kali korzysta z NAT hosta; Whonix Workstation tylko przez Gateway;
Tsurugi pozostaje izolowane. Extended: PCI A dla hosta, USB B dla gości,
bez awaryjnego użycia A przez gości. Przy wielu USB NIC lub trasie hosta przez B
instalator odmawia zmiany. Zmiana wariantu dotyczy całego labu, także istniejących VM.

## Podgląd i poszczególne etapy

```bash
# Wyłącznie podgląd, bez sudo i zmian:
bash install.sh --stage plan --network standard
bash install.sh --stage plan --network extended --dongle-b 0b95:1790
# Wyłącznie kontrola zależności (kod 1 oznacza braki):
bash install.sh --stage prerequisites --check
# Pełny instalator; podany USB ID to przykład:
bash install.sh --network extended --dongle-b 0b95:1790
# Etap lab dla przygotowanego hosta:
bash install.sh --stage lab --network standard --music without
```

Pełny instalator zawsze wybiera wszystkie automatyczne obrazy i ignoruje dawny
częściowy wybór. Ekspercki etap lab nadal pozwala na --images kali,whonix;
nie usuwa pozostałych istniejących maszyn. Opcje lab są w installer-options.json.
Opcjonalna muzyka wymaga mpv; brakujący odtwarzacz jest instalowany tylko na żądanie.

## Paczka i granice obsługi

PKGBUILD instaluje /usr/bin/forge, /usr/bin/forge-install, dokumentację i archiwum
pasujących źródeł instalatora. Python jest jawną zależnością. Launcher rozpakowuje
źródła do prywatnego katalogu w ~/.cache/forge/installer-runs i zachowuje logi/artefakty.
Paczka nazywa się forge-lab; repozytoryjne forge to niezwiązany program ArrayFire.

Wykrywana jest rodzina Arch przez ID/ID_LIKE. Automatyczne przygotowanie rozruchu
nadal obsługuje rozpoznaną konfigurację Limine z limine-mkinitcpio; inne bootloadery
są odrzucane. Nie potwierdzono wdrożenia na wszystkich pochodnych Archa.
Libvirt jest budowany z przypiętego przepisu, z aktywnymi testami i AppArmor.
Już spełnione zależności nie są ponownie podawane jako cele instalacji pacmana,
co zapobiega wymuszonemu obniżeniu lokalnej wersji libvirt.
Aktualizacja systemu może zastąpić lokalny libvirt — rzeczywiste capabilities
muszą potwierdzać AppArmor. Nie omijamy podpisów ani sum kontrolnych pacmana.

Budowanie samej paczki Forge odbywa się jako użytkownik; sudo jest potrzebne do instalacji i konfiguracji
hosta, a przy budowie libvirt również do clean chroot.

## Walidacja

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s packaging/arch/install -p 'test*.py' -v
cargo test --workspace --offline --locked
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
```

Testy obejmują zależności, wybory i alias Extended, wszystkie obrazy, kolejność
pobranie → hooki, przerwany transfer, zmianę trasy podczas transferu, wznowienie
po restarcie i zawartość katalogu testowego paczki. Katalog testowy używa atrapy
binarki i nie jest gotową paczką. Testy sprzętu używają sztucznego sysfs.
Fizyczny start/stop, AppArmor procesu QEMU, odpięcie B oraz izolacja połączeń
wewnątrz gości wymagają późniejszej walidacji na hoście.
