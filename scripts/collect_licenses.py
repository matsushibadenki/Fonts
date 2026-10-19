#!/usr/bin/env python3
"""Collect locked license declarations, local notice texts and unchanged MPL sources.

No package downloads or source modifications. Run npm ci first and keep Cargo's
locked registry source/archive cache available. Does not certify embedded/system
libraries or identify which packages a particular binary actually links.
"""
import collections
from datetime import date
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'src-tauri/resources'
DATE = date.today().isoformat()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def select_license(expression):
    """Explicit selections for this lockfile; unknown compound terms need review."""
    if not expression:
        raise ValueError('Missing package license declaration')
    value = expression.replace(' / ', ' OR ').replace('/', ' OR ')
    conjunctions = {
        '(Apache-2.0 OR MIT) AND BSD-3-Clause': 'MIT AND BSD-3-Clause',
        '(MIT OR Apache-2.0) AND Unicode-3.0': 'MIT AND Unicode-3.0',
        'BSD-3-Clause AND MIT': 'BSD-3-Clause AND MIT',
        'Apache-2.0 AND MIT': 'Apache-2.0 AND MIT',
    }
    if value == 'CC0-1.0 OR MIT-0 OR Apache-2.0':
        return 'CC0-1.0'  # This package includes the full CC0 text locally.
    if value in conjunctions:
        return conjunctions[value]
    choices = value.split(' OR ')
    allowed = ['MIT', 'MIT-0', 'Apache-2.0', 'BSD-2-Clause', 'BSD-3-Clause',
               'ISC', 'Zlib', 'Unicode-3.0', 'MPL-2.0', 'Apache-2.0 WITH LLVM-exception']
    if any(choice not in allowed + ['0BSD', 'CC0-1.0', 'Unlicense', 'LGPL-2.1-or-later'] for choice in choices):
        raise ValueError(f'License expression needs review: {expression}')
    for choice in allowed:
        if choice in choices:
            return choice
    raise ValueError(f'No reviewed selection: {expression}')


def notice_files(root):
    return sorted(f for f in root.iterdir() if f.is_file() and
                  any(word in f.name.lower() for word in ['license', 'copying', 'notice']))


