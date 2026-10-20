#!/usr/bin/env python3
"""Prepare public release assets after local signing. Never reads the private key."""
import argparse,datetime,hashlib,json,platform,re,shutil,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent

def main():
 p=argparse.ArgumentParser();p.add_argument('--bundle',type=Path,default=ROOT/'target/release/bundle');p.add_argument('--out',type=Path,default=ROOT/'target/release-publish');a=p.parse_args()
 config=ROOT/'src-tauri/tauri.conf.json';version=json.loads(config.read_text())['version']
 assert re.fullmatch(r'\d+\.\d+\.\d+',version),'Only stable versions may be published'
 machine=platform.machine();assert machine in ['arm64','x86_64'],'Build on the native supported macOS architecture'
 arch='aarch64' if machine=='arm64' else 'x86_64';bundle=a.bundle.resolve();archive=bundle/'macos/Fonts.app.tar.gz';sig=archive.with_name(archive.name+'.sig')
 subprocess.run(['cargo','run','--locked','--release','-p','fonts-app','--example','verify_update','--',str(archive),str(sig),str(config)],cwd=ROOT,check=True)
 out=a.out.resolve()/f'v{version}';out.mkdir(parents=True,exist_ok=True)
 name=f'Fonts_{version}_{arch}.app.tar.gz';shutil.copyfile(archive,out/name);shutil.copyfile(sig,out/(name+'.sig'))
 assets=[out/name,out/(name+'.sig')]
 for dmg in (bundle/'dmg').glob('*.dmg'):
  target=out/dmg.name;shutil.copyfile(dmg,target);assets.append(target)
 manifest={'version':version,'notes':(ROOT/f'docs/releases/v{version}.md').read_text(),'pub_date':datetime.datetime.now(datetime.timezone.utc).isoformat().replace('+00:00','Z'),'platforms':{f'darwin-{arch}':{'url':f'https://github.com/matsushibadenki/Fonts/releases/download/v{version}/{name}','signature':sig.read_text().strip()}}}
 (out/'latest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n');assets.append(out/'latest.json')
 (out/'SHA256SUMS').write_text(''.join(f'{hashlib.sha256(file.read_bytes()).hexdigest()}  {file.name}\n' for file in assets))
 print(out)
if __name__=='__main__':main()
