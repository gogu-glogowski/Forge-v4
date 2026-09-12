# Forge 4.0 — architektura

Stan: **dokument**. Brak silnika.

Nie przepisujemy 2.5/3.0. Bierzemy stamtąd: fail-closed, podpisane obrazy, para Whonix bez uplinku na Workstation. **Nie bierzemy:** lasu komend z README v2 (`plan` / `inspect` / `fetch` / `prepare`), gościa Fedora Workstation, NAT-u hosta dla VM.

---

## 1. Dwa kable, człowiek wkłada i wyjmuje

| | Dostawca | Sprzęt | Kto |
|--|----------|--------|-----|
| **A** | internet hosta | wbudowany Ethernet | **tylko Fedora** |
| **B** | inny internet | USB-C → Ethernet (domyślnie) albo USB Wi‑Fi | **tylko VM**, passthrough |

A i B to nie automat. Operator **podpina albo wypina**.

- **A wpięte:** okno serwisowe. VM wyłączone (albo przynajmniej bez dongle). `dnf`, instalacja libvirt/Rust/Forge, `forge pull`.
- **A wyjęte:** tryb sprawy. Host bez default route na świat.
- **B wpięte:** passthrough do **jednej** VM, która w tej chwili ma prawo do internetu: `whonix-gw` **albo** `kali`, nigdy obie, nigdy Tsurugi/SIFT/Workstation.
- **B wyjęte:** żadna VM nie ma świata. Isolated i tak nigdy nie ma.

Kabel łatwiejszy niż Wi‑Fi: mniej sond SSID, mniej obcych AP. USB Wi‑Fi jest **tym samym slotem B**, nie trzecim internetem.

Host **nigdy** nie NATuje gości (`virbr0` / `type='user'` / passt). Gość albo ślepy, albo trzyma **ten** dongle.

```text
ISP A ── wbudowany Ethernet ── tylko HOST (maintenance / pull)
                                 potem wyjmij

ISP B ── USB dongle ──┬── Whonix Gateway     (gdy Tor)
                      └── Kali               (gdy clearnet OSINT)
                      (człowiek przekłada; status krzyczy gdy oba)

Tsurugi / SIFT / Whonix Workstation ── brak B, brak A
```

---

## 2. Role (bez zmian poza kablem B)

| Rola | NIC | Profile |
|------|-----|---------|
| `isolated` | brak | Tsurugi, SIFT |
| `whonix-ws` | tylko `forge-whonix` (`forward=none`) | Whonix Workstation |
| `whonix-gw` | `forge-whonix` + **wyłącznie** USB hostdev **B** | Whonix Gateway |
| `osint-clearnet` | **wyłącznie** USB hostdev **B** | Kali |

Gateway bez dongle: wolno startować, Tor nie wyjdzie. Forge **nie** dokłada NAT-u.

`status` jest strażnikiem roli: zła karta, `network=default`, dongle w dwóch domenach, isolated z NIC → błąd, nie ostrzeżenie.

---

## 3. Obrazy: na dysku zawsze qcow2

Ściąganie może być OVA/7z. Po `pull` kanon to **qcow2** (jedna baza na profil). Create/clone operują na nazwie VM i na tym qcow2, nie na pliku, który operator wskazuje ręcznie.

