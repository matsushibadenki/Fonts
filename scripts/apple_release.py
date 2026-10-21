#!/usr/bin/env python3
"""Build with Developer ID, notarize both distribution formats, fail before publishing."""
import base64
import json
import os
from pathlib import Path
import re
import secrets
import shlex
import subprocess
import sys
import tempfile

REQUIRED = ('APPLE_CERTIFICATE', 'APPLE_CERTIFICATE_PASSWORD', 'APPLE_SIGNING_IDENTITY',
            'APPLE_TEAM_ID', 'APPLE_API_ISSUER', 'APPLE_API_KEY', 'APPLE_API_PRIVATE_KEY',
            'TAURI_SIGNING_PRIVATE_KEY')


def validate_environment(env):
    missing = [name for name in REQUIRED if not env.get(name)]
    if missing:
        raise ValueError('Missing release secrets: ' + ', '.join(missing))
    if not re.fullmatch(r'[A-Z0-9]{10}', env['APPLE_TEAM_ID']):
        raise ValueError('Invalid Apple Team ID')
    identity = env['APPLE_SIGNING_IDENTITY']
    if not identity.startswith('Developer ID Application: ') or not identity.endswith(f"({env['APPLE_TEAM_ID']})"):
        raise ValueError('Expected Developer ID Application identity for configured team')
    certificate = base64.b64decode(env['APPLE_CERTIFICATE'], validate=True)
    if not certificate:
        raise ValueError('Empty certificate')
    return certificate


def run(args, **kwargs):
    subprocess.run(args, check=True, **kwargs)


def verify_app(app, team):
    run(['codesign', '--verify', '--deep', '--strict', str(app)])
    info = subprocess.check_output(['codesign', '-dv', '--verbose=4', str(app)], stderr=subprocess.STDOUT).decode()
    if f'TeamIdentifier={team}\n' not in info or 'Authority=Developer ID Application:' not in info:
        raise ValueError('Unexpected Apple signing authority/team')
    if 'runtime' not in info or 'Timestamp=' not in info:
        raise ValueError('Missing hardened runtime or secure timestamp')
    run(['xcrun', 'stapler', 'validate', str(app)])
    run(['spctl', '--assess', '--type', 'execute', '--verbose=2', str(app)])


def main():
    env = os.environ.copy()
    certificate = validate_environment(env)
    if sys.argv[1:] == ["--check"]:
        print("Apple release credentials are configured")
        return
    original = shlex.split(subprocess.check_output(['security', 'list-keychains', '-d', 'user']).decode())
    with tempfile.TemporaryDirectory(prefix='fonts-apple-', dir=env.get('RUNNER_TEMP')) as folder:
        folder = Path(folder)
        keychain = folder / 'release.keychain-db'
        cert = folder / 'certificate.p12'
        key = folder / 'AuthKey.p8'
        for path, data in [(cert, certificate), (key, env['APPLE_API_PRIVATE_KEY'].encode())]:
            path.write_bytes(data)
            path.chmod(0o600)
        password = secrets.token_urlsafe(32)
        try:
            # Do not print command arguments or tool errors containing secret passwords.
            for args in [
                ['create-keychain', '-p', password, str(keychain)],
                ['set-keychain-settings', '-lut', '21600', str(keychain)],
                ['unlock-keychain', '-p', password, str(keychain)],
                ['import', str(cert), '-k', str(keychain), '-P', env['APPLE_CERTIFICATE_PASSWORD'], '-T', '/usr/bin/codesign'],
                ['set-key-partition-list', '-S', 'apple-tool:,apple:,codesign:', '-s', '-k', password, str(keychain)],
                ['list-keychains', '-d', 'user', '-s', str(keychain), *original],
            ]:
                result = subprocess.run(['security', *args], capture_output=True)
                if result.returncode:
                    raise RuntimeError('Temporary signing keychain setup failed')
            cert.unlink()
            env['APPLE_API_KEY_PATH'] = str(key)
            for name in ('APPLE_CERTIFICATE', 'APPLE_CERTIFICATE_PASSWORD', 'APPLE_API_PRIVATE_KEY'):
                env.pop(name, None)
            env['TAURI_SIGNING_PRIVATE_KEY_PASSWORD'] = ''
            run(['npm', 'run', 'tauri', '--', 'build', '--bundles', 'app,dmg', '--ci'], env=env)
            bundle = Path('target/release/bundle')
            app = bundle / 'macos/Fonts.app'
            verify_app(app, env['APPLE_TEAM_ID'])
            version = json.loads(Path('src-tauri/tauri.conf.json').read_text())['version']
            dmg = bundle / f'dmg/Fonts_{version}_aarch64.dmg'
            run(['codesign', '--force', '--sign', env['APPLE_SIGNING_IDENTITY'], '--keychain', str(keychain), '--timestamp', str(dmg)])
            # App is notarized/stapled by Tauri; separately notarize the final signed DMG.
            result = subprocess.check_output(['xcrun', 'notarytool', 'submit', str(dmg), '--key', str(key),
                '--key-id', env['APPLE_API_KEY'], '--issuer', env['APPLE_API_ISSUER'],
                '--wait', '--timeout', '20m', '--output-format', 'json'], env=env)
            if json.loads(result)['status'] != 'Accepted':
                raise RuntimeError('Apple rejected the DMG notarization')
            run(['xcrun', 'stapler', 'staple', str(dmg)])
            run(['xcrun', 'stapler', 'validate', str(dmg)])
            run(['codesign', '--verify', '--strict', str(dmg)])
            run(['spctl', '--assess', '--type', 'open', '--context', 'context:primary-signature', '--verbose=2', str(dmg)])
            with tempfile.TemporaryDirectory(prefix='fonts-update-check-') as extracted:
                run(['tar', '-xzf', str(bundle / 'macos/Fonts.app.tar.gz'), '-C', extracted])
                verify_app(Path(extracted) / 'Fonts.app', env['APPLE_TEAM_ID'])
        finally:
            subprocess.run(['security', 'list-keychains', '-d', 'user', '-s', *original], capture_output=True)
            subprocess.run(['security', 'delete-keychain', str(keychain)], capture_output=True)


if __name__ == '__main__':
    main()
