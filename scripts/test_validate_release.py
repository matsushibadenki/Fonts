import unittest
from validate_release import validate


class ReleaseValidationTests(unittest.TestCase):
    def test_current_release(self):
        self.assertEqual(validate('v0.1.2'), '0.1.2')

    def test_reject_mismatched_version(self):
        with self.assertRaises(ValueError):
            validate('v99.0.0')

    def test_reject_untrusted_tag_syntax(self):
        for tag in ['main', 'v0.1.2-beta', '../v0.1.2', 'v0.1.2; echo bad']:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                validate(tag)
