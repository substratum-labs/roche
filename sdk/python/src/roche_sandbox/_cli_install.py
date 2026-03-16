# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Auto-download prebuilt Roche CLI binary.

This module is invoked as a post-install hook via the ``[cli]`` extras::

    pip install roche-sandbox[cli]

It downloads the appropriate prebuilt binary from GitHub Releases and
places it in the Python environment's ``bin/`` (or ``Scripts/``) directory.
"""

from __future__ import annotations

import io
import os
import platform
import shutil
import stat
import sys
import tarfile
import urllib.request

GITHUB_REPO = "substratum-labs/roche"
VERSION = "0.1.0"


def _detect_target() -> str:
    machine = platform.machine().lower()
    system = platform.system().lower()

    if system == "linux":
        arch = "aarch64" if machine in ("aarch64", "arm64") else "x86_64"
        return f"{arch}-unknown-linux-gnu"
    elif system == "darwin":
        arch = "aarch64" if machine in ("aarch64", "arm64") else "x86_64"
        return f"{arch}-apple-darwin"
    else:
        raise RuntimeError(f"Unsupported platform: {system}/{machine}")


def _bin_dir() -> str:
    """Return the directory where binaries should be installed."""
    # Prefer the venv/virtualenv bin dir, fall back to user bin
    if sys.prefix != sys.base_prefix:
        # Inside a virtualenv
        if platform.system() == "Windows":
            return os.path.join(sys.prefix, "Scripts")
        return os.path.join(sys.prefix, "bin")
    # Fall back to user's local bin
    return os.path.expanduser("~/.local/bin")


def install(version: str | None = None) -> str:
    """Download and install the Roche CLI binary.

    Returns the path to the installed binary.
    """
    version = version or VERSION
    target = _detect_target()
    url = (
        f"https://github.com/{GITHUB_REPO}/releases/download/"
        f"v{version}/roche-{target}.tar.gz"
    )

    print(f"Downloading Roche CLI v{version} for {target}...")
    resp = urllib.request.urlopen(url)  # noqa: S310
    data = resp.read()

    dest_dir = _bin_dir()
    os.makedirs(dest_dir, exist_ok=True)

    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tar:
        for member in tar.getmembers():
            if member.name in ("roche", "roched", "roche.exe", "roched.exe"):
                f = tar.extractfile(member)
                if f is None:
                    continue
                dest = os.path.join(dest_dir, member.name)
                with open(dest, "wb") as out:
                    shutil.copyfileobj(f, out)
                # Make executable
                os.chmod(dest, os.stat(dest).st_mode | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)
                print(f"  Installed {dest}")

    roche_path = os.path.join(dest_dir, "roche")
    print(f"Roche CLI installed to {dest_dir}")
    return roche_path


def main() -> None:
    try:
        install()
    except Exception as e:
        print(f"Warning: Failed to install Roche CLI: {e}", file=sys.stderr)
        print("You can install manually: cargo install roche-cli", file=sys.stderr)
        print(f"Or download from https://github.com/{GITHUB_REPO}/releases", file=sys.stderr)


if __name__ == "__main__":
    main()
