import base64
import unittest
from unittest.mock import patch
from apple_release import REQUIRED, validate_environment, verify_app


class AppleReleaseTests(unittest.TestCase):
    def environment(self):
        env = dict.fromkeys(REQUIRED, 'test')
        env.update(APPLE_TEAM_ID='ABCDE12345',
                   APPLE_SIGNING_IDENTITY='Developer ID Application: Test (ABCDE12345)',
                   APPLE_CERTIFICATE=base64.b64encode(b'fixture').decode())
        return env

    def test_missing_credentials_stop_release(self):
        for name in REQUIRED:
            env = self.environment()
            del env[name]
            with self.subTest(secret=name), self.assertRaises(ValueError):
                validate_environment(env)

    def test_reject_development_identity_and_wrong_team(self):
        for identity in ['Apple Development: Test (ABCDE12345)', 'Developer ID Application: Test (OTHER12345)', '-']:
            env = self.environment()
            env['APPLE_SIGNING_IDENTITY'] = identity
            with self.subTest(identity=identity), self.assertRaises(ValueError):
                validate_environment(env)

    def test_reject_untrusted_app_before_gatekeeper(self):
        for info in [b'TeamIdentifier=OTHER12345\nAuthority=Developer ID Application: Test\nruntime\nTimestamp=now',
                     b'TeamIdentifier=ABCDE12345\nSignature=adhoc\nruntime\nTimestamp=now',
                     b'TeamIdentifier=ABCDE12345\nAuthority=Developer ID Application: Test\n']:
            with patch('apple_release.run') as run, patch('apple_release.subprocess.check_output', return_value=info):
                with self.assertRaises(ValueError):
                    verify_app('Fonts.app', 'ABCDE12345')
                self.assertEqual(run.call_count, 1)
