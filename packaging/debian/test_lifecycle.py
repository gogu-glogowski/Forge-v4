#!/usr/bin/env python3
"""Real dpkg lifecycle test in a fresh isolated root; never use / as the root."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True, type=Path)
    parser.add_argument('packages', nargs=1, type=Path)
    args = parser.parse_args()
    if os.geteuid() != 0:
        raise SystemExit('Run under fakeroot (recommended) or sudo; dpkg only operates inside --root.')
    requested = args.root.absolute()
    root = requested.resolve()
    if root != requested or root.exists() or root == Path('/') or len(root.parts) < 4:
        raise SystemExit('Root must be a fresh absolute non-symlink directory below a working/test directory.')
    packages = [path.resolve(strict=True) for path in args.packages]
    for package in packages:
        metadata = subprocess.check_output(['dpkg-deb', '--field', str(package), 'Package'], text=True).strip()
        if metadata != 'forge':
            raise SystemExit('Only Forge packages may be tested.')
        control = subprocess.check_output(['dpkg-deb', '--ctrl-tarfile', str(package)])
        import io, tarfile
        with tarfile.open(fileobj=io.BytesIO(control)) as archive:
            if any(Path(entry.name).name in ['preinst', 'postinst', 'prerm', 'postrm', 'triggers'] for entry in archive):
                raise SystemExit('This isolated test is designed for script-free Forge packages.')
    root.mkdir(mode=0o755)
    admin = root / 'var/lib/dpkg'
    admin.mkdir(parents=True)
    shutil.copyfile('/var/lib/dpkg/status', admin / 'status')
    # Only file lists are copied; never copy executable maintainer scripts or triggers.
    info = admin / 'info'
    info.mkdir()
    for listing in Path('/var/lib/dpkg/info').glob('*.list'):
        shutil.copyfile(listing, info / listing.name)
    sentinels = [root / name for name in [
        'var/lib/forge/bases/keep.qcow2', 'var/lib/forge/vms/keep.qcow2',
        'var/lib/forge/network-mode', 'home/test/.local/share/forge/vms/keep.json',
        'etc/libvirt/hooks/qemu', 'etc/libvirt/qemu/keep.xml',
        'etc/apparmor.d/local/usr.lib.libvirt.virt-aa-helper']]
    for path in sentinels:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('retained lab fixture\n')
    before = {p: hashlib.sha256(p.read_bytes()).hexdigest() for p in sentinels}

    def preserved():
        for path, digest in before.items():
            assert path.is_file() and hashlib.sha256(path.read_bytes()).hexdigest() == digest, path

    def dpkg(*options):
        result = subprocess.run(['dpkg', '--root=' + str(root), '--log=' + str(root / 'dpkg.log'), *map(str, options)], text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        with (root / 'lifecycle-output.log').open('a') as log:
            log.write(result.stdout)
        if result.returncode:
            raise RuntimeError('dpkg failed:\n' + '\n'.join(result.stdout.splitlines()[-35:]))
        print('PASS dpkg', options[0])
        preserved()

    def version(package):
        return subprocess.check_output(['dpkg-query', '--admindir=' + str(admin), '-W', '-f=${Version}', package], text=True).strip()

    with tempfile.TemporaryDirectory(prefix='forge-upgrade-fixture-') as temporary:
        previous = []
        for number, package in enumerate(packages):
            tree = Path(temporary) / str(number)
            subprocess.run(['dpkg-deb', '--raw-extract', str(package), str(tree)], check=True)
            path = tree / 'DEBIAN/control'
            lines = path.read_text().splitlines()
            lines = ['Version: 4.2.0~rc1-1' if line.startswith('Version:') else line for line in lines]
            # Both payloads represent the predecessor fixture, including its recommendation.
            path.write_text('\n'.join(line.replace('forge-music (>= 4.2.0-1)', 'forge-music (>= 4.2.0~rc1-1)') for line in lines) + '\n')
            fixture = Path(temporary) / f'previous-{number}.deb'
            subprocess.run(['dpkg-deb', '--root-owner-group', '-Zxz', '-z1', '--build', str(tree), str(fixture)], check=True)
            previous.append(fixture)
        dpkg('--install', *previous)
        assert version('forge') == '4.2.0~rc1-1'
        dpkg('--install', *packages)
        assert version('forge') == '4.2.0-1'
        binary = root / 'usr/bin/forge'
        uid, gid = int(os.environ.get('SUDO_UID', '1000')), int(os.environ.get('SUDO_GID', '1000'))
        def user():
            os.setgroups([])
            os.setgid(gid)
            os.setuid(uid)
        result = subprocess.check_output([str(binary), '--version'], text=True, preexec_fn=None if os.environ.get('FAKEROOTKEY') else user)
        assert result.strip() == 'forge 4.2.0', result
        help_text = subprocess.check_output([str(binary), 'prepare', '--help'], text=True, preexec_fn=None if os.environ.get('FAKEROOTKEY') else user)
        assert '--all' in help_text and '--no-music' in help_text
        assert hashlib.sha256(binary.read_bytes()).digest() == hashlib.sha256(Path(__file__).resolve().parents[2].joinpath('target/release/forge').read_bytes()).digest()
        library = root / 'usr/share/forge/music'
        assert len(list(library.glob('*.mp3'))) == 0
        for name in ['CREDITS.md', 'LICENSE-CC-BY-4.0.txt', 'manifest.json']:
            assert (library / name).is_file()
        dpkg('--install', *packages)
        dpkg('--remove', 'forge')
        assert not binary.exists()
        assert not library.exists()
        dpkg('--install', *packages)
        dpkg('--purge', 'forge')
        assert not binary.exists() and not library.exists()
    print('PASS: install, RC-fixture upgrade, reinstall, remove, reinstall, purge; retained lab files unchanged.')
    print('Isolated dpkg log:', root / 'dpkg.log')

if __name__ == '__main__':
    main()
