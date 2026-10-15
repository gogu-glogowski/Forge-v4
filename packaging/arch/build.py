#!/usr/bin/env python3
"""Generate PKGBUILD/source archive and build with makepkg, without installing."""
import argparse
import hashlib
from pathlib import Path
import os
import shutil
import shlex
import subprocess
import tarfile
import tempfile
import tomllib

REPO = Path(__file__).resolve().parents[2]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=REPO / 'dist/arch')
    parser.add_argument('--cross-host', action='store_true', help='skip dependency checks for packaging-only tests on a non-Arch host')
    parser.add_argument('--config', type=Path)
    args = parser.parse_args()
    version = tomllib.loads((REPO / 'Cargo.toml').read_text())['workspace']['package']['version']
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    source = output / f'forge-{version}-source.tar.gz'
    tracked = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=REPO).decode().rstrip('\0').split('\0')
    with tarfile.open(source, 'w:gz') as archive:
        for name in sorted(set(tracked)):
            path = REPO / name
            if path.is_file() and not name.endswith('.mp3') and not name.startswith('packages/'):
                archive.add(path, arcname=f'forge-{version}/{name}')
    template = (REPO / 'packaging/arch/PKGBUILD.in').read_text()
    recipe = template.replace('@VERSION@', version).replace('@SOURCE_SHA256@', hashlib.sha256(source.read_bytes()).hexdigest())
    (output / 'PKGBUILD').write_text(recipe)
    with tempfile.TemporaryDirectory(prefix='forge-makepkg-') as temporary:
        work = Path(temporary)
        shutil.copyfile(source, work / source.name)
        (work / 'PKGBUILD').write_text(recipe)
        env = os.environ.copy()
        env['PKGDEST'] = str(output)
        command = ['makepkg', '--noconfirm', '--force']
        if args.cross_host:
            command.append('--nodeps')
            # Empty isolated package DB for .BUILDINFO; never query the host as Arch.
            database = work / 'pacmandb'
            database.joinpath('local').mkdir(parents=True)
            config = work / 'pacman.conf'
            config.write_text(f'[options]\nArchitecture = auto\nDBPath = {database}\nLogFile = {work}/pacman.log\n')
            pacman = shutil.which('pacman')
            if not pacman:
                raise SystemExit('pacman is required to generate cross-host build metadata')
            wrapper = work / 'pacman-query'
            wrapper.write_text('#!/bin/sh\nexec ' + shlex.quote(pacman) + ' --config ' + shlex.quote(str(config)) + ' "$@"\n')
            wrapper.chmod(0o755)
            env['PACMAN'] = str(wrapper)
        if args.config:
            command.extend(['--config', str(args.config.resolve())])
        subprocess.run(command, cwd=work, env=env, check=True)
    package = output / f'forge-{version}-1-x86_64.pkg.tar.zst'
    assert package.is_file(), 'makepkg config must set PKGEXT=.pkg.tar.zst'
    (output / 'SHA256SUMS').write_text(''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n' for p in [package, source, output / 'PKGBUILD']))
    print('Arch package:', package)

if __name__ == '__main__':
    main()
