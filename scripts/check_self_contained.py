#!/usr/bin/env python3
"""Check product paths without relying on developer reference checkouts.
Requires Python 3.11+. Run after resolving the lockfile (CI runs this after cargo test).
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parent.parent


def require_local(path):
    resolved = path.resolve(strict=True)
    if not resolved.is_relative_to(ROOT):
        raise ValueError(f'External product path: {path} -> {resolved}')


def main():
    for name in ('fontforge', 'LumaPaint'):
        if os.path.lexists(ROOT / name):
            raise ValueError(f'Reference checkout/link must not be present: {name}')
    for name in ('crates', 'vendor', 'src', 'src-tauri', 'scripts', 'node_modules', '.github'):
        directory = ROOT / name
        if not directory.exists():
            continue
        require_local(directory)
        for base, dirs, files in os.walk(directory, followlinks=False):
            dirs[:] = [d for d in dirs if d not in ('target', '__pycache__')]
            for entry in dirs + files:
                path = Path(base) / entry
                if path.is_symlink():
                    require_local(path)
    for name in ('Cargo.toml', 'Cargo.lock', 'package.json', 'package-lock.json', 'vite.config.ts'):
        require_local(ROOT / name)
    lock = json.loads((ROOT / 'package-lock.json').read_text())
    for package in lock.get('packages', {}).values():
        target = package.get('resolved', '')
        if target.startswith('file:'):
            require_local(ROOT / unquote(target[5:]))
        elif package.get('link'):
            require_local(ROOT / target)
    config = json.loads((ROOT / 'src-tauri/tauri.conf.json').read_text())
    resources = config['bundle'].get('resources', [])
    for pattern in resources:
        base = ROOT / 'src-tauri'
        # Check the path even before expanding a resource glob.
        if not (base / pattern).resolve().is_relative_to(ROOT):
            raise ValueError(f'External bundle resource: {pattern}')
        matches = list(base.glob(pattern))
        if not matches:
            raise ValueError(f'Missing bundle resource: {pattern}')
        for path in matches:
            require_local(path)
    # --no-deps avoids downloading registry packages for other OS targets in macOS CI.
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--format-version', '1', '--no-deps', '--locked', '--offline'], cwd=ROOT))
    pending = [ROOT / 'Cargo.toml']
    for package in metadata['packages']:
        if package.get('source') is None:
            pending.append(Path(package['manifest_path']))
            for target in package['targets']:
                require_local(Path(target['src_path']))
    seen = set()
    while pending:
        manifest = pending.pop().resolve(strict=True)
        require_local(manifest)
        if manifest in seen:
            continue
        seen.add(manifest)
        config = tomllib.loads(manifest.read_text())
        tables = [config, config.get('workspace', {}), *config.get('target', {}).values()]
        dependencies = []
        for table in tables:
            for section in ('dependencies', 'dev-dependencies', 'build-dependencies'):
                dependencies.extend(table.get(section, {}).values())
        for overrides in config.get('patch', {}).values():
            dependencies.extend(overrides.values())
        dependencies.extend(config.get('replace', {}).values())
        for dependency in dependencies:
            if isinstance(dependency, dict) and 'path' in dependency:
                directory = manifest.parent / dependency['path']
                require_local(directory)
                pending.append(directory / 'Cargo.toml')
    print(f'Self-contained: {len(seen) - 1} local Cargo packages; product/npm/resource paths stay inside Fonts.')



if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f'Self-contained check failed: {error}', file=sys.stderr)
        sys.exit(1)
