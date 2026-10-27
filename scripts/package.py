"""Package only mzk, documentation and dependency information; never bundle mpv."""
import hashlib
import json
import pathlib
import subprocess
import sys
import tomllib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
target, platform = sys.argv[1:]
version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
exe = 'mzk.exe' if platform.startswith('windows') else 'mzk'
binary = ROOT / 'target' / target / 'release' / exe
assert binary.is_file(), binary
dist = ROOT / 'dist'
dist.mkdir(exist_ok=True)
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=ROOT))
inventory = [{k: p.get(k) for k in ('name', 'version', 'license', 'repository')} for p in metadata['packages']]
name = f'mzk-{version}-{platform}'
archive = dist / (name + '.zip')
with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as z:
    z.write(binary, f'{name}/{exe}')
    for rel in ('README.md', 'README.zh-CN.md', 'LICENSE', 'SECURITY.md', 'THIRD_PARTY.md', 'CHANGELOG.md', '.env.example', 'Cargo.lock'):
        z.write(ROOT / rel, f'{name}/{rel}')
    for path in (ROOT / 'docs').glob('*.md'):
        z.write(path, f'{name}/docs/{path.name}')
    z.writestr(f'{name}/dependencies.json', json.dumps(inventory, indent=2) + '\n')
    for package in metadata['packages']:
        if not package.get('source'):
            continue
        crate = pathlib.Path(package['manifest_path']).parent
        notices = set()
        if package.get('license_file'):
            notices.add(crate / package['license_file'])
        for path in crate.rglob('*'):
            if path.is_file() and path.name.upper().startswith(('LICENSE', 'COPYING', 'NOTICE')) and path.suffix.lower() in ('', '.txt', '.md', '.html', '.rst'):
                notices.add(path)
        for path in sorted(notices):
            if path.is_file():
                z.write(path, f'{name}/dependency-licenses/{package["name"]}-{package["version"]}/{path.relative_to(crate).as_posix()}')
(dist / (name + '.sha256')).write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + '  ' + archive.name + '\n')
print(archive.name)
