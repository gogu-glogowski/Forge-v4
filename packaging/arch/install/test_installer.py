import importlib.util
from pathlib import Path
import tempfile
import shutil
import subprocess
import unittest
import sys
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

def module(name):
    spec = importlib.util.spec_from_file_location(name, HERE / f'{name}.py')
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value

host = module('host')
lab = module('lab')

class InstallerTests(unittest.TestCase):
    def test_boot_preserves_root_and_is_idempotent(self):
        original = 'ESP_PATH="/boot"\nKERNEL_CMDLINE[default]+="quiet root=UUID=example rootflags=subvol=/@"\n'
        changed = host.updated_boot(original)
        self.assertTrue(changed.startswith(original))
        self.assertEqual(changed.count('lsm='), 1)
        self.assertEqual(host.updated_boot(changed), changed)

    def test_existing_lsm_is_never_overwritten(self):
        for existing in ('lsm=selinux', 'apparmor=0'):
            with self.assertRaises(ValueError):
                host.updated_boot(f'KERNEL_CMDLINE[default]+="{existing}"\n')

    def test_unknown_boot_layout_is_rejected(self):
        with self.assertRaises(ValueError):
            host.updated_boot('GRUB_CMDLINE_LINUX_DEFAULT="quiet"\n')

    def test_recipe_keeps_upstream_checks_and_pins(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / 'PKGBUILD'
            host.recipe(target)
            text = target.read_text()
            for option in ('apparmor', 'apparmor_profiles', 'secdriver_apparmor'):
                self.assertIn(f'-D{option}=enabled', text)
            self.assertIn('meson test -C build --print-errorlogs', text)
            self.assertIn("sha256sums=('66c9dc02", text)
            self.assertIn('pkgrel=1.2', text)
            if shutil.which('makepkg'):
                metadata = subprocess.check_output(['makepkg', '--printsrcinfo'], cwd=tmp, text=True)
                self.assertIn('pkgrel = 1.2', metadata)


    def test_source_release_can_build_without_git_metadata(self):
        spec = importlib.util.spec_from_file_location('arch_builder', HERE.parent / 'build.py')
        builder = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(builder)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'Cargo.toml').write_text('source')
            (root / 'dist').mkdir()
            (root / 'dist/old-package').write_text('exclude')
            (root / 'target').mkdir()
            (root / 'target/binary').write_text('exclude')
            (root / 'external-link').symlink_to('/etc/passwd')
            with patch.object(builder, 'REPO', root):
                self.assertEqual(builder.source_files(), ['Cargo.toml'])

    def test_package_filename_uses_recipe_release(self):
        spec = importlib.util.spec_from_file_location('arch_builder_release', HERE.parent / 'build.py')
        builder = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(builder)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'Cargo.toml').write_text('[workspace.package]\nversion="4.2.2"\n')
            recipe = root / 'packaging/arch/PKGBUILD.in'
            recipe.parent.mkdir(parents=True)
            with patch.object(builder, 'REPO', root):
                for release in ('5', '6.1'):
                    recipe.write_text(f'pkgrel={release}\n')
                    self.assertEqual(builder.package_filename(), f'forge-lab-4.2.2-{release}-x86_64.pkg.tar.zst')

    def test_lab_package_does_not_claim_arrayfire_identity(self):
        template = (HERE.parent / 'PKGBUILD.in').read_text()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'PKGBUILD').write_text(template.replace('@VERSION@', '4.2.0').replace('@SOURCE_SHA256@', '0' * 64))
            metadata = subprocess.check_output(['makepkg', '--printsrcinfo'], cwd=root, text=True)
        self.assertIn('pkgname = forge-lab', metadata)
        for field in ('provides', 'conflicts', 'replaces'):
            self.assertNotIn(f'{field} = forge\n', metadata)
        self.assertIn('/usr/bin/forge', template)

    def test_package_without_profiles_is_rejected(self):
        info = 'pkgname = libvirt\npkgver = 1:12.8.0-1.2\ndepend = apparmor\n'
        with patch.object(host.subprocess, 'check_output', side_effect=[info, 'usr/bin/virsh\n']):
            with self.assertRaises(ValueError):
                host.inspect(Path('libvirt.pkg.tar.zst'))

    def test_kernel_enabled_alone_does_not_pass_capabilities(self):
        with self.assertRaises(ValueError):
            lab.verify_capabilities('<capabilities><host><secmodel><model>dac</model></secmodel></host></capabilities>')
        lab.verify_capabilities('<capabilities><host><secmodel><model>apparmor</model></secmodel></host></capabilities>')

    def test_dedicated_requires_explicit_usb_selection(self):
        with self.assertRaises(ValueError):
            lab.validate_options({'network': 'dedicated', 'music': 'without'})
        lab.validate_options({'network': 'dedicated', 'music': 'without', 'dongle_b': '0b95:1790'})
        lab.validate_options({'network': 'standard', 'music': 'with'})

    def test_missing_or_invalid_vm_fails_final_check(self):
        good = '\n'.join(f'{name} profile shutoff role=ok b=off' for name in lab.EXPECTED)
        lab.verify_inventory(good)
        with self.assertRaises(ValueError):
            lab.verify_inventory(good.replace('tsurugi', 'missing'))
        with self.assertRaises(ValueError):
            lab.verify_inventory(good.replace('role=ok', 'ROLE FAIL', 1))

if __name__ == '__main__':
    unittest.main()
