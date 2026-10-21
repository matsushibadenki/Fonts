#!/usr/bin/env python3
"""Reject mismatched tags, package versions and missing release notes before signing."""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def validate(tag, root=ROOT):
    if not re.fullmatch(r"v\d+\.\d+\.\d+", tag):
        raise ValueError("Expected a stable vX.Y.Z tag")
    version = tag[1:]
    package = json.loads((root / "package.json").read_text())
    lock = json.loads((root / "package-lock.json").read_text())
    config = json.loads((root / "src-tauri/tauri.conf.json").read_text())
    cargo_version = re.search(r'(?ms)^\[workspace\.package\]\s*\nversion = "([^"\n]+)"', (root / "Cargo.toml").read_text()).group(1)
    versions = [package["version"], lock["version"], lock["packages"][""]["version"],
                config["version"], cargo_version]
    if any(item != version for item in versions):
        raise ValueError("Tag and package versions must match")
    if not (root / f"docs/releases/{tag}.md").read_text().strip():
        raise ValueError("Release notes must not be empty")
    return version


if __name__ == "__main__":
    print(f"Validated release {validate(sys.argv[1])}")
