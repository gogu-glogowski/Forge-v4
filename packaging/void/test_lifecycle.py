#!/usr/bin/env python3
"""XBPS lifecycle in a fresh isolated root with synthetic dependency providers.
This checks package handling, not native dependency availability or confinement.
"""
import argparse,subprocess,tempfile,shutil,os,hashlib,re
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__);p.add_argument('package',type=Path);p.add_argument('--root',required=True,type=Path);a=p.parse_args()
root=a.root.absolute();assert root==root.resolve() and not root.exists() and len(root.parts)>=4
root.mkdir(parents=True);env=os.environ.copy();env['XBPS_ARCH']='x86_64'
def run(*args):
 r=subprocess.run(list(map(str,args)),env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 if r.returncode:raise RuntimeError(r.stdout)
 return r.stdout
keep=root/'var/lib/forge/vms/keep.qcow2';keep.parent.mkdir(parents=True);keep.write_bytes(b'PRESERVE LAB');before=hashlib.sha256(keep.read_bytes()).digest()
with tempfile.TemporaryDirectory(prefix='forge-xbps-fixture-') as tmp:
 repo=Path(tmp);shutil.copy2(a.package,repo/a.package.name)
 listing=run('tar','--zstd','-tf',a.package).splitlines();assert not any(n.endswith('.mp3') for n in listing)
 # xbps-create dependency names match the actual candidate package metadata.
 deps='glibc libgcc qemu libvirt gnome-boxes NetworkManager gnupg curl ca-certificates 7zip tar xz e2fsprogs iproute2 procps-ng psmisc runit sudo coreutils util-linux apparmor'.split()
 fixture=repo/'fixture';(fixture/'usr/share/forge-test').mkdir(parents=True);(fixture/'usr/share/forge-test/marker').write_text('synthetic test only')
 subprocess.run(['xbps-create','-A','x86_64','-n','forge-test-deps-1_1','-s','Synthetic dependency fixture','-P',' '.join(n+'-9999_1' for n in deps),str(fixture)],cwd=repo,env=env,check=True,stdout=subprocess.DEVNULL)
 pkgs=list(repo.glob('*.xbps'));print(run('xbps-rindex','-a',*pkgs))
 prefix=['xbps-install','-r',root,'-i','-R',repo,'-y']
 print(run(*prefix,'forge'))
 assert '4.2.0' in run(root/'usr/bin/forge','--version')
 print(run(*prefix,'-f','forge'))
 print(run('xbps-remove','-r',root,'-y','forge'))
 assert not (root/'usr/bin/forge').exists()
 assert hashlib.sha256(keep.read_bytes()).digest()==before
print('PASS isolated XBPS install/reinstall/remove; lab preserved. Dependencies are synthetic; no native support claim.')
