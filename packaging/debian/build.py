#!/usr/bin/env python3
"""Build local binary DEBs; never install packages or reconfigure the host."""
import argparse
import gzip
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

REPO = Path(__file__).resolve().parents[2]

def run(*args, cwd=REPO):
    return subprocess.check_output(args, cwd=cwd, text=True).strip()

def copy(source, destination, mode=0o644):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(mode)

def build_tree(tree, package, version, architecture, depends, recommends, description):
    control = tree / 'DEBIAN'
    control.mkdir()
    size = sum(p.stat().st_size for p in tree.rglob('*') if p.is_file())
    text = (f'Package: {package}\nVersion: {version}\nArchitecture: {architecture}\n'
            'Maintainer: Forge contributors <noreply@github.com>\n'
            'Section: misc\nPriority: optional\n'
            'Homepage: https://github.com/gogu-glogowski/Forge-v4\n'
            f'Installed-Size: {(size + 1023) // 1024}\n')
    if depends:
        text += f'Depends: {depends}\n'
    if recommends:
        text += f'Recommends: {recommends}\n'
    (control / 'control').write_text(text + f'Description: {description}\n')
    hashes = []
    for path in sorted(tree.rglob('*')):
        if path.is_file() and 'DEBIAN' not in path.relative_to(tree).parts:
            hashes.append(f'{hashlib.md5(path.read_bytes()).hexdigest()}  {path.relative_to(tree)}\n')
    (control / 'md5sums').write_text(''.join(hashes))
    for path in tree.rglob('*'):
        if path.is_dir():
            path.chmod(0o755)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=REPO / 'dist')
    parser.add_argument('--skip-build', action='store_true')
    args = parser.parse_args()
    version = tomllib.loads((REPO / 'Cargo.toml').read_text())['workspace']['package']['version']
    debversion = version + '-1'
    if not args.skip_build:
        subprocess.run(['cargo', 'build', '--release', '--locked', '-p', 'forge-cli'], cwd=REPO, check=True)
    binary = REPO / 'target/release/forge'
    if run(str(binary), '--version') != f'forge {version}':
        raise SystemExit('Binary version differs from Cargo; rebuild first (run as normal user).')
    arch = run('dpkg', '--print-architecture')
    if arch != 'amd64':
        raise SystemExit('This release supports desktop amd64 only.')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='forge-deb-') as temporary:
        work = Path(temporary)
        # Let Debian determine the minimum libc/libgcc ABI needed by this build.
        (work / 'debian').mkdir()
        (work / 'debian/control').write_text('Source: forge\nSection: misc\nPriority: optional\nMaintainer: Forge contributors <noreply@github.com>\n\nPackage: forge\nArchitecture: amd64\nDescription: Forge KVM lab\n')
        shlibs = run('dpkg-shlibdeps', '-O', '-e' + str(binary), cwd=work)
        shared = next(line.split('=', 1)[1] for line in shlibs.splitlines() if line.startswith('shlibs:Depends='))
        tree = work / 'forge'
        tree.mkdir()
        copy(binary, tree / 'usr/bin/forge', 0o755)
        doc = tree / 'usr/share/doc/forge'
        copy(REPO / 'README.md', doc / 'README.md')
        copy(REPO / 'packaging/debian/README.md', doc / 'README.Debian')
        copy(REPO / 'LICENSE', doc / 'copyright')
        doc.joinpath('changelog.gz').write_bytes(gzip.compress((REPO / 'CHANGELOG.md').read_bytes(), mtime=0))
        for name in ['CREDITS.md', 'LICENSE-CC-BY-4.0.txt', 'manifest.json']:
            copy(REPO / 'assets/music' / name, tree / 'usr/share/forge/music' / name)
        dependencies = shared + ', qemu-system-x86, qemu-utils, libvirt-clients, libvirt-daemon-system, gnome-boxes, apparmor, gnupg, curl, ca-certificates, 7zip | p7zip-full, tar, xz-utils, e2fsprogs, network-manager, iproute2, procps, psmisc, systemd, sudo, mpv'
        build_tree(tree, 'forge', debversion, arch, dependencies, None,
                   'Verified KVM lab with GNOME Boxes integration\n Immutable bases, writable overlays, libvirt hooks and Standard NAT.\n Dedicated A/B hardware support remains experimental. Run as a normal user.\n Setup is explicit; installation does not change networks or running guests.')
        target = output / f'forge_{debversion}_{arch}.deb'
        subprocess.run(['dpkg-deb', '--root-owner-group', '-Zxz', '-z1', '--build', str(tree), str(target)], check=True)
        artifacts = [target]
        (output / 'SHA256SUMS').write_text(''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n' for p in artifacts))
        print('Shared-library dependencies:', shared)
        print('Packages:', *(str(p) for p in artifacts), sep='\n')

if __name__ == '__main__':
    main()
