import contextlib
import io
import itertools
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab
from plan import build_plan, DEFAULT_IMAGES


class PlanTests(unittest.TestCase):
    def test_all_28_mode_image_music_combinations(self):
        count = 0
        for size in range(1, 4):
            for images in itertools.combinations(DEFAULT_IMAGES, size):
                for network, music in itertools.product(['standard', 'dedicated'], ['with', 'without']):
                    options = dict(network=network, music=music, images=list(images), dongle_b='0B95:1790')
                    plan = build_plan(options)
                    self.assertEqual(plan['commands'][1][-1], network)
                    self.assertIn('--images-only', plan['commands'][0])
                    self.assertNotIn('--images-only', plan['commands'][-1])
                    self.assertEqual(plan['commands'][-1][3], ','.join(images))
                    self.assertEqual('--no-music' in plan['commands'][-1], music == 'without')
                    self.assertEqual(['whonix-gateway', 'whonix-workstation'] == [n for n in plan['vms'] if n.startswith('whonix')], 'whonix' in images)
                    self.assertEqual(any('cables' in c for c in plan['commands']), network == 'dedicated')
                    count += 1
        self.assertEqual(count, 28)

    def test_extended_is_the_same_hook_policy_as_dedicated(self):
        options = dict(music='without', dongle_b='0b95:1790')
        extended = build_plan(dict(network='extended', **options))
        dedicated = build_plan(dict(network='dedicated', **options))
        self.assertEqual(extended, dedicated)
        self.assertEqual(extended['images'], DEFAULT_IMAGES)
        self.assertEqual(len(extended['vms']), 4)
        self.assertEqual(extended['variant'], 'extended')

    def test_invalid_selections_and_usb_ids_are_rejected(self):
        for images in [[], ['kali', 'kali'], ['unknown'], ['whonix-workstation'], 'kali', [None]]:
            with self.subTest(images=images), self.assertRaises(ValueError):
                build_plan(dict(network='standard', music='without', images=images))
        for dongle in [None, '', '0b95', '0b95:17900', '0b95:1790;id']:
            with self.subTest(dongle=dongle), self.assertRaises(ValueError):
                build_plan(dict(network='dedicated', music='without', dongle_b=dongle))

    def test_offline_plan_never_runs_commands_or_writes_state(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {'XDG_DATA_HOME': tmp}):
            with patch.object(sys, 'argv', ['lab.py', '--plan', '--network', 'dedicated', '--dongle-b', '0b95:1790', '--images', 'whonix']):
                with patch.object(lab, 'output', side_effect=AssertionError('external read')), patch.object(lab, 'run', side_effect=AssertionError('mutation')):
                    with contextlib.redirect_stdout(io.StringIO()) as captured:
                        lab.main()
            plan = json.loads(captured.getvalue())
            self.assertEqual(plan['vms'], ['whonix-gateway', 'whonix-workstation'])
            self.assertEqual(list(Path(tmp).iterdir()), [])

    def test_cancel_before_privileged_checks_does_not_write_state(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {'XDG_DATA_HOME': tmp}):
            with patch.object(sys, 'argv', ['lab.py', '--network', 'standard', '--images', 'kali', '--music', 'without']), patch('builtins.input', return_value='nie'):
                with patch.object(lab, 'output', side_effect=AssertionError('external read')), patch.object(lab, 'run', side_effect=AssertionError('mutation')):
                    with contextlib.redirect_stdout(io.StringIO()):
                        lab.main()
            self.assertEqual(list(Path(tmp).iterdir()), [])

    def test_dedicated_hardware_and_host_route_guards(self):
        real_path = Path
        with tempfile.TemporaryDirectory() as tmp:
            root = real_path(tmp)
            net = root / 'class/net'
            net.mkdir(parents=True)
            pci = root / 'devices/pci0000:00/0000:00:01.0'
            pci.mkdir(parents=True)
            usb = pci / 'usb1/1-1'
            usb.mkdir(parents=True)
            (usb / 'idVendor').write_text('0b95')
            (usb / 'idProduct').write_text('1790')
            for name, device in [('enp1s0', pci), ('enxB', usb)]:
                (net / name).mkdir()
                (net / name / 'device').symlink_to(device)
            def mapped(value):
                return net if str(value) == '/sys/class/net' else real_path(value)
            with patch.object(lab, 'Path', side_effect=mapped), patch.object(lab, 'output') as output:
                ipv6_routes = []
                output.side_effect = lambda *args: json.dumps(ipv6_routes) if '-6' in args else output.return_value
                output.return_value = '[{"dev":"enp1s0"}]'
                lab.dedicated_preflight('0B95:1790')
                with self.assertRaises(ValueError):
                    lab.dedicated_preflight('1234:5678')
                for route in ['[]', '[{"dev":"enxB"}]', '[{"dev":"missing"}]']:
                    output.return_value = route
                    with self.subTest(route=route), self.assertRaises(ValueError):
                        lab.dedicated_preflight('0b95:1790')
                output.return_value = '[{"dev":"enp1s0"}]'
                for ipv6_routes in [[{'dev': 'enp1s0'}], [{'dev': 'enxB'}], [{'dev': 'missing'}]]:
                    if ipv6_routes[0]['dev'] == 'enp1s0':
                        lab.dedicated_preflight('0b95:1790')
                    else:
                        with self.subTest(ipv6=ipv6_routes), self.assertRaises(ValueError):
                            lab.dedicated_preflight('0b95:1790')
                ipv6_routes = []
                (net / 'enp1s0/wireless').mkdir()
                with self.assertRaises(ValueError):
                    lab.dedicated_preflight('0b95:1790')
                (net / 'enp1s0/wireless').rmdir()
                (net / 'enxC').mkdir()
                (net / 'enxC/device').symlink_to(usb)
                with self.assertRaises(ValueError):
                    lab.dedicated_preflight('0b95:1790')

    def test_explicit_plan_is_offline(self):
        root = Path(__file__).resolve().parents[3]
        result = subprocess.run(['bash', str(root / 'install.sh'), '--stage', 'plan'], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['network'], 'standard')
        result = subprocess.run(['bash', str(root / 'install.sh'), '--stage', 'invalid'], capture_output=True)
        self.assertNotEqual(result.returncode, 0)

    def test_cli_invalid_plan_fails_without_traceback(self):
        result = subprocess.run([sys.executable, str(Path(lab.__file__)), '--plan', '--images', 'kali,kali'], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('Traceback', result.stderr)


if __name__ == '__main__':
    unittest.main()
