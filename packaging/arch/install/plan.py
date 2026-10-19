"""Pure installer plan: usable offline, without libvirt, hardware or privilege."""
import re

DEFAULT_IMAGES = ['tsurugi', 'kali', 'whonix']
MODES = {'standard': 'standard', 'extended': 'dedicated', 'dedicated': 'dedicated'}
VM_NAMES = {'tsurugi': ['tsurugi'], 'kali': ['kali'],
            'whonix': ['whonix-gateway', 'whonix-workstation']}
HOOKS = {
    'standard': 'Wspólny hook Forge: kontrola ról i dysków; Kali przez NAT hosta; Whonix przez Gateway.',
    'dedicated': 'Wspólny hook Forge w trybie A/B: host przez PCI A, goście przez USB B; bez B brak wyjścia; bez przejścia na A.',
}


def validate_options(options):
    if options.get('network') not in MODES:
        raise ValueError('Wybierz standard albo extended (Dedicated A/B).')
    if options.get('music') not in ('with', 'without'):
        raise ValueError('Muzyka: with albo without.')
    images = options.get('images', DEFAULT_IMAGES)
    if not isinstance(images, list) or not images or any(not isinstance(p, str) or p not in VM_NAMES for p in images) or len(images) != len(set(images)):
        raise ValueError('Wybierz niepowtarzające się obrazy: tsurugi,kali,whonix. Whonix jest nierozdzielną parą.')
    if MODES[options['network']] == 'dedicated' and not re.fullmatch(r'[0-9a-fA-F]{4}:[0-9a-fA-F]{4}', options.get('dongle_b') or ''):
        raise ValueError('Tryb A/B wymaga jawnego identyfikatora USB B: vvvv:pppp.')


def build_plan(options):
    validate_options(options)
    images = options.get('images', DEFAULT_IMAGES)
    mode = MODES[options['network']]
    prepare = ['/usr/bin/forge', 'prepare', '--profiles', ','.join(images)]
    if options['music'] == 'without':
        prepare.append('--no-music')
    commands = [prepare + ['--images-only'], ['/usr/bin/forge', 'setup', '--network', mode]]
    if MODES[options['network']] == 'dedicated':
        commands.append(['/usr/bin/forge', 'dev', 'cables'])
    commands.append(prepare)
    return {
        'steps': ['prerequisites', 'wariant', 'pobranie i weryfikacja wszystkich obrazów', 'hooki wariantu', 'maszyny i kontrola'],
        'variant': 'extended' if mode == 'dedicated' else 'standard',
        'network': mode, 'images': images,
        'vms': [name for p in images for name in VM_NAMES[p]],
        'hook_policy': HOOKS[mode],
        'dongle_b': options.get('dongle_b', '').lower() if MODES[options['network']] == 'dedicated' else None,
        'commands': commands,
        'requires_sudo_on_apply': True,
        'excluded_images': {'sift': 'Ręczny import OVA SANS; brak automatycznego pobierania.'},
        'note': 'Najpierw wszystkie bazy i ich weryfikacja; potem hook i sieć wariantu, na końcu VM. Istniejące VM i obrazy pozostają zachowane.',
    }
