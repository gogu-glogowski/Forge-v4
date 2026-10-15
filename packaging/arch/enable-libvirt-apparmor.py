#!/usr/bin/env python3
"""Patch a local official libvirt PKGBUILD; never build or install automatically."""
import argparse
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('pkgbuild', type=Path)
args = parser.parse_args()
s = args.pkgbuild.read_text()
for option in ['apparmor', 'apparmor_profiles', 'secdriver_apparmor']:
    old = f'-D{option}=disabled'
    if s.count(old) != 1:
        raise SystemExit(f'Review upstream changes: expected one {old}')
    s = s.replace(old, f'-D{option}=enabled')
if 'depends=(\n' not in s:
    raise SystemExit('Review upstream dependency layout')
s = s.replace('depends=(\n', 'depends=(\n  apparmor\n', 1)
output = args.pkgbuild.with_name('PKGBUILD.forge-apparmor')
if output.exists():
    raise SystemExit(f'Refusing to overwrite {output}')
output.write_text(s)
print(output)
print('Review diff, build on the native target with upstream checks; no installation was performed.')
