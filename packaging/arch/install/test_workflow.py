import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import importlib.util
import shutil
import tarfile
import unittest
from unittest.mock import patch, MagicMock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import prerequisites
import workflow
import lab
from plan import DEFAULT_IMAGES


class WorkflowTests(unittest.TestCase):
    def run_workflow(self, directory, ready=True, failure=False):
        calls = []
        def command(args, **kwargs):
            calls.append(args)
            if failure and str(args[1]).endswith('lab.py'):
                raise subprocess.CalledProcessError(1, args)
            return subprocess.CompletedProcess(args, 0)
        with patch.dict(os.environ, {'XDG_DATA_HOME': directory}), patch.object(sys, 'argv', ['workflow.py', '--network', 'extended', '--dongle-b', '0b95:1790']):
            with patch.object(workflow.os, 'geteuid', return_value=1000), patch.object(workflow, 'host_ready', return_value=ready), patch.object(workflow.subprocess, 'run', side_effect=command), patch('builtins.input', return_value='tak'), contextlib.redirect_stdout(io.StringIO()):
                result = workflow.main()
        return result, calls

    def test_full_install_checks_dependencies_then_uses_all_images(self):
        with tempfile.TemporaryDirectory() as tmp:
            result, calls = self.run_workflow(tmp)
            self.assertEqual(result, 0)
            self.assertTrue(str(calls[0][1]).endswith('prerequisites.py'))
            self.assertTrue(str(calls[1][1]).endswith('lab.py'))
            self.assertEqual(calls[1][calls[1].index('--images') + 1], ','.join(DEFAULT_IMAGES))
            self.assertEqual(calls[1][calls[1].index('--network') + 1], 'extended')
            self.assertFalse((Path(tmp) / 'forge/pending-install.json').exists())

    def test_restart_boundary_retains_variant_without_starting_lab(self):
        with tempfile.TemporaryDirectory() as tmp:
            result, calls = self.run_workflow(tmp, ready=False)
            self.assertEqual(result, 75)
            self.assertTrue(str(calls[-1][-1]).endswith('02-host.sh'))
            state = json.loads((Path(tmp) / 'forge/pending-install.json').read_text())
            self.assertEqual(state['network'], 'extended')
            self.assertEqual(state['images'], DEFAULT_IMAGES)
            self.assertFalse(any(str(arg).endswith('lab.py') for cmd in calls for arg in cmd))
            result, calls = self.run_workflow(tmp)
            self.assertEqual(result, 0)
            self.assertFalse((Path(tmp) / 'forge/pending-install.json').exists())

    def test_failure_does_not_mark_installation_complete(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(subprocess.CalledProcessError):
                self.run_workflow(tmp, failure=True)
            self.assertTrue((Path(tmp) / 'forge/pending-install.json').is_file())

    def test_prerequisites_include_every_runtime_and_build_dependency(self):
        packages = prerequisites.requirements()
        for name in ['python', 'gnupg', 'curl', 'networkmanager', 'iproute2', 'edk2-ovmf', 'devtools', 'sudo', 'rust>=1.89', 'cargo']:
            self.assertIn(name, packages)
        self.assertEqual(len(packages), len(set(packages)))

    def test_missing_package_query_rejects_broken_database(self):
        for code, text in [(0, ''), (127, 'curl\n')]:
            result = subprocess.CompletedProcess([], code, stdout=text, stderr='')
            with patch.object(prerequisites.subprocess, 'run', return_value=result):
                self.assertEqual(prerequisites.missing_packages(['curl']), text.splitlines())
        with patch.object(prerequisites.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1, stdout='', stderr='database error')):
            with self.assertRaises(ValueError):
                prerequisites.missing_packages(['curl'])

    def test_prerequisites_check_mode_never_installs_packages(self):
        with patch.object(sys, 'argv', ['prerequisites.py', '--check']), patch.object(prerequisites, 'check_platform'), patch.object(prerequisites, 'missing_packages', return_value=['curl']), patch.object(prerequisites.subprocess, 'run', side_effect=AssertionError('mutation')), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(prerequisites.main(), 1)

    def test_prerequisites_install_and_verify_missing_packages(self):
        calls = []
        with patch.object(sys, 'argv', ['prerequisites.py']), patch.object(prerequisites, 'check_platform'), patch.object(prerequisites.os, 'geteuid', return_value=1000), patch.object(prerequisites, 'missing_packages', side_effect=[['curl'], []]), patch.object(prerequisites.shutil, 'which', return_value='/usr/bin/tool'), patch.object(prerequisites.subprocess, 'run', side_effect=lambda args, **kwargs: calls.append(args)), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(prerequisites.main(), 0)
        self.assertEqual(calls[0][:4], ['sudo', 'pacman', '-Syu', '--needed'])
        self.assertEqual(calls[0][4:], ['curl'])
        self.assertNotIn('libvirt', calls[0])
        self.assertNotIn('--noconfirm', calls[0])

    def test_fully_satisfied_dependencies_do_not_force_libvirt_downgrade(self):
        calls = []
        with patch.object(sys, 'argv', ['prerequisites.py']), patch.object(prerequisites, 'check_platform'), patch.object(prerequisites.os, 'geteuid', return_value=1000), patch.object(prerequisites, 'missing_packages', return_value=[]), patch.object(prerequisites.shutil, 'which', return_value='/usr/bin/tool'), patch.object(prerequisites.subprocess, 'run', side_effect=lambda args, **kwargs: calls.append(args)), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(prerequisites.main(), 0)
        self.assertEqual(calls, [['sudo', 'pacman', '-Syu', '--needed']])

    def test_host_readiness_requires_profile_on_disk_and_current_capabilities(self):
        for profile, model, expected in [(True, 'apparmor', True), (False, 'apparmor', False), (True, 'dac', False)]:
            with self.subTest(profile=profile, model=model):
                path = MagicMock()
                path.read_text.return_value = 'Y'
                path.is_file.return_value = profile
                capabilities = f'<capabilities><host><secmodel><model>{model}</model></secmodel></host></capabilities>'
                def command(args, **kwargs):
                    return subprocess.CompletedProcess(args, 0, stdout=capabilities if args[-1] == 'capabilities' else '--images-only')
                with patch.object(workflow, 'Path', return_value=path), patch.object(workflow.grp, 'getgrnam', return_value=MagicMock(gr_gid=948)), patch.object(workflow.os, 'getgroups', return_value=[948]), patch.object(workflow.subprocess, 'run', side_effect=command):
                    self.assertEqual(workflow.host_ready(), expected)

    def test_final_inventory_refuses_active_vm(self):
        with self.assertRaises(ValueError):
            lab.verify_inventory('kali kali running role=ok b=off', ['kali'])

    def lab_stage(self, directory, fail_download=False, fail_second_preflight=False):
        real_path = Path
        root = real_path(directory)
        forge = root / 'forge-cli'
        forge.write_text('fixture')
        apparmor = root / 'apparmor-enabled'
        apparmor.write_text('Y')
        data = root / 'forge'
        data.mkdir(exist_ok=True)
        (data / 'installer-options.json').write_text(json.dumps(dict(network='standard', music='without', images=['kali'])))
        def mapped(value):
            return apparmor if str(value) == '/sys/module/apparmor/parameters/enabled' else real_path(value)
        commands = []
        def run(*args):
            commands.append(list(args))
            if fail_download and '--images-only' in args:
                raise subprocess.CalledProcessError(1, args)
        def output(*args):
            if args == ('id', '-nG'):
                return 'major libvirt kvm'
            if args[0] == 'sudo':
                return json.dumps({'profiles': {'/usr/lib/libvirt/virt-aa-helper': 'enforce'}})
            if args[-1] == 'capabilities':
                return '<capabilities><host><secmodel><model>apparmor</model></secmodel></host></capabilities>'
            if args[-1] == '--name':
                return ''
            if args[-1] == '--help':
                return '--profiles --images-only'
            if args[-1] == 'status':
                return '\n'.join(f'{name} profile shutoff role=ok b=off' for name in lab.EXPECTED)
            raise AssertionError(args)
        preflights = []
        def preflight(dongle):
            preflights.append(dongle)
            if fail_second_preflight and len(preflights) == 2:
                raise ValueError('host route changed to B during download')
        with patch.dict(os.environ, {'XDG_DATA_HOME': directory}), patch.object(sys, 'argv', ['lab.py', '--network', 'extended', '--music', 'without', '--dongle-b', '0b95:1790', '--yes']), patch.object(lab, 'Path', side_effect=mapped), patch.object(lab, 'FORGE', str(forge)), patch.object(lab.os, 'geteuid', return_value=1000), patch.object(lab, 'output', side_effect=output), patch.object(lab, 'run', side_effect=run), patch.object(lab, 'dedicated_preflight', side_effect=preflight), contextlib.redirect_stdout(io.StringIO()):
            try:
                lab.main()
            except (ValueError, subprocess.CalledProcessError):
                if not (fail_download or fail_second_preflight):
                    raise
        return commands, preflights

    def test_lab_downloads_every_image_before_hook_setup(self):
        with tempfile.TemporaryDirectory() as tmp:
            commands, preflights = self.lab_stage(tmp)
            self.assertIn('--images-only', commands[0])
            self.assertEqual(commands[0][3], ','.join(DEFAULT_IMAGES))
            self.assertEqual(commands[1][1:], ['setup', '--network', 'dedicated'])
            self.assertEqual(commands[2][1:], ['dev', 'cables'])
            self.assertEqual(commands[-1][1:], ['dev', 'doctor'])
            self.assertEqual(len(preflights), 2)

    def test_interrupted_download_leaves_hooks_and_network_untouched(self):
        with tempfile.TemporaryDirectory() as tmp:
            commands, _ = self.lab_stage(tmp, fail_download=True)
            self.assertEqual(len(commands), 1)
            self.assertIn('--images-only', commands[0])
            self.assertFalse((Path(tmp) / 'forge/dongle-b').exists())

    def test_route_change_during_download_stops_before_hook_setup(self):
        with tempfile.TemporaryDirectory() as tmp:
            commands, preflights = self.lab_stage(tmp, fail_second_preflight=True)
            self.assertEqual(len(commands), 1)
            self.assertEqual(len(preflights), 2)
            self.assertFalse((Path(tmp) / 'forge/dongle-b').exists())

    def test_package_staging_installs_launcher_and_matching_source_archive(self):
        root = Path(__file__).resolve().parents[3]
        spec = importlib.util.spec_from_file_location('forge_builder_staging', root / 'packaging/arch/build.py')
        builder = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(builder)
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            source = work / 'forge-4.2.2'
            source.mkdir()
            names = [name for name in builder.source_files() if not name.endswith('.mp3') and not name.startswith('packages/')]
            for name in names:
                destination = source / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(root / name, destination)
            archive = work / 'forge-4.2.2-source.tar.gz'
            with tarfile.open(archive, 'w:gz') as tar:
                for name in names:
                    tar.add(source / name, arcname='forge-4.2.2/' + name)
            binary = source / 'target/release/forge'
            binary.parent.mkdir(parents=True)
            binary.write_text('#!/bin/sh\nexit 0\n')
            staging = work / 'pkg'
            subprocess.run(['bash', '-eu', '-c', 'source "$1"; pkgver=4.2.2; pkgdir="$2"; unset CARGO_TARGET_DIR; package', 'stage-test', str(source / 'packaging/arch/PKGBUILD.in'), str(staging)], cwd=work, check=True)
            launcher = staging / 'usr/bin/forge-install'
            self.assertTrue(os.access(launcher, os.X_OK))
            self.assertEqual((staging / 'usr/share/forge/installer-source.tar.gz').read_bytes(), archive.read_bytes())
            with tarfile.open(staging / 'usr/share/forge/installer-source.tar.gz') as tar:
                for name in ('install.sh', 'packaging/arch/install/workflow.py', 'packaging/arch/install/prerequisites.py', 'packaging/arch/install/plan.py'):
                    self.assertEqual(tar.extractfile('forge-4.2.2/' + name).read(), (root / name).read_bytes())

    def test_packaged_installer_includes_matching_sources_and_python(self):
        recipe = (Path(__file__).resolve().parent.parent / 'PKGBUILD.in').read_text()
        self.assertIn("'python'", recipe)
        self.assertIn('installer-source.tar.gz', recipe)
        self.assertIn('/usr/bin/forge-install', recipe)


if __name__ == '__main__':
    unittest.main()
