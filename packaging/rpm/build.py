#!/usr/bin/env python3
"""Build an unsigned Fedora-targeted binary RPM without modifying the host."""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

REPO = Path(__file__).resolve().parents[2]

def copy(source, target, executable=False):
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)
    target.chmod(0o755 if executable else 0o644)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=REPO / 'dist/rpm')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--target', choices=['fedora', 'el10'], default='fedora')
    args = parser.parse_args()
    version = tomllib.loads((REPO / 'Cargo.toml').read_text())['workspace']['package']['version']
    if not args.skip_build:
        subprocess.run(['cargo', 'build', '--release', '--locked', '-p', 'forge-cli'], cwd=REPO, check=True)
    binary = REPO / 'target/release/forge'
    assert subprocess.check_output([str(binary), '--version'], text=True).strip() == f'forge {version}'
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='forge-rpm-') as temporary:
        top = Path(temporary)
        for name in ['SOURCES', 'SPECS', 'BUILD', 'BUILDROOT', 'RPMS', 'SRPMS', 'tmp', 'rpmdb']:
            (top / name).mkdir()
        payload = top / f'forge-{version}-payload'
        copy(binary, payload / 'usr/bin/forge', True)
        for name in ['README.md', 'CHANGELOG.md']:
            copy(REPO / name, payload / 'usr/share/doc/forge' / name)
        copy(REPO / 'packaging/rpm/README.md', payload / 'usr/share/doc/forge/README.RPM.md')
        copy(REPO / 'LICENSE', payload / 'usr/share/licenses/forge/LICENSE')
        for name in ['CREDITS.md', 'LICENSE-CC-BY-4.0.txt', 'manifest.json']:
            copy(REPO / 'assets/music' / name, payload / 'usr/share/forge/music' / name)
        with tarfile.open(top / f'SOURCES/forge-{version}-payload.tar.xz', 'w:xz') as archive:
            archive.add(payload, arcname=payload.name)
        spec = (REPO / 'packaging/rpm/forge.spec.in').read_text().replace('@VERSION@', version)
        if args.target == 'el10':
            spec = spec.replace('Release:        1%{?dist}', 'Release:        5.el10').replace('Requires:       fedora-release >= 44', 'Requires:       (almalinux-release >= 10 or rocky-release >= 10)')
        path = top / 'SPECS/forge.spec'
        path.write_text(spec)
        subprocess.run(['rpmbuild', '-bb', '--define', f'_topdir {top}', '--define', f'_dbpath {top}/rpmdb', '--define', f'_tmppath {top}/tmp', str(path)], check=True)
        packages = list((top / 'RPMS/x86_64').glob('forge-*.rpm'))
        assert len(packages) == 1
        target = output / packages[0].name
        shutil.copyfile(packages[0], target)
        (output / 'forge.spec').write_text(spec)
        (output / 'SHA256SUMS').write_text(f'{hashlib.sha256(target.read_bytes()).hexdigest()}  {target.name}\n')
        print('RPM:', target)

if __name__ == '__main__':
    main()
