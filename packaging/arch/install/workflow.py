#!/usr/bin/env python3
"""Full source/packaged installer; never reboot automatically."""
import argparse
import grp
import json
import os
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET

from plan import DEFAULT_IMAGES, build_plan
import prerequisites

HERE = Path(__file__).resolve().parent


def host_ready():
    try:
        if Path('/sys/module/apparmor/parameters/enabled').read_text().strip() != 'Y':
            return False
        if not any(Path('/etc/apparmor.d', name).is_file() for name in
                   ('usr.lib.libvirt.virt-aa-helper', 'usr.libexec.virt-aa-helper')):
            return False
        if grp.getgrnam('libvirt').gr_gid not in os.getgroups():
            return False
        result = subprocess.run(['virsh', '-c', 'qemu:///system', 'capabilities'], capture_output=True, text=True)
        if result.returncode or 'apparmor' not in [node.text for node in ET.fromstring(result.stdout).findall('./host/secmodel/model')]:
            return False
        # Build/install the matching new CLI once on older hosts.
        result = subprocess.run(['/usr/bin/forge', 'prepare', '--help'], capture_output=True, text=True)
        return result.returncode == 0 and '--images-only' in result.stdout
    except (OSError, KeyError, ET.ParseError):
        return False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--network', choices=['standard', 'extended', 'dedicated'])
    parser.add_argument('--music', choices=['with', 'without'])
    parser.add_argument('--dongle-b')
    args = parser.parse_args()
    if os.geteuid() == 0:
        raise ValueError('Uruchom jako użytkownik pulpitu, bez sudo.')
    subprocess.run([sys.executable, str(HERE / 'prerequisites.py')], check=True)
    data = Path(os.environ.get('XDG_DATA_HOME') or Path.home() / '.local/share') / 'forge'
    state = data / 'pending-install.json'
    options = json.loads(state.read_text()) if state.is_file() else {}
    options['network'] = args.network or options.get('network') or input('Wariant [standard/extended]: ').strip() or 'standard'
    options['images'] = DEFAULT_IMAGES.copy()
    options['music'] = args.music or options.get('music') or 'without'
    if args.dongle_b:
        options['dongle_b'] = args.dongle_b
    if options['network'] in ('dedicated', 'extended') and not options.get('dongle_b'):
        options['dongle_b'] = input('Adapter USB B (vvvv:pppp): ').strip()
    plan = build_plan(options)
    print(json.dumps(plan, ensure_ascii=False, indent=2), flush=True)
    if input('Zainstalować ten wariant i wszystkie obrazy? [tak/N]: ').strip().lower() != 'tak':
        return 0
    data.mkdir(parents=True, exist_ok=True)
    temporary = state.with_suffix('.tmp')
    temporary.write_text(json.dumps(options, indent=2) + '\n')
    temporary.chmod(0o600)
    temporary.replace(state)
    if not host_ready():
        subprocess.run(['bash', str(HERE / '02-host.sh')], check=True)
        print('Przygotowanie hosta zakończone. Uruchom ponownie komputer i powtórz instalator z --stage install. Wybór wariantu zapisany; obrazy i hooki zostaną dokończone po restarcie.')
        return 75
    command = [sys.executable, str(HERE / 'lab.py'), '--network', options['network'],
               '--music', options['music'], '--images', ','.join(DEFAULT_IMAGES), '--yes']
    if plan['network'] == 'dedicated':
        command += ['--dongle-b', options['dongle_b']]
    subprocess.run(command, check=True)
    state.unlink(missing_ok=True)
    print('Instalacja zakończona. Wszystkie cztery VM pozostają wyłączone. Test fizycznej izolacji gości wykonaj przed użyciem labu.')
    return 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error
