#!/usr/bin/env python3
"""Host stage for the reviewed CachyOS/Limine target; never invokes a shell."""
import argparse
import hashlib
import os
from pathlib import Path
import pwd
import re
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
RECIPE_SHA = '5bfcd14b88222f7091303d7f78f10d333c0dca27f3dccfae9ee6e4205edc3132'
BOOT_LINE = 'KERNEL_CMDLINE[default]+=" apparmor=1 lsm=landlock,lockdown,yama,integrity,apparmor,bpf"'
MARKER = '# Forge: AppArmor for confined libvirt guests'


def run(*args):
    subprocess.run(args, check=True)


def updated_boot(text):
    if BOOT_LINE in text.splitlines():
        return text
    if MARKER in text or re.search(r'\blsm=|\bapparmor=', text):
        raise ValueError('Istnieją już ustawienia LSM/AppArmor: wymagają przeglądu przed zmianą.')
    if not re.search(r'^KERNEL_CMDLINE\[default\]\+?=', text, re.M):
        raise ValueError('Brak rozpoznanej konfiguracji Limine; nie zmieniam rozruchu.')
    return text.rstrip() + '\n\n' + MARKER + '\n' + BOOT_LINE + '\n'


def ensure_no_guests():
    for proc in Path('/proc').glob('[0-9]*/comm'):
        try:
            name = proc.read_text().strip()
        except (OSError, ProcessLookupError):
            continue
        if name.startswith('qemu-system') or name in ('qemu-kvm', 'kvm'):
            raise ValueError('Zatrzymaj wszystkie maszyny QEMU przed zmianą libvirt.')


def preflight():
    from prerequisites import check_platform
    check_platform()
    if not Path('/etc/default/limine').is_file() or not shutil.which('limine-mkinitcpio'):
        raise ValueError('Automatyczna konfiguracja rozruchu wymaga Limine z limine-mkinitcpio.')
    updated_boot(Path('/etc/default/limine').read_text())
    ensure_no_guests()
    for unit in ('virtqemud.service', 'virtqemud.socket'):
        if subprocess.run(['systemctl', 'is-active', '--quiet', unit]).returncode == 0:
            raise ValueError('Aktywne modularne libvirt: wymagany przegląd przed wyborem usługi.')


def recipe(destination):
    source = HERE / 'libvirt/PKGBUILD.upstream'
    if hashlib.sha256(source.read_bytes()).hexdigest() != RECIPE_SHA:
        raise ValueError('Niezgodny hash przypiętego przepisu libvirt.')
    text = source.read_text()
    for option in ('apparmor', 'apparmor_profiles', 'secdriver_apparmor'):
        old = f'-D{option}=disabled'
        if text.count(old) != 1:
            raise ValueError(f'Przepis wymaga przeglądu: {option}')
        text = text.replace(old, f'-D{option}=enabled')
    text = text.replace('depends=(\n', 'depends=(\n  apparmor\n', 1)
    text = text.replace('pkgrel=1\n', 'pkgrel=1.2\n', 1)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(text)
    print(f'Przepis z AppArmor: {destination}')


def inspect(package):
    info = subprocess.check_output(['bsdtar', '-xOf', str(package), '.PKGINFO'], text=True)
    for expected in ('pkgname = libvirt', 'pkgver = 1:12.8.0-1.2', 'depend = apparmor'):
        if expected not in info.splitlines():
            raise ValueError(f'Nieoczekiwane metadane libvirt: brak {expected}')
    paths = subprocess.check_output(['bsdtar', '-tf', str(package)], text=True).splitlines()
    paths = {p.removeprefix('./') for p in paths}
    for expected in ('usr/lib/libvirt/virt-aa-helper',
                     'etc/apparmor.d/usr.lib.libvirt.virt-aa-helper',
                     'etc/apparmor.d/abstractions/libvirt-qemu',
                     'etc/apparmor.d/libvirt/TEMPLATE.qemu'):
        if expected not in paths:
            raise ValueError(f'Niepełny pakiet AppArmor: brak {expected}')
    print('Metadane, virt-aa-helper i profile obecne. Test uruchomienia nastąpi na hoście.')


def configure(user):
    if os.geteuid() != 0:
        raise ValueError('Konfiguracja hosta wymaga sudo.')
    account = pwd.getpwnam(user)
    if account.pw_uid < 1000 or account.pw_uid == 65534:
        raise ValueError('Wskaż zwykłego użytkownika pulpitu.')
    preflight()
    for path in ('/usr/lib/libvirt/virt-aa-helper', '/etc/apparmor.d/usr.lib.libvirt.virt-aa-helper', '/usr/bin/forge'):
        if not Path(path).is_file():
            raise ValueError(f'Brak wymaganego pliku: {path}')
    boot = Path('/etc/default/limine')
    old = boot.read_text()
    new = updated_boot(old)
    if new != old:
        # Keep a unique root-owned recovery copy; never overwrite a prior backup.
        fd, backup_name = tempfile.mkstemp(prefix='limine.forge-backup-', dir='/etc/default')
        with os.fdopen(fd, 'w') as backup:
            backup.write(old)
        print(f'Kopia konfiguracji rozruchu: {backup_name}', flush=True)
        try:
            boot.write_text(new)
            run('limine-mkinitcpio')
        except BaseException:
            boot.write_text(old)
            subprocess.run(['limine-mkinitcpio'], check=False)
            raise
    run('systemd-sysusers')
    run('usermod', '-aG', 'libvirt,kvm', user)
    # Start after reboot, when AppArmor is active. Do not start unconfined libvirt now.
    run('systemctl', 'enable', 'apparmor.service', 'libvirtd.service', 'virtlogd.socket', 'virtlockd.socket')
    print('Rozruch i grupy przygotowane. Wymagany restart; nie uruchomiono żadnej VM.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['preflight', 'recipe', 'inspect', 'configure'])
    parser.add_argument('value', nargs='?')
    args = parser.parse_args()
    try:
        if args.action == 'preflight':
            preflight()
        elif args.action == 'recipe':
            recipe(Path(args.value))
        elif args.action == 'inspect':
            inspect(Path(args.value))
        else:
            configure(args.value)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error


if __name__ == '__main__':
    main()
