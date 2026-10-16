#!/usr/bin/env python3
"""Assemble one AlmaLinux bundle without installing anything on the build host."""
import argparse, hashlib, shutil, tarfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--rpm', type=Path, required=True)
p.add_argument('--dependencies', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
if a.output.exists():
    raise SystemExit('Output already exists; choose a new versioned destination')
a.output.mkdir(parents=True)
files = [a.rpm]
for pattern in ['gnome-boxes-*.rpm', 'libvirt-gconfig-*.rpm', 'libvirt-gobject-*.rpm']:
    found = list(a.dependencies.glob(pattern))
    if len(found) != 1:
        raise SystemExit(f'Expected one {pattern}')
    files += found
for f in files: shutil.copyfile(f, a.output / f.name)
shutil.copyfile(REPO / 'target/release/forge', a.output / 'forge-installer')
(a.output / 'forge-installer').chmod(0o755)
shutil.copyfile(REPO / 'packaging/rpm/install-almalinux.sh', a.output / 'install-almalinux.sh')
shutil.copyfile(REPO / 'PORT-CONTRACT.md', a.output / 'PORT-CONTRACT.md')
(a.output / 'README.PL.md').write_text('''# Forge — AlmaLinux 10 x86_64

Uruchom jako zwykły użytkownik w terminalu:

    bash install-almalinux.sh

Pierwsze pytanie wybiera muzykę. Instalator sprawdza prerequisites, pyta o
Standard/Dedicated i magazyn, instaluje Forge, hook oraz przygotowuje cztery VM.
Wymaga wyłączonych gości, internetu, SELinux Enforcing i dostępnego KVM.
Istniejące obrazy i VM są zachowywane; nie migruje automatycznie istniejących danych.
SIFT pomijany. Nie uruchamia gości ani nie restartuje hosta automatycznie.
Raport znajduje się w ~/.local/share/forge/installation-report.json.

Kandydat do testów: pełna macierz fizycznych instalacji, reboot i sprzętowy
Dedicated muszą zostać potwierdzone. Sumy SHA256 wykrywają zmianę plików;
nie stanowią podpisu wydawcy. Forge/Boxes są lokalnymi niepodpisanymi RPM.
''')
entries = sorted(f for f in a.output.iterdir() if f.is_file())
(a.output / 'SHA256SUMS').write_text(''.join(f'{hashlib.sha256(f.read_bytes()).hexdigest()}  {f.name}\n' for f in entries))
with tarfile.open(str(a.output)+'.tar.xz', 'w:xz') as t: t.add(a.output, arcname=a.output.name)
print(a.output)
