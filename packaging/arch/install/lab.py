#!/usr/bin/env python3
"""Resumable desktop-user lab stage; Forge owns all image, hook and VM operations."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import xml.etree.ElementTree as ET

from plan import DEFAULT_IMAGES, build_plan, validate_options

FORGE = '/usr/bin/forge'

EXPECTED = {'tsurugi', 'kali', 'whonix-gateway', 'whonix-workstation'}


def output(*args):
    return subprocess.check_output(args, text=True, env={**os.environ, 'LC_ALL': 'C'})


def run(*args):
    print('+ ' + ' '.join(map(str, args)), flush=True)
    subprocess.run(args, check=True)


def verify_capabilities(text):
    root = ET.fromstring(text)
    if 'apparmor' not in [n.text for n in root.findall('./host/secmodel/model')]:
        raise ValueError('Libvirt nie udostępnia AppArmor. Nie pobieram obrazów ani nie tworzę VM.')


def verify_inventory(text, expected=EXPECTED):
    rows = {line.split()[0]: line for line in text.splitlines() if line.split()}
    for name in expected:
        if name not in rows or 'role=ok' not in rows[name] or 'ROLE FAIL' in rows[name] or 'shutoff' not in rows[name].split():
            raise ValueError(f'Brak poprawnie zweryfikowanej maszyny: {name}')


def dedicated_preflight(dongle):
    # Existing cable enforcement quarantines every USB NIC. Limit this installer
    # to one USB network adapter, selected explicitly, and a working PCI A route.
    usb = []
    for iface in Path('/sys/class/net').iterdir():
        device = (iface / 'device').resolve()
        for parent in [device, *device.parents]:
            if (parent / 'idVendor').is_file() and (parent / 'idProduct').is_file():
                usb.append((iface.name, (parent / 'idVendor').read_text().strip() + ':' + (parent / 'idProduct').read_text().strip()))
                break
    if len(usb) != 1 or usb[0][1].lower() != dongle.lower():
        raise ValueError('Instalator A/B wymaga dokładnie jednego adaptera USB net, zgodnego z wyborem B. Sprawdź forge dev usb.')
    routes = json.loads(output('ip', '-j', '-4', 'route', 'show', 'default'))
    if not routes:
        raise ValueError('Brak domyślnej trasy IPv4 przez A.')
    # Quarantining USB B also removes host IPv6 connectivity through it.
    routes += json.loads(output('ip', '-j', '-6', 'route', 'show', 'default'))
    for route in routes:
        iface = route.get('dev', '')
        device = (Path('/sys/class/net') / iface / 'device').resolve()
        if not iface or '/pci' not in str(device) or '/usb' in str(device) or (Path('/sys/class/net') / iface / 'wireless').exists():
            raise ValueError('Przed A/B połącz host przez przewodowy interfejs PCI A. Nie odłączam obecnego uplinku.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--network', choices=['standard', 'extended', 'dedicated'])
    parser.add_argument('--music', choices=['with', 'without'])
    parser.add_argument('--dongle-b', help='USB Ethernet vvvv:pppp, required for dedicated mode')
    parser.add_argument('--images', help='Comma-separated tsurugi,kali,whonix; Whonix is always a pair')
    parser.add_argument('--plan', action='store_true', help='Print an offline JSON plan; no commands, files or sudo')
    parser.add_argument('--yes', action='store_true', help='Apply the displayed choices without the final prompt')
    args = parser.parse_args()
    if args.plan:
        options = {'network': args.network or 'standard', 'music': args.music or 'without',
                   'images': args.images.split(',') if args.images is not None else DEFAULT_IMAGES,
                   'dongle_b': args.dongle_b}
        print(json.dumps(build_plan(options), ensure_ascii=False, indent=2))
        return
    data = Path(os.environ.get('XDG_DATA_HOME') or Path.home() / '.local/share') / 'forge'
    state = data / 'installer-options.json'
    options = json.loads(state.read_text()) if state.exists() else {}
    if args.images is not None:
        options['images'] = args.images.split(',')
    for key in ('network', 'music', 'dongle_b'):
        if getattr(args, key) is not None:
            options[key] = getattr(args, key)
    if not options.get('network'):
        options['network'] = input('Sieć [standard/extended, domyślnie standard]: ').strip() or 'standard'
    # Full installation always selects all automatic images; old partial choices do not carry over.
    if args.images is None:
        options['images'] = DEFAULT_IMAGES.copy()
    if not options.get('music'):
        options['music'] = input('Muzyka [with/without, domyślnie without]: ').strip() or 'without'
    if options['network'] in ('dedicated', 'extended') and not options.get('dongle_b'):
        options['dongle_b'] = input('Adapter B (vvvv:pppp): ').strip()
    plan = build_plan(options)
    print(json.dumps(plan, ensure_ascii=False, indent=2), flush=True)
    if not args.yes and input('Zastosować ten plan? [tak/N]: ').strip().lower() != 'tak':
        print('Anulowano. Bez zmian.')
        return
    if os.geteuid() == 0:
        raise ValueError('Uruchom jako użytkownik pulpitu, bez sudo.')
    if not Path(FORGE).is_file():
        raise ValueError('Najpierw ukończ etap 02-host.sh.')
    if Path('/sys/module/apparmor/parameters/enabled').read_text().strip() != 'Y':
        raise ValueError('AppArmor nie jest aktywny. Najpierw ukończ etap 2 i zrestartuj komputer.')
    if 'libvirt' not in output('id', '-nG').split():
        raise ValueError('Sesja nie ma grupy libvirt. Po etapie 2 wymagany jest restart lub ponowne logowanie.')
    verify_capabilities(output('virsh', '-c', 'qemu:///system', 'capabilities'))
    aa = json.loads(output('sudo', 'aa-status', '--json'))
    if not any(name.endswith('virt-aa-helper') and mode == 'enforce' for name, mode in aa.get('profiles', {}).items()):
        raise ValueError('Profil virt-aa-helper nie jest załadowany w trybie enforce.')
    if output('virsh', '-c', 'qemu:///system', 'list', '--name').strip():
        raise ValueError('Zatrzymaj wszystkie maszyny QEMU przed przygotowaniem labu.')
    # Fail before modifying the host when an older Forge lacks profile selection.
    prepare_help = output(FORGE, 'prepare', '--help')
    if not all(flag in prepare_help for flag in ('--profiles', '--images-only')):
        raise ValueError('Zainstaluj nową paczkę Forge z obsługą prepare --profiles i --images-only; bez zmian w sieci.')
    if options['network'] in ('dedicated', 'extended'):
        dedicated_preflight(options['dongle_b'])
    if options['music'] == 'with' and not shutil.which('mpv'):
        run('sudo', 'pacman', '-S', '--needed', 'mpv')
    data.mkdir(parents=True, exist_ok=True)
    # Save choices, never sudo credentials or a false completion marker.
    temporary = state.with_suffix('.tmp')
    temporary.write_text(json.dumps(options, indent=2) + '\n')
    temporary.chmod(0o600)
    temporary.replace(state)
    for command in plan['commands']:
        if command[1] == 'setup' and plan['network'] == 'dedicated':
            # Downloads can take hours: check routes again immediately before changing networking.
            dedicated_preflight(options['dongle_b'])
            (data / 'dongle-b').write_text(options['dongle_b'].lower() + '\n')
        run(*command)
    inventory = output(FORGE, 'status')
    print(inventory)
    verify_inventory(inventory, plan['vms'])
    run(FORGE, 'dev', 'doctor')
    print('Wybrane VM przygotowane i wyłączone. Otwórz ponownie GNOME Boxes → QEMU System.')
    print('Konfiguracja i role sprawdzone. Test rozruchu/wyłączenia oraz sieci wewnątrz gości pozostaje osobnym etapem walidacji portu.')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, ET.ParseError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error
