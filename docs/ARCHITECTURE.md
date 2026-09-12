# Forge 4.0 — architektura

Stan: **tylko ten dokument**. Brak silnika, CLI i GUI.

To nie jest przepisanie 2.5 ani 3.0. Nowy produkt, te same zasady fail-closed (własność, provenance obrazów, brak zgadywania). Inny **model sieci**.

---

## 1. Po co

Laptop-lab na **Fedorze**: hypervisor nic nie śledzi. Goście mają **role**, nie „internet z NAT-u, bo tak libvirt stawia `virbr0`”.

Operator ma dwa fizyczne wyjścia na świat, **różni dostawcy**:

1. **Zwykły port Ethernet** (wbudowany) — internet A.
2. **Dongle USB-C → Ethernet** — internet B.

Forge 4.0 nie robi z Fedory mini-routera. Albo cisza, albo **prawdziwy** kabel w **jednej** VM.

---

## 2. Role (kontrakt)

Pięć ról. Profil bez roli nie istnieje. Rola nie jest „opcją sieci w kreatorze” — jest tożsamością domeny.

### `isolated`

Brak karty sieciowej. Zero `<interface>`, zero USB-net.

**Profile:** Tsurugi LAB, SIFT Workstation.

Co tam robisz: kopia dysku, Autopsy, Volatility. Nie Facebook. Nie `dnf` gościa przez sieć.

### `whonix-ws`

Jedna karta: **tylko** do Gateway (sieć libvirt `forge-whonix`, `forward=none`).

Zakaz: NAT, `type='user'`, passt, most, USB hostdev, druga NIC.

### `whonix-gw`

Dwie rzeczy, nic więcej:

1. Ta sama sieć `forge-whonix` (do Workstation).
2. **Wyłącznie USB passthrough** dongle USB-C→Ethernet (dostawca B).

Zakaz: `virbr0`, `network=default`, `<interface type='user'>`, passt, most wbudowanego Ethernetu hosta.

Bez wetkniętego dongle Gateway **wolno** wystartować — Tor po prostu nie ma wyjścia. Forge nie dokłada zastępczego NAT-u „żeby działało”.

### `osint-clearnet`

Jedna karta: **passthrough zwykłego portu Ethernet** (dostawca A). Inny internet niż dongle.

**Profil:** Kali.

Zakaz: dongle USB-C, sieć `forge-whonix`, NAT hosta. Host w tym czasie **nie** ma adresu na tym porcie.

### Host `maintenance` (to nie jest VM)

Goście **wyłączeni**. Dongle **nie** jest w Gateway. Wbudowany Ethernet **w hoście** (nie w Kali).

Wtedy i tylko wtedy: aktualizacje Fedory, instalacja KVM/libvirt, instalacja Forge, paczki hypervisoru.

Potem: odciąć internet hosta (wyjąć kabel / `nmcli down`). Wrócić do trybu sprawy.

---

## 3. Profile — zamknięta lista

Nic poza tym. Debian/Ubuntu/Fedora-gość, druga Tsurugi „pod OSINT”, Windows — poza 4.0.

