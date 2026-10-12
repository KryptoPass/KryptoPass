#!/usr/bin/env python3
"""Capture the real application using an empty, isolated user profile."""
import os
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory
from metadata import ROOT

output = ROOT / "packaging/screenshots/vault.png"
output.parent.mkdir(parents=True, exist_ok=True)
with TemporaryDirectory(prefix="kryptopass-screenshot-") as temporary:
    profile = Path(temporary)
    environment = dict(os.environ, QT_QPA_PLATFORM="offscreen", QT_QUICK_BACKEND="software", LANG="es_ES.UTF-8", LC_ALL="C.UTF-8")
    for variable, name in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"), ("GNUPGHOME", "gnupg")]:
        path = profile / name
        path.mkdir(mode=0o700)
        environment[variable] = str(path)
    subprocess.run([str(ROOT / "target/debug/kryptopass-desktop"), "--screenshot", str(output)], env=environment, check=True, timeout=20)
