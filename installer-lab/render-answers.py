#!/usr/bin/env python3
"""Render private installer answers locally; passwords never enter the repository."""
import argparse,getpass,os,subprocess
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--output',required=True,type=Path);p.add_argument('--desktop',choices=['gnome','xfce'],default='gnome');a=p.parse_args()
if a.output.exists():raise SystemExit('Choose a new private output directory; existing data will not be overwritten.')
one=getpass.getpass('Password for Major: ');two=getpass.getpass('Repeat password: ')
if one!=two or not one:raise SystemExit('Passwords must match and be nonempty.')
hash=subprocess.check_output(['openssl','passwd','-6','-stdin'],input=one+'\n',text=True).strip();del one,two
os.umask(0o077);a.output.mkdir(parents=True,mode=0o700)
names={'debian-preseed.cfg.in':'debian/preseed.cfg','ubuntu-user-data.in':'ubuntu/user-data','el10-kickstart.cfg.in':'el10/ks.cfg'}
for path in Path(__file__).with_name('templates').iterdir():
 text=path.read_text().replace('@PASSWORD_HASH@',hash).replace('@DEBIAN_TASK@','gnome-desktop' if a.desktop=='gnome' else 'xfce-desktop')
 target=a.output/names[path.name];target.parent.mkdir(exist_ok=True,mode=0o700);target.write_text(text);target.chmod(0o600)
(a.output/'ubuntu/meta-data').write_text('instance-id: forge-install\nlocal-hostname: forge-ubuntu\n')
print('Private answers written. Only Debian uses --desktop; Ubuntu/RHEL keep GNOME.')