def main():
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--format-version', '1', '--locked', '--offline'], cwd=ROOT))
    npm_lock = json.loads((ROOT / 'package-lock.json').read_text())
    checksums = {}
    for block in (ROOT / 'Cargo.lock').read_text().split('[[package]]')[1:]:
        fields = dict(re.findall(r'^([a-z]+) = \"([^\"]+)\"$', block, re.M))
        checksums[(fields['name'], fields['version'])] = fields.get('checksum')
    report = {
        'checked_on': DATE,
        'scope': 'All locked Rust and npm packages, including build/test/optional/non-host packages. Not a binary linkage SBOM. Embedded native and system libraries require the separate README review.',
        'policy': 'MIT for Fonts-owned code, including owner-authorized LumaPaint copies. Third-party code retains the selected upstream license; AND terms are cumulative. FreeType uses FTL. Built-in snippet geometry remains CC0-1.0.',
        'lockfile_sha256': {p: digest(ROOT / p) for p in ['Cargo.lock', 'package-lock.json']},
        'rust': [], 'npm': [], 'mpl_sources': [],
    }
    sections = [
        'Fonts — third-party license notices',
        'Generated from Cargo.lock, package-lock.json and installed package license texts.',
        'Includes development and non-host-platform packages for transparency; inclusion does not assert binary linkage.',
        'Upstream license/copyright/NOTICE texts below are preserved. The selections in the inventory do not override file-level licenses.',
        'FreeType and its internal licenses are listed in FONT_ENGINE_NOTICES.txt and the native section below.',
        'Unmodified MPL Rust source archives and the MPL license are included under resources/licenses/.',
        'Inspect the matching license-inventory.json for exact versions and checksums.',
        'Linux system GTK/WebKitGTK and OS/SDK runtime terms require a separate platform distribution review.',
    ]
    sources = OUT / 'licenses/sources'
    sources.mkdir(parents=True, exist_ok=True)
    expected_sources = set()
    workspace = set(metadata['workspace_members'])
    for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        root = Path(package['manifest_path']).parent
        declared = package.get('license')
        selected = select_license(declared)
        files = notice_files(root)
        record = {
            'name': package['name'], 'version': package['version'],
            'declared_license': declared, 'selected_license': selected,
            'origin': 'crates.io' if package.get('source') else 'workspace' if package['id'] in workspace else 'vendored',
            'repository': package.get('repository'),
            'local_notice_files': [p.name for p in files],
            'upstream_notice_review_required': not files and package['id'] not in workspace,
        }
        report['rust'].append(record)
        if package['id'] in workspace:
            continue
        label = f"Rust {package['name']} {package['version']} — {declared} (selected: {selected})"
        sections += ['\n' + '=' * 72, label]
        if not files:
            sections.append('No root license file in this package; retain its source-file headers and upstream declaration. See inventory for source repository.')
        for file in files:
            sections += [f'\n--- {file.name} ---', file.read_text(errors='replace')]
        if selected == 'MPL-2.0':
            name = f"{package['name']}-{package['version']}.crate"
            archive = root.parent.parent.parent / 'cache' / root.parent.name / name
            if digest(archive) != checksums[(package['name'], package['version'])]:
                raise ValueError(f'Archive checksum mismatch: {name}')
            shutil.copyfile(archive, sources / name)
            expected_sources.add(name)
            report['mpl_sources'].append({'name': package['name'], 'version': package['version'],
                'file': 'sources/' + name, 'sha256': digest(archive), 'modified': False})
    for file in sources.glob('*.crate'):
        if file.name not in expected_sources:
            file.unlink()  # Remove only obsolete generated source archives.
    for package_path, package in sorted(npm_lock['packages'].items()):
        if not package_path:
            continue
        declared = package.get('license')
        selected = select_license(declared)
        root = ROOT / package_path
        files = notice_files(root) if root.is_dir() else []
        report['npm'].append({'package_path': package_path, 'version': package['version'],
            'declared_license': declared, 'selected_license': selected,
            'dev': bool(package.get('dev')), 'optional': bool(package.get('optional')),
            'installed': root.is_dir(), 'local_notice_files': [p.name for p in files]})
        sections += ['\n' + '=' * 72, f"npm {package_path} {package['version']} — {declared} (selected: {selected})"]
        if not files:
            sections.append('No installed root notice text on this host; optional non-host packages and development-tool notices must be checked before redistributing them.')
        for file in files:
            sections += [f'\n--- {file.name} ---', file.read_text(errors='replace')]
    native_files = [
        ROOT / 'vendor/freetype-sys/freetype2/LICENSE.TXT',
        ROOT / 'vendor/freetype-sys/freetype2/docs/FTL.TXT',
        ROOT / 'vendor/freetype-sys/freetype2/src/bdf/README',
        ROOT / 'vendor/freetype-sys/freetype2/src/pcf/README',
        ROOT / 'vendor/freetype-sys/freetype2/src/gzip/zlib.h',
        ROOT / 'vendor/freetype-sys/freetype2/src/autofit/ft-hb-ft.c',
        ROOT / 'vendor/freetype-sys/freetype2/src/base/fthash.c',
    ]
    libz = next(p for p in metadata['packages'] if p['name'] == 'libz-sys')
    native_files.append(Path(libz['manifest_path']).parent / 'src/zlib/LICENSE')
    sections += ['\n' + '=' * 72, 'Native sources: FreeType (FTL, internal MIT/Old MIT and Zlib notices); zlib (Zlib)']
    for file in native_files:
        text = file.read_text(errors='replace')
        # C/H headers contain license blocks; do not turn the notice into a code dump.
        if file.suffix in ['.c', '.h']:
            comments = re.match(r'\A(?:\s*/\*.*?\*/\s*)+', text, re.S)
            if not comments:
                raise ValueError(f'Native notice header not found: {file}')
            text = comments.group(0)
        sections += ['\n--- ' + ('zlib/LICENSE' if file not in native_files[:-1] else str(file.relative_to(ROOT))) + ' ---', text]
    native_root = ROOT / 'vendor/font-hinting'
    sections += ['\n' + '=' * 72, 'Fonts is based in part on the work of the FreeType Team and the ttfautohint library. ttfautohint uses FTL (including its generated-bytecode exception); HarfBuzz uses Old MIT and file-level licenses.']
    report['native_hinting'] = json.loads((native_root / 'manifest.json').read_text())
    for relative in ['ttfautohint/COPYING', 'ttfautohint/FTL.TXT', 'harfbuzz/COPYING', 'harfbuzz/src/ms-use/COPYING']:
        file = native_root / relative
        sections += ['\n--- ' + str(file.relative_to(ROOT)) + ' ---', '\n'.join(line.rstrip() for line in file.read_text(errors='replace').splitlines()).rstrip()]
    # Retain individual authors' headers, including BSD/X11 and Bison's skeleton exception.
    headers = set()
    for file in sorted(native_root.rglob('*')):
        if file.suffix not in ['.h', '.hh', '.c', '.cc']: continue
        text = file.read_text(errors='replace')
        match = re.match(r'\A(?:\s*/\*.*?\*/\s*)+', text, re.S)
        if match and match.group(0) not in headers:
            header = match.group(0)
            if 'copyright' in header.lower() or 'license' in header.lower():
                headers.add(header)
                sections += ['\n--- ' + str(file.relative_to(ROOT)) + ' ---', '\n'.join(line.rstrip() for line in header.splitlines()).rstrip()]
    mpl = next(p for p in metadata['packages'] if p['name'] == 'option-ext')
    mpl_text = Path(mpl['manifest_path']).parent / 'LICENSE.txt'
    shutil.copyfile(mpl_text, OUT / 'licenses/MPL-2.0.txt')
    dunce = next(p for p in metadata['packages'] if p['name'] == 'dunce')
    shutil.copyfile(Path(dunce['manifest_path']).parent / 'LICENSE', OUT / 'licenses/CC0-1.0.txt')
    shutil.copyfile(ROOT / 'LICENSE', OUT / 'LICENSE-MIT.txt')
    inventory = json.dumps(report, ensure_ascii=False, indent=2) + '\n'
    (ROOT / 'docs/license-inventory.json').write_text(inventory)
    (OUT / 'licenses/license-inventory.json').write_text(inventory)
    (OUT / 'licenses/README.txt').write_text(
        'Fonts MPL source availability\n\nThe unchanged .crate archives in sources/ contain the exact upstream Rust MPL-2.0 sources used by this lockfile. They are standard gzip-compressed tar archives and can be unpacked with tar -xzf. SHA-256 values are in license-inventory.json and match Cargo.lock.\n\nThese sources remain under MPL-2.0, not the Fonts MIT license. See MPL-2.0.txt. No MPL source files were modified.\n\nlightningcss and its platform packages in package-lock.json are MPL-2.0 build dependencies, not included as executables in the Fonts app. If redistributing these development tools, separately provide the matching upstream sources and notices.\n')
    (OUT / 'THIRD_PARTY_NOTICES.txt').write_text('\n'.join(sections) + '\n')
    print(f"Collected {len(report['rust'])} Rust packages, {len(report['npm'])} npm packages and {len(report['mpl_sources'])} unchanged MPL source archives.")
    print('Selected Rust licenses:', dict(collections.Counter(p['selected_license'] for p in report['rust'])))


if __name__ == '__main__':
    main()