| Profil | Rola | Co ściąga Forge (gdy powstanie silnik) |
|--------|------|----------------------------------------|
| **Tsurugi LAB** | `isolated` | Oficjalne **OVA** z [tsurugi-linux.org/downloads](https://tsurugi-linux.org/downloads.php). Weryfikacja: SHA512 + podpis, klucz projektu **0x116AD57C**. Import do qcow2, **wycinać wszystkie NIC** z XML. Live USB Acquire **nie** jest gościem Forge (pendrive w torbie). |
| **SIFT** | `isolated` | Oficjalne **OVA** SANS [SIFT Workstation](https://www.sans.org/tools/sift-workstation). Hash z strony SANS. Import, **wycinać NIC**. Konto SANS może być wymagane do pobrania — to ograniczenie upstreamu, nie omijamy go mirrorami. |
| **Kali** | `osint-clearnet` | Oficjalny obraz QEMU z `cdimage.kali.org` (`*-qemu-amd64`), `SHA256SUMS` + `.gpg`, klucz **827C8569F2518CC677FECA1AED65462EC8D5E4C5** (jak 3.0). Jedna NIC: hostdev wbudowanego Ethernetu, nie `user`/`default`. |
| **Whonix Gateway** | `whonix-gw` | Oficjalny pakiet **libvirt/KVM** Whonix, podpis OpenPGP, klucz **916B8D99C38EAF5E8ADC7A2A8D66066A2EEACCDA** (jak 2.5/3.0). Jeden bundle → Gateway i Workstation. |
| **Whonix Workstation** | `whonix-ws` | Ten sam bundle. |

Jedna para Whonix (jeden Gateway, jeden Workstation). Start: Gateway, potem Workstation. Stop: odwrotnie.

Tsurugi ma w Distro przełącznik OSINT — **w Forge 4.0 go nie używasz**. OSINT clearnet = Kali. OSINT Tor = Whonix Workstation.

---

## 4. Fizyczne kable

```text
ISP A  ── wbudowany Ethernet ──┬── HOST          tylko okno maintenance
                               └── Kali          passthrough, gdy host nie używa portu
                                                 (osint-clearnet)

ISP B  ── USB-C dongle Ethernet ──── Whonix Gateway     tylko to
                                      │
                                      └── forge-whonix (isolated) ── Whonix Workstation

Tsurugi     ── (brak kabla)
SIFT        ── (brak kabla)
```

Zasady:

- Jeden fizyczny NIC w **co najwyżej jednym** miejscu naraz (host **albo** jedna VM).
- Dongle nigdy w Kali, Workstation, Tsurugi, SIFT, hoście (poza przypadkiem, gdy Gateway jest off i operator świadomie serwisuje dongle — to nie jest tryb Forge).
- Wbudowany port nigdy w Gateway.
- Dwa dostawcy są **cechą**, nie przypadek: Tor i clearnet nie wychodzą tą samą firmą.

Passthrough = USB (dongle) albo PCI/USB hostdev wbudowanej karty, jeśli płyta tak daje. Forge nie emuluje karty i nie NATuje. Gość widzi **ten** sprzęt.

---

## 5. Libvirt (gdy powstanie silnik)

| | Forge 3.0 | Forge 4.0 |
|--|-----------|-----------|
| URI | `qemu:///session` (łatwe na Arch) | **`qemu:///system`** (Fedora; USB hostdev, prawdziwa sieć isolated) |
| NAT Kali | `type='user'` | **zakaz** |
| Uplink Gateway | `user` + passt | **tylko USB hostdev** |
| Link GW↔WS | UDP localhost 6688/5577 | sieć libvirt **`forge-whonix`**, `forward='none'`, bez DHCP na świat, bez `virbr0` |
| Isolated | nie było | brak `<interface>` |

`qemu:///session` nie jest URI labu śledczego: nie odda USB hostowi porządnie i pcha w `type='user'`.

Doctor (przyszły) odmawia startu, gdy:

- domena Forge ma `network=default` / `type='user'` / passt uplink,
- `isolated` ma jakąkolwiek NIC,
- `whonix-ws` ma cokolwiek poza `forge-whonix`,
- `whonix-gw` ma uplink inny niż uzgodniony USB hostdev,
- ten sam hostdev jest w dwóch domenach,
- host ma IPv4/IPv6 na NIC, który jest jednocześnie w gościu.

---

## 6. Host Fedora — cykl życia

Jedna świeża Fedora. Mało paczek: KVM, libvirt, virt-viewer, Forge (gdy będzie). LUKS. SELinux zostaje.

```text
[instalacja OS + virt + Forge]   ← wbudowany Ethernet, gości jeszcze nie ma
        ↓
[ściągnięcie i weryfikacja obrazów]  ← ten sam kabel, wciąż maintenance
        ↓
[odcięcie internetu hosta]
        ↓
[tryb sprawy]
   isolated = Tsurugi / SIFT
   dongle w Gateway tylko gdy Tor
   wbudowany port w Kali tylko gdy clearnet
        ↓
[okno serwisowe]
   wszystkie VM off
   kabel z powrotem w hosta
   dnf update (+ ewentualnie nowe obrazy)
   odciąć znowu
```

Aktualizacje przy wyłączonych VM są **warunkiem**, nie tarczą. Łatają hypervisor. W oknie serwisowym host nie jest przeglądarką OSINT.

Obrazy gości aktualizujesz **nową zweryfikowaną bazą**, nie `apt` w środku sprawy na Tsurugi z doklejonym NAT-em.

---

## 7. Fail-closed (z 2.5/3.0, bez regresji)

- Provenance: obrazy tylko z oficjalnych drzew + podpis/hash jak w tabeli profili. Brak podpisu = brak instalacji.
- Własność: kasowanie domeny tylko przy zgodności UUID + metadata Forge + ścieżka dysku.
- Brak guest-exec, SSH, QGA, cloud-init jako kanału sterowania.
- Wyświetlacz: zewnętrzny `virt-viewer` / `remote-viewer`, nie osadzony pulpit w Forge.
- Cache obrazów `~/.local/share/forge/cache/` (albo `$FORGE_DATA_DIR`); skasowanie VM nie kasuje cache.
- Przerwana operacja → stan recovery, nie „udane, bo XML jest”.

Świadomie **nie** przenosimy z 3.0: user-mode NAT, passt na Gateway, `qemu:///session` jako default.

---

## 8. Świadomie poza 4.0

- RAID, LVM spanning, druga Tsurugi online.
- Most hosta (`virbr0`) „dla wygody”.
- Whonix Workstation z Wi‑Fi / drugim dongle.
- Tails jako profil (pendrive, nie libvirt).
- Qubes.
- Windows / Cellebrite — inny stos, nie Forge.
- MNP — osobny projekt; tu nie ma gościa MNP.

---

## 9. Ten cut vs następny

| Ten commit | Później |
|------------|---------|
| To repo, ten plik, README, Apache-2.0 | `forge-core` + CLI na Fedorze |
| Role i kable zamknięte | Doctor, import OVA, hostdev, sieć `forge-whonix` |
| Brak kodu | Ściąganie: Whonix, Tsurugi, SIFT, Kali |

Zmiana roli profilu (np. Kali → `isolated`) to zmiana kontraktu, nie flagi w GUI.
