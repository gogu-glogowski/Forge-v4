#!/usr/bin/env python3
"""Choose desktop and run local Ansible; never partitions disks or sets passwords."""
import argparse,subprocess,shutil
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--desktop',choices=['gnome','cosmic','xfce'],default='gnome');p.add_argument('--check',action='store_true');p.add_argument('--syntax-check',action='store_true');a=p.parse_args()
if not shutil.which('ansible-playbook'):raise SystemExit('Install ansible-core with the native package manager first; see README.PL.md.')
cmd=['ansible-playbook','-i','localhost,',str(Path(__file__).with_name('configure.yml')),'-e',f'forge_desktop={a.desktop}']
if a.syntax_check:cmd.append('--syntax-check')
else:
 cmd.append('--ask-become-pass')
 if a.check:cmd+=['--check','--diff']
 else:
  print('This configures the currently booted installed system. Disks and passwords stay unchanged.')
  if input('Type KONFIGURUJ to continue: ')!='KONFIGURUJ':raise SystemExit('Cancelled')
subprocess.run(cmd,check=True)
