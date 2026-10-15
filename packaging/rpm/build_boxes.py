#!/usr/bin/env python3
"""Build a native AlmaLinux 10 Boxes RPM from verified GNOME source or a staged build."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

VERSION = '50.0'
SOURCE_SHA256 = 'fd6a5de18d1090946ca99f1f5a34aa3e15dc8183f6a72226e8d1504a13672d67'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True, type=Path)
    parser.add_argument('--stage', type=Path, help='Package an already compiled DESTDIR; no build')
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    source = args.source.resolve()
    if hashlib.sha256(source.read_bytes()).hexdigest() != SOURCE_SHA256:
        raise SystemExit('GNOME source checksum mismatch')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='boxes-el10-') as temporary:
        top = Path(temporary)
        for name in ['SOURCES', 'SPECS', 'BUILD', 'BUILDROOT', 'RPMS', 'SRPMS', 'tmp', 'rpmdb']:
            (top / name).mkdir()
        stage = top / 'stage'
        if args.stage:
            shutil.copytree(args.stage.resolve(), stage, symlinks=True)
        else:
            with tarfile.open(source) as archive:
                archive.extractall(top, filter='data')
            build = top / 'build'
            subprocess.run(['meson', 'setup', str(build), str(top / f'gnome-boxes-{VERSION}'),
                            '--prefix=/usr', '--libdir=lib64', '--buildtype=release',
                            '-Ddistributor_name=AlmaLinux', '-Ddistributor_version=10',
                            '-Dinstalled_tests=false'], check=True)
            subprocess.run(['meson', 'compile', '-C', str(build)], check=True)
            subprocess.run(['meson', 'test', '-C', str(build), '--print-errorlogs'], check=True)
            subprocess.run(['meson', 'install', '-C', str(build), '--no-rebuild'], check=True,
                           env={**os.environ, 'DESTDIR': str(stage)})
        # libgovf is private to Boxes. Development files and installed GUI tests
        # are not needed in the runtime package.
        for relative in ['usr/include/gnome-boxes', 'usr/share/gnome-boxes/gir-1.0',
                         'usr/share/gnome-boxes/vapi', 'usr/lib64/gnome-boxes/girepository-1.0',
                         'usr/lib64/gnome-boxes/pkgconfig', 'usr/share/installed-tests/gnome-boxes',
                         'usr/libexec/gnome-boxes/installed-tests']:
            shutil.rmtree(stage / relative, ignore_errors=True)
        with tarfile.open(source) as archive:
            license_data = archive.extractfile(f'gnome-boxes-{VERSION}/COPYING').read()
        license_path = stage / 'usr/share/licenses/gnome-boxes/COPYING'
        license_path.parent.mkdir(parents=True, exist_ok=True)
        license_path.write_bytes(license_data)
        manifest = []
        for path in sorted((stage / 'usr').rglob('*')):
            relative = '/' + path.relative_to(stage).as_posix()
            if path.is_file() or path.is_symlink():
                manifest.append(relative)
            elif 'gnome-boxes' in path.parts or path.name == 'gnome-boxes':
                manifest.append('%dir ' + relative)
        (top / 'SOURCES/files.list').write_text('\n'.join(manifest) + '\n')
        with tarfile.open(top / 'SOURCES/boxes-payload.tar.xz', 'w:xz') as archive:
            archive.add(stage / 'usr', arcname='usr')
        spec = '''Name: gnome-boxes
Version: 50.0
Release: 1.el10
Summary: Native GNOME Boxes for AlmaLinux 10
License: LGPL-2.0-or-later AND CC0-1.0 AND CC-BY-SA-3.0
URL: https://apps.gnome.org/Boxes/
Source0: boxes-payload.tar.xz
Source1: files.list
ExclusiveArch: x86_64
Requires: almalinux-release >= 10
Requires: libvirt-daemon-kvm libvirt-client qemu-img libosinfo osinfo-db
Requires: adwaita-icon-theme gsettings-desktop-schemas mtools genisoimage
%global __requires_exclude ^libgovf-0[.]1[.]so.*$

%description
GNOME Boxes built from verified upstream 50.0 source against AlmaLinux 10
libraries. Connect Forge to QEMU System (qemu:///system). No libvirt services,
networks, hooks or user groups are modified by this package.

%prep
mkdir payload
tar -xJf %{SOURCE0} -C payload

%build

%install
mkdir -p %{buildroot}/usr
cp -a payload/usr/. %{buildroot}/usr/

%files -f %{SOURCE1}
'''
        spec_path = top / 'SPECS/gnome-boxes.spec'
        spec_path.write_text(spec)
        subprocess.run(['rpmbuild', '-bb', '--define', f'_topdir {top}',
                        '--define', f'_dbpath {top}/rpmdb', '--define', f'_tmppath {top}/tmp',
                        str(spec_path)], check=True)
        packages = list((top / 'RPMS/x86_64').glob('gnome-boxes-*.rpm'))
        if len(packages) != 1:
            raise SystemExit('Expected one Boxes RPM')
        target = output / packages[0].name
        shutil.copyfile(packages[0], target)
        (output / 'gnome-boxes-payload.spec').write_text(spec)
        print('RPM:', target)


if __name__ == '__main__':
    main()
