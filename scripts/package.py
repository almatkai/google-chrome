"""Deterministic development packages only; never uploads or invents artifact pins."""
import argparse, hashlib, json, pathlib, stat, subprocess, sys, zipfile
ROOT = pathlib.Path(__file__).resolve().parents[1]
sha = lambda b: hashlib.sha256(b).hexdigest()
p = argparse.ArgumentParser()
p.add_argument('--binary', type=pathlib.Path, required=True)
p.add_argument('--target', choices=['x86_64-pc-windows-msvc', 'aarch64-apple-darwin'], required=True)
p.add_argument('--output', type=pathlib.Path, default=ROOT / 'dist')
a = p.parse_args()
manifest = json.loads((ROOT / 'extension.json').read_text(encoding='utf-8'))
info = json.loads(subprocess.check_output([str(a.binary.resolve()), '--protocol-info'], timeout=15))
assert info['id'] == manifest['id'] and info['version'] == manifest['version'] and info['protocol'] == 1
assert info['commands'] == [c['name'] for c in manifest['commands']]
lock = (ROOT / 'Cargo.lock').read_text()
assert '#1a11afc14693cc6463e0b95ae7d61991b4a60427"' in lock
subprocess.run([sys.executable, str(ROOT / 'scripts/vendor-beta9-licenses.py'), '--check'], cwd=ROOT, check=True)
windows = a.target.startswith('x86_64-pc-windows')
exe = 'bin/tezbar-google-chrome' + ('.exe' if windows else '')
payload = {exe: a.binary.read_bytes(), 'LICENSE': (ROOT / 'LICENSE').read_bytes(), 'NOTICE': (ROOT / 'NOTICE').read_bytes()}
header=payload[exe]
if windows:
 assert header[:2] == b'MZ', 'Expected Windows PE'
 offset=int.from_bytes(header[60:64], 'little');assert header[offset:offset+6]==b'PE\0\0d\x86','Expected PE x64'
else:
 assert header[:4]==bytes.fromhex('cffaedfe') and int.from_bytes(header[4:8],'little')==0x0100000c,'Expected ARM64 Mach-O'
for directory in ['resources', 'licenses']:
 for file in sorted((ROOT / directory).rglob('*')):
  assert not file.is_symlink()
  if file.is_file(): payload[file.relative_to(ROOT).as_posix()] = file.read_bytes()
manifest['files'] = {name: sha(data) for name, data in sorted(payload.items())}
manifest['artifacts'] = {}
assert payload[manifest['icon']] == (ROOT / 'upstream/assets/chrome-icon.png').read_bytes()
payload['extension.json'] = (json.dumps(manifest, ensure_ascii=False, indent=2) + '\n').encode()
output = a.output; output.mkdir(parents=True, exist_ok=True)
name = f'google-chrome-{a.target}-development.zip'
archive = output / name
with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED) as z:
 for name, data in sorted(payload.items()):
  entry=zipfile.ZipInfo(name, date_time=(2026,1,1,0,0,0));entry.create_system=3;entry.external_attr=(stat.S_IFREG|(0o755 if name==exe else 0o644))<<16;entry.compress_type=zipfile.ZIP_DEFLATED;z.writestr(entry,data)
with zipfile.ZipFile(archive) as z:
 assert set(z.namelist()) == set(payload)
 for name, digest in manifest['files'].items(): assert sha(z.read(name)) == digest
report=dict(target=a.target,archive=str(archive),sha256=sha(archive.read_bytes()),executable=exe,executableSha256=sha(payload[exe]),files=manifest['files'],developmentOnly=True,platformSupported=not windows,limitation='Windows is a protocol/fixture probe only; upstream and native package declare macOS only. macOS desktop acceptance remains pending.')
(output / (archive.name+'.sha256')).write_text(report['sha256']+'  '+archive.name+'\n')
(output / (archive.name+'.artifact.json')).write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='files'},indent=2))