| Profil | Upstream | Weryfikacja (sensowna, nie NASA) | Wynik `pull` |
|--------|----------|-----------------------------------|--------------|
| **Kali** | `cdimage.kali.org` obraz QEMU `*-qemu-amd64` | HTTPS + `SHA256SUMS` + `.gpg`, klucz `827C8569F2518CC677FECA1AED65462EC8D5E4C5` | qcow2 |
| **Tsurugi LAB** | OVA z [tsurugi-linux.org/downloads](https://tsurugi-linux.org/downloads.php) | SHA512 + podpis, klucz **0x116AD57C** | konwersja OVA → qcow2, **wycinać NIC** |
| **SIFT** | OVA SANS [SIFT Workstation](https://www.sans.org/tools/sift-workstation) | hash z oficjalnej strony SANS (konto SANS może być wymagane — bez luster) | OVA → qcow2, **wycinać NIC** |
| **Whonix** | oficjalny pakiet libvirt/KVM, jeden bundle na parę | OpenPGP, klucz `916B8D99C38EAF5E8ADC7A2A8D66066A2EEACCDA` | dwa qcow2 (gw, ws) |

Poza listą nic: zero gościa Fedora, Debian, Ubuntu, Windows, drugiej Tsurugi „pod OSINT”.

Tsurugi Acquire (live USB) nie jest profilem Forge.

### Baza vs nakładka — to jest „sprawdzenie”, nie demon

**Nie** ma procesu w tle, który co chwilę haszuje qcow2.

Żywy dysk VM **musi** się zmieniać (Ty w Tsurugi, `apt` w Kali). Hash całego pliku po tygodniu pracy zawsze „nie wyjdzie”. Albo ignorujesz alarm (architektura bez wartości), albo boisz się własnych zmian. Oba złe.

Zamiast tego dwa pliki na profil:

```text
pull →  base-<profil>.qcow2     tylko do odczytu, digest zapisany raz
create →  <vm>.qcow2            nakładka (overlay), tu jest Twoja praca
```

Co się sprawdza, **kiedy** (przy komendzie, nie w tle):

| Kiedy | Co | Po co |
|-------|-----|--------|
| `pull` | podpis/suma **upstreamu**, potem digest **bazy** | czy ściągnęliśmy to, co wydawca podpisał |
| `create` / `start` | digest **bazy** nadal się zgadza; nakładka wskazuje na tę bazę | czy ktoś podmienił kanon pod spodem |
| `start` / `status` | XML vs **rola** (NIC, dongle B exclusive, isolated bez karty) | czy po cichu nie wrócił `virbr0` / druga karta — to jest prawdziwe „zapomniałem, że zmieniłem” |
| nigdy | hash nakładki `<vm>.qcow2` | to Twoje dane, nie pieczęć |

Brak daemona = mniej powierzchni, host śledczy zostaje cichy. `status` i `start` są strażnikiem. Jeśli baza się nie zgadza — **odmowa startu**, nie żółta naklejka.

---

## 4. CLI — od pierwszej linijki dwa tryby

Jedna binarka. **`forge` bez argumentów i `--help` pokazuje tylko tryb użytkownika.** Reszta schowana pod `forge dev`.

### `user` (friendly) — to jest produkt

```text
forge pull <profil>
forge create <profil> [nazwa]
forge clone <vm> <nowa-nazwa>
forge start <vm>
forge stop <vm>
forge status [vm]
forge list
forge delete <vm>
```

**`pull`** — jedyna komenda pobrania. Zastępuje `image inspect` + `image fetch`. Kryptografia jak w tabeli wyżej.

**`create kali`** — VM o nazwie `kali` (albo `create kali kali-2`). Rola z profilu, nie z flagi. `create tsurugi`, `create sift` analogicznie. **`create whonix`** stawia **parę** (`whonix-gateway` + `whonix-workstation`), nie dwie zgadywane komendy.

**`clone`** — źródło to **nazwa VM**, nie ścieżka qcow2. `clone kali kali-2`. Whonix: **odmowa** (jedna para; klon pary to nie 4.0).

**`stop`** — ACPI. `stop --force` to osobne ucięcie zasilania (jak v2, bez cichej eskalacji).

**`status`** — czy domena żyje **oraz** czy XML zgadza się z rolą (NIC, dongle, `forge-whonix`). Bez nazwy: cały lab.

**`list`** — **propozycja zamiast** `profile list` + `image list`:

```text
profile    image          vms
kali       ready          kali, kali-2
tsurugi    missing        —
sift       ready          sift
whonix     ready          whonix-gateway, whonix-workstation
```

Jedna tablica. Osobne `profile list` / `image list` nie wracają.

**`delete`** — fail-closed, tylko to co Forge udowodni że jest jego. **Baza** z `pull` zostaje.

### `developer` — `forge dev …`

Nie mieszają się w `--help`. Ścieżka dla nas i diagnostyki:

```text
forge dev doctor          # Fedora 44, KVM, libvirtd, URI, brak NAT na naszych domenach
forge dev xml <vm>        # zrzut XML (rola vs rzeczywistość)
forge dev create --dry-run …
forge dev delete --dry-run …
```

`doctor` **nie** jest komendą z README dla gościa. Po instalacji: `forge dev doctor`. Jeśli kiedyś recovery wróci — tylko tutaj, nie jako trzeci workflow.

### Do kosza (v2, przerost)

- `forge vm plan`
- `forge image inspect` / `forge image fetch` (jest `pull`)
- `forge image prepare` / `prepare-start` / `prepare-promote` / cały gość **Fedora Workstation**
- `forge vm create <profil> <instancja>` jako trzyetapowy rytuał z `--dry-run` obowiązkowym
- `forge vm fresh`
- `forge state adopt` / `rebuild` / `recover` jako codzienność
- `forge profile list` + `forge image list` jako para
- Disposable, GME, QGA, SSH do gościa, cloud-init

Jeśli kiedyś recovery będzie musiało wrócić, to pod `doctor`, nie jako trzeci workflow.

---

## 5. Libvirt

- URI: **`qemu:///system`**
- Sieć `forge-whonix`: `forward='none'`, tylko GW↔WS
- Hostdev USB: jeden dongle B, exclusive
- Storage: qcow2 pod kontrolą Forge (`$FORGE_DATA_DIR` albo `~/.local/share/forge/`)
- Display: `virt-viewer` / virt-manager, nie osadzony GUI w tym cutcie
- Start Whonix: najpierw Gateway, potem Workstation; stop odwrotnie (`start`/`stop` na parze może to wymusić później; na razie dokumentowane)

Doctor odmawia, gdy URI to session, gdy brak KVM, gdy Fedora < 44.

---

## 6. Host — cykl

```text
Fedora 44 świeża
  → A wpięte: dnf, @virtualization, rust, Forge, forge dev doctor, forge pull …
  → A wyjęte
  → sprawy: B w Gateway albo w Kali, isolated bez kabla
  → serwis: VM off, A z powrotem, dnf, A wyjęte
```

`pull` wymaga A (albo innej chwilowej drogi hosta na świat). Nie ściągamy obrazów przez dongle B w gościu.

---

## 7. Fail-closed (z v2, bez NASA)

Zostaje: dokładna własność (UUID + metadata + ścieżka), digest zapisany przy `pull`, kasowanie tylko udowodnionych zasobów, brak zgadywania po podobnej nazwie.

Nie wraca: pełne przehashowanie obrazu na `start`, łańcuch SLSA, gość jako źródło prawdy, cichy rollback.

---

## 8. Ten cut

| Jest | Nie ma |
|------|--------|
| Ten plik + README | kod Rust |
| Role, kable A/B, qcow2, CLI | import OVA, hostdev, doctor w binariów |

Następny commit z kodem: `doctor` + `pull` + `create` dla jednego profilu isolated — nie GUI.
