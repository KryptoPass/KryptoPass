"""Keep installed desktop metadata consistent with the Cargo manifest."""

import configparser
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import unittest
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]
GENERATOR = ROOT / "packaging" / "gen_install.py"


class PackagingMetadataTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with (ROOT / "Cargo.toml").open("rb") as manifest:
            workspace = tomllib.load(manifest)["workspace"]
        cls.identity = workspace["metadata"]["kryptopass"]
        cls.package = workspace["package"]
        with (ROOT / "crates" / "kp-desktop" / "Cargo.toml").open("rb") as manifest:
            cls.binary = tomllib.load(manifest)["bin"][0]["name"]

    def generate(self, output, *arguments):
        return subprocess.run(
            [sys.executable, str(GENERATOR), "--output", str(output), *arguments],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )

    def assert_success(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_install_files_match_manifest_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            self.assert_success(self.generate(output))

            app_id = self.identity["app-id"]
            desktop = configparser.ConfigParser(interpolation=None)
            desktop.read(output / f"{app_id}.desktop", encoding="utf-8")
            entry = desktop["Desktop Entry"]
            self.assertEqual(entry["Name"], self.identity["display-name"])
            self.assertEqual(entry["Icon"], app_id)
            self.assertEqual(entry["Exec"], f"{self.binary} %f")
            self.assertEqual(entry["MimeType"], "application/vnd.kryptopass.backup;")
            self.assertEqual(entry["X-DocPath"], f"{app_id}/index.html")
            self.assertEqual(entry["Actions"], "NewEntry;Search;")
            self.assertEqual(desktop["Desktop Action NewEntry"]["Exec"], f"{self.binary} --new-entry")
            self.assertEqual(desktop["Desktop Action Search"]["Exec"], f"{self.binary} --search")

            mime = ET.parse(output / f"{app_id}.mime.xml").getroot()
            namespace = {"mime": "http://www.freedesktop.org/standards/shared-mime-info"}
            backup = mime.find("mime:mime-type[@type='application/vnd.kryptopass.backup']", namespace)
            self.assertIsNotNone(backup)
            self.assertEqual(backup.find("mime:glob", namespace).get("pattern"), "*.kpb")

            component = ET.parse(output / f"{app_id}.metainfo.xml").getroot()
            self.assertEqual(component.findtext("id"), app_id)
            self.assertEqual(component.findtext("name"), self.identity["display-name"])
            self.assertEqual(component.findtext("summary"), self.identity["summary"])
            self.assertEqual(component.findtext("project_license"), self.package["license"])
            self.assertEqual(
                component.findtext("url[@type='homepage']"), self.package["homepage"]
            )
            self.assertEqual(
                component.findtext("launchable[@type='desktop-id']"), f"{app_id}.desktop"
            )
            self.assertEqual(component.findtext("provides/binary"), self.binary)
            self.assertEqual(
                component.findtext("provides/mediatype"), "application/vnd.kryptopass.backup"
            )
            release = component.find("releases/release")
            self.assertIsNotNone(release)
            self.assertEqual(release.get("version"), self.package["version"])
            self.assertEqual(release.get("date"), self.identity["release-date"])

    def test_check_detects_stale_install_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            self.assert_success(self.generate(output))
            self.assert_success(self.generate(output, "--check"))

            desktop = output / f"{self.identity['app-id']}.desktop"
            original = desktop.read_text(encoding="utf-8")
            stale = original.replace(
                f"Name={self.identity['display-name']}", "Name=Outdated application name"
            )
            self.assertNotEqual(stale, original)
            desktop.write_text(stale, encoding="utf-8")

            result = self.generate(output, "--check")
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(desktop.read_text(encoding="utf-8"), stale)


if __name__ == "__main__":
    unittest.main()
