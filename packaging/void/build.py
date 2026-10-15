#!/usr/bin/env python3
"""Build a native unsigned XBPS binary package; never install or start services."""
import argparse,hashlib,subprocess,tempfile,tomllib,shutil
from pathlib import Path
REPO=Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--output',type=Path,required=True);p.add_argument('--skip-build',action='store_true');a=p.parse_args()
v=tomllib.loads((REPO/'Cargo.toml').read_text())['workspace']['package']['version']
if not a.skip_build:subprocess.run(['cargo','build','--release','--locked','-p','forge-cli'],cwd=REPO,check=True)
b=REPO/'target/release/forge';assert subprocess.check_output([str(b),'--version'],text=True).strip()==f'forge {v}'
out=a.output.resolve();out.mkdir(parents=True,exist_ok=True)
with tempfile.TemporaryDirectory(prefix='forge-xbps-') as tmp:
 root=Path(tmp)/'root';(root/'usr/bin').mkdir(parents=True);shutil.copy2(b,root/'usr/bin/forge')
 for name in ['README.md','CHANGELOG.md']:
  d=root/'usr/share/doc/forge'/name;d.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(REPO/name,d)
 for name in ['CREDITS.md','LICENSE-CC-BY-4.0.txt','manifest.json']:
  d=root/'usr/share/forge/music'/name;d.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(REPO/'assets/music'/name,d)
 d=root/'usr/share/licenses/forge/LICENSE';d.parent.mkdir(parents=True);shutil.copyfile(REPO/'LICENSE',d)
 deps='glibc>=2.39 libgcc qemu libvirt gnome-boxes NetworkManager gnupg curl ca-certificates 7zip tar xz e2fsprogs iproute2 procps-ng psmisc runit sudo coreutils util-linux apparmor mpv'
 deps=' '.join(n if '>=' in n else n+'>=0' for n in deps.split())
 subprocess.run(['xbps-create','-A','x86_64','-n',f'forge-{v}_1','-s','Verified KVM lab (experimental Void glibc port)','-l','Apache-2.0, CC-BY-4.0','-D',deps,str(root)],cwd=out,check=True)
 files=list(out.glob(f'forge-{v}_1.x86_64.xbps'));assert len(files)==1
 (out/'SHA256SUMS').write_text(f'{hashlib.sha256(files[0].read_bytes()).hexdigest()}  {files[0].name}\n')
 print(files[0])
