#!/usr/bin/env python3
"""Check dependencies from the canonical Arch package recipe, then install missing packages."""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess

HERE = Path(__file__).resolve().parent
TOOLS = ('python3', 'cargo', 'makepkg', 'extra-x86_64-build', 'virsh', 'qemu-img',
         'gpg', 'curl', '7z', 'nmcli', 'ip', 'nft', 'aa-status', 'systemctl')


def requirements():
    recipe = (HERE.parent / 'PKGBUILD.in').read_text()
    result = []
    for field in ('depends', 'makedepends'):
        match = re.search(r'^' + field + r'=\((.*?)\)', recipe, re.M | re.S)
        if not match:
            raise ValueError('Brak listy zależności: ' + field)
        result.extend(shlex.split(match[1]))
    return list(dict.fromkeys(result + ['base-devel', 'devtools', 'git', 'edk2-ovmf']))


def check_platform(text=None):
    text = Path('/etc/os-release').read_text() if text is None else text
    values = {}
    for line in text.splitlines():
        if '=' in line and not line.startswith('#'):
            key, value = line.split('=', 1)
            parsed = shlex.split(value)
            values[key] = ' '.join(parsed)
    if values.get('ID') not in ('arch', 'cachyos', 'manjaro') and 'arch' not in values.get('ID_LIKE', '').split():
        raise ValueError('Wymagany system z rodziny Arch i pacman.')
    if platform.machine() != 'x86_64':
        raise ValueError('Ten instalator wymaga x86_64.')
    if not shutil.which('pacman') or not shutil.which('sudo'):
        raise ValueError('Brak pacman lub sudo; zainstaluj je przed uruchomieniem instalatora.')


def missing_packages(packages):
    result = subprocess.run(['pacman', '-T', *packages], capture_output=True, text=True)
    if result.returncode not in (0, 127):
        raise ValueError('Nie można sprawdzić bazy pacmana: ' + result.stderr.strip())
    return result.stdout.splitlines()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='Only report missing dependencies')
    args = parser.parse_args()
    check_platform()
    packages = requirements()
    missing = missing_packages(packages)
    print(json.dumps({'required': packages, 'missing': missing}, ensure_ascii=False, indent=2), flush=True)
    if args.check:
        return 1 if missing else 0
    if os.geteuid() == 0:
        raise ValueError('Uruchom jako użytkownik pulpitu, bez sudo.')
    # Rolling release: never perform a partial upgrade. Pacman asks before changes.
    subprocess.run(['sudo', 'pacman', '-Syu', '--needed', *missing], check=True)
    remaining = missing_packages(packages)
    absent_tools = [tool for tool in TOOLS if not shutil.which(tool)]
    if remaining or absent_tools:
        raise ValueError(f'Niepełne prerequisites: pakiety={remaining}, narzędzia={absent_tools}')
    print('Prerequisites sprawdzone i zainstalowane.')
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error
