"""Verify user-scoped browser native-host registration."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
TOOLS = ROOT / "tools"
sys.path.insert(0, str(TOOLS))

import browser_integration  # noqa: E402


class BrowserNativeHostTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with (ROOT / "Cargo.toml").open("rb") as manifest:
            cls.identity = tomllib.load(manifest)["workspace"]["metadata"]["kryptopass"]

    def make_proxy(self, directory, capture=None):
        proxy = Path(directory) / "fake proxy's executable"
        script = "#!/bin/sh\n"
        if capture is not None:
            script += f"printf '%s\\n' \"$@\" > {str(capture)!r}\n"
        proxy.write_text(script, encoding="utf-8")
        proxy.chmod(0o700)
        return proxy

    def test_firefox_registration_uses_cargo_identity_and_only_allows_our_extension(self):
        with tempfile.TemporaryDirectory() as directory:
            temporary = Path(directory)
            home = temporary / "user home"
            proxy = self.make_proxy(temporary)

            manifest_path = browser_integration.install(proxy, "firefox", home=home)

            expected_hosts = home / ".mozilla/native-messaging-hosts"
            self.assertEqual(manifest_path, expected_hosts / f"{self.identity['browser-host-id']}.json")
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertEqual(manifest["name"], self.identity["browser-host-id"])
            self.assertEqual(manifest["description"], self.identity["display-name"] + " Browser")
            self.assertEqual(manifest["type"], "stdio")
            self.assertEqual(manifest["path"], str(expected_hosts / f"{self.identity['browser-host-id']}-launcher"))
            self.assertEqual(manifest["allowed_extensions"], [self.identity["browser-extension-id"]])
            self.assertNotIn("allowed_origins", manifest)

    def test_chromium_family_uses_browser_specific_xdg_paths_and_exact_extension_origins(self):
        with tempfile.TemporaryDirectory() as directory:
            temporary = Path(directory)
            home = temporary / "user home"
            config = temporary / "custom config"
            proxy = self.make_proxy(temporary)
            browsers = [
                ("chromium", "chromium", "a" * 32),
                ("brave", "BraveSoftware/Brave-Browser", "b" * 32),
                ("brave-origin", "BraveSoftware/Brave-Origin", "c" * 32),
            ]

            with mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": str(config)}):
                for browser, directory_name, extension_id in browsers:
                    with self.subTest(browser=browser):
                        manifest_path = browser_integration.install(
                            proxy, browser, extension_id=extension_id, home=home
                        )

                        expected_hosts = config / directory_name / "NativeMessagingHosts"
                        self.assertEqual(
                            manifest_path,
                            expected_hosts / f"{self.identity['browser-host-id']}.json",
                        )
                        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
                        self.assertEqual(manifest["name"], self.identity["browser-host-id"])
                        self.assertEqual(
                            manifest["description"], self.identity["display-name"] + " Browser"
                        )
                        self.assertEqual(manifest["type"], "stdio")
                        self.assertEqual(
                            manifest["allowed_origins"],
                            [f"chrome-extension://{extension_id}/"],
                        )
                        self.assertNotIn("allowed_extensions", manifest)

            self.assertFalse(home.exists())

    def test_all_non_firefox_browsers_reject_invalid_extension_ids_before_registration(self):
        with tempfile.TemporaryDirectory() as directory:
            temporary = Path(directory)
            home = temporary / "user home"
            config = temporary / "custom config"
            proxy = self.make_proxy(temporary)

            with mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": str(config)}):
                for browser in ("chromium", "brave", "brave-origin"):
                    with self.subTest(browser=browser):
                        with self.assertRaisesRegex(ValueError, "32-character ID"):
                            browser_integration.install(
                                proxy,
                                browser,
                                extension_id="invalid-extension-id",
                                home=home,
                            )

            self.assertFalse(config.exists())
            self.assertFalse(home.exists())

    def test_launcher_quotes_proxy_path_and_forwards_arguments(self):
        with tempfile.TemporaryDirectory() as directory:
            temporary = Path(directory)
            capture = temporary / "captured arguments"
            proxy = self.make_proxy(temporary, capture)
            home = temporary / "user home"
            runtime = temporary / "runtime"
            manifest_path = browser_integration.install(proxy, "firefox", home=home)
            launcher = manifest_path.parent / f"{self.identity['browser-host-id']}-launcher"

            result = subprocess.run(
                [str(launcher), "--probe"],
                cwd=temporary,
                env={**os.environ, "XDG_RUNTIME_DIR": str(runtime)},
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(
                capture.read_text(encoding="utf-8").splitlines(),
                ["--socket", str(runtime / self.identity["app-id"] / "browser.sock"), "--probe"],
            )

    def test_install_refuses_to_overwrite_existing_registration(self):
        with tempfile.TemporaryDirectory() as directory:
            temporary = Path(directory)
            home = temporary / "user home"
            hosts = home / ".mozilla/native-messaging-hosts"
            hosts.mkdir(parents=True)
            existing_manifest = hosts / f"{self.identity['browser-host-id']}.json"
            existing_manifest.write_text("existing registration\n", encoding="utf-8")
            proxy = self.make_proxy(temporary)

            with self.assertRaises(FileExistsError):
                browser_integration.install(proxy, "firefox", home=home)

            self.assertEqual(existing_manifest.read_text(encoding="utf-8"), "existing registration\n")
            self.assertFalse((hosts / f"{self.identity['browser-host-id']}-launcher").exists())


if __name__ == "__main__":
    unittest.main()
