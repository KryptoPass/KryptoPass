"""Verify the development launcher when Cargo package and binary names differ."""

import configparser
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tools" / "dev_desktop.py"


class DevelopmentDesktopTests(unittest.TestCase):
    def test_install_and_uninstall_use_explicit_binary_from_cargo_target_directory(self):
        with (ROOT / "Cargo.toml").open("rb") as manifest:
            identity = tomllib.load(manifest)["workspace"]["metadata"]["kryptopass"]

        with tempfile.TemporaryDirectory() as directory:
            temporary = Path(directory)
            data_home = temporary / "xdg data"
            environment = {
                **os.environ,
                "XDG_DATA_HOME": str(data_home),
                "CARGO_TARGET_DIR": str(temporary / "cargo target"),
                "PYTHONDONTWRITEBYTECODE": "1",
            }
            result = subprocess.run(
                ["cargo", "metadata", "--no-deps", "--format-version", "1"],
                cwd=ROOT,
                env=environment,
                capture_output=True,
                text=True,
                timeout=60,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            metadata = json.loads(result.stdout)
            packages = [p for p in metadata["packages"] if p["name"] == "kp-desktop"]
            self.assertEqual(len(packages), 1)
            self.assertEqual(
                [t["name"] for t in packages[0]["targets"] if "bin" in t["kind"]],
                ["kryptopass-desktop"],
            )
            target_directory = Path(metadata["target_directory"])
            self.assertEqual(target_directory, temporary / "cargo target")
            expected_binary = target_directory / "debug" / "kryptopass-desktop"

            def run_cli(action):
                result = subprocess.run(
                    [sys.executable, "-B", str(SCRIPT), action],
                    cwd=ROOT,
                    env=environment,
                    capture_output=True,
                    text=True,
                    timeout=60,
                    check=False,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

            run_cli("install")

            desktop_path = data_home / "applications" / f"{identity['app-id']}.desktop"
            self.assertTrue(desktop_path.is_file())
            desktop = configparser.ConfigParser(interpolation=None)
            desktop.read(desktop_path, encoding="utf-8")
            entry = desktop["Desktop Entry"]
            self.assertEqual(entry["Type"], "Application")
            self.assertEqual(entry["X-KryptoPass-Development"], "true")
            self.assertEqual(shlex.split(entry["Exec"]), [str(expected_binary)])

            run_cli("uninstall")

            self.assertFalse(desktop_path.exists())
            self.assertEqual(list(desktop_path.parent.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
