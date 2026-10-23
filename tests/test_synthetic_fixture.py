"""Lifecycle checks for the locally generated synthetic OpenPGP fixture."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "scripts" / "prepare_synthetic_sync_fixture.py"
FIXTURE_ID = "kp-synthetic-key-v1"


class SyntheticFixtureTests(unittest.TestCase):
    def test_generation_staging_reuse_and_corruption_rejection(self) -> None:
        if shutil.which("gpg") is None or shutil.which("gpgconf") is None:
            self.skipTest("GnuPG is required to generate the disposable fixture")

        with tempfile.TemporaryDirectory(prefix="kp-fixture-test-") as temporary:
            root = Path(temporary)
            cache = root / "cache"
            stage = root / "debug-assets"
            environment = os.environ.copy()
            environment["KP_SYNTHETIC_FIXTURE_DIR"] = str(cache)
            environment.pop("GNUPGHOME", None)

            generated = self._run_generator(cache, stage, environment)
            self.assertEqual(0, generated.returncode, "fixture generation failed")

            manifest_path = cache / "fixture.json"
            key_path = cache / "secret-key.pgp"
            manifest = self._read_manifest(manifest_path)
            key_bytes = key_path.read_bytes()
            self._assert_fixture_integrity(manifest, key_bytes)
            self.assertEqual(0o700, stat.S_IMODE(cache.stat().st_mode))
            self.assertEqual(0o600, stat.S_IMODE(key_path.stat().st_mode))

            staged_directory = stage / "synthetic-sync-v1"
            staged_manifest = self._read_manifest(staged_directory / "fixture.json")
            staged_key_path = staged_directory / "secret-key.pgp"
            staged_key = staged_key_path.read_bytes()
            self.assertTrue(manifest == staged_manifest, "staged fixture metadata differs")
            self.assertTrue(key_bytes == staged_key, "staged synthetic key differs")
            self.assertEqual(0o700, stat.S_IMODE(staged_directory.stat().st_mode))
            self.assertEqual(0o600, stat.S_IMODE(staged_key_path.stat().st_mode))
            self._assert_fixture_integrity(staged_manifest, staged_key)

            second = self._run_generator(cache, stage, environment)
            self.assertEqual(0, second.returncode, "cached fixture reuse failed")
            reused = self._read_manifest(manifest_path)
            self.assertEqual(manifest["fingerprint"], reused["fingerprint"])
            self.assertTrue(key_bytes == key_path.read_bytes(), "cached synthetic key changed")

            independent_cache = root / "independent-cache"
            independent = self._run_generator(independent_cache, None, environment)
            self.assertEqual(0, independent.returncode, "independent fixture generation failed")
            independent_manifest = self._read_manifest(independent_cache / "fixture.json")
            self.assertNotEqual(
                manifest["fingerprint"],
                independent_manifest["fingerprint"],
                "independent caches reused the same synthetic identity",
            )

            for corrupt_name, corrupt_bytes in (
                ("fixture.json", b"{invalid manifest"),
                ("secret-key.pgp", b"corrupt synthetic fixture"),
            ):
                with self.subTest(corrupt_file=corrupt_name):
                    corrupt_cache = root / f"corrupt-{corrupt_name.replace('.', '-') }"
                    shutil.copytree(cache, corrupt_cache)
                    corrupt_path = corrupt_cache / corrupt_name
                    corrupt_path.write_bytes(corrupt_bytes)
                    before = corrupt_path.read_bytes()

                    rejected = self._run_generator(corrupt_cache, None, environment)

                    self.assertNotEqual(0, rejected.returncode, "corrupt fixture was accepted")
                    self.assertEqual(before, corrupt_path.read_bytes(), "corrupt fixture was replaced")

    def _run_generator(
        self,
        cache: Path,
        stage: Path | None,
        environment: dict[str, str],
    ) -> subprocess.CompletedProcess[bytes]:
        command = ["python3", str(GENERATOR)]
        if stage is not None:
            command.extend(["--stage", str(stage)])
        run_environment = environment.copy()
        run_environment["KP_SYNTHETIC_FIXTURE_DIR"] = str(cache)
        return subprocess.run(
            command,
            cwd=ROOT,
            env=run_environment,
            capture_output=True,
            timeout=60,
            check=False,
        )

    def _read_manifest(self, path: Path) -> dict[str, object]:
        return json.loads(path.read_text(encoding="utf-8"))

    def _assert_fixture_integrity(self, manifest: dict[str, object], key_bytes: bytes) -> None:
        self.assertEqual(FIXTURE_ID, manifest.get("fixture_id"))
        self.assertRegex(str(manifest.get("fingerprint", "")), re.compile(r"^[0-9A-Fa-f]{40}$"))
        expected_sha256 = str(manifest.get("sha256", ""))
        self.assertRegex(expected_sha256, re.compile(r"^[0-9a-f]{64}$"))
        self.assertGreater(len(key_bytes), 0)
        self.assertEqual(len(key_bytes), manifest.get("size"))
        self.assertEqual(hashlib.sha256(key_bytes).hexdigest(), expected_sha256)
        self.assertTrue(str(manifest.get("passphrase", "")))
        self.assertTrue(str(manifest.get("warning", "")))


if __name__ == "__main__":
    unittest.main()
