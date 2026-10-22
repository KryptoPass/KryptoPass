"""Exercise the compiled Spanish catalog through Qt's runtime translator."""

from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]
PROBE_SOURCE = r"""
#include <QCoreApplication>
#include <QTranslator>
#include <cstdio>

int main(int argc, char **argv)
{
    QCoreApplication application(argc, argv);
    if (argc != 5)
        return 2;

    QTranslator translator;
    if (!translator.load(QString::fromUtf8(argv[1]))) {
        std::fputs("Could not load the compiled catalog", stderr);
        return 3;
    }
    if (!application.installTranslator(&translator))
        return 4;

    const int count = QString::fromUtf8(argv[4]).toInt();
    if (translator.translate(argv[2], argv[3], nullptr, count).isEmpty()) {
        std::fputs("Message missing from the compiled catalog", stderr);
        return 5;
    }
    const QByteArray text =
        QCoreApplication::translate(argv[2], argv[3], nullptr, count).toUtf8();
    return std::fwrite(text.constData(), 1, text.size(), stdout) == text.size() ? 0 : 6;
}
"""


class TranslationTests(unittest.TestCase):
    @staticmethod
    def run_command(command, **arguments):
        result = subprocess.run(
            command,
            capture_output=True,
            encoding="utf-8",
            check=False,
            **arguments,
        )
        if result.returncode != 0:
            raise AssertionError(
                f"Command failed ({result.returncode}): {command}\n"
                f"{result.stdout}{result.stderr}"
            )
        return result

    @classmethod
    def setUpClass(cls):
        temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(temporary.cleanup)
        directory = Path(temporary.name)
        catalog_directory = directory / "translations"
        cls.run_command(
            [
                sys.executable,
                str(ROOT / "packaging" / "translations.py"),
                "compile",
                "--output",
                str(catalog_directory),
            ],
            cwd=ROOT,
        )
        with (ROOT / "Cargo.toml").open("rb") as manifest:
            app_id = tomllib.load(manifest)["workspace"]["metadata"]["kryptopass"][
                "app-id"
            ]
        cls.catalog = catalog_directory / f"{app_id}_es.qm"

        source = directory / "translation_probe.cpp"
        source.write_text(PROBE_SOURCE, encoding="utf-8")
        cls.probe = directory / "translation_probe"
        flags = shlex.split(
            cls.run_command(["pkg-config", "--cflags", "--libs", "Qt6Core"]).stdout
        )
        cls.run_command(
            ["c++", "-std=c++17", "-fPIC", str(source), "-o", str(cls.probe), *flags]
        )

    def translate(self, context, source, count=-1):
        return self.run_command(
            [str(self.probe), str(self.catalog), context, source, str(count)]
        ).stdout

    def test_main_counts_use_spanish_plural_rules(self):
        for singular, plural in (("elemento", "elementos"), ("clave", "claves")):
            for count in (0, 1, 2):
                with self.subTest(word=singular, count=count):
                    expected = singular if count == 1 else plural
                    self.assertEqual(
                        self.translate("Main", f"%n {singular}", count),
                        f"{count} {expected}",
                    )

    def test_backend_import_counts_use_spanish_plural_rules(self):
        source = "Importación completada: %n elemento añadido a la bóveda actual."
        for count in (0, 1, 2):
            with self.subTest(count=count):
                phrase = "elemento añadido" if count == 1 else "elementos añadidos"
                self.assertEqual(
                    self.translate("Backend", source, count),
                    f"Importación completada: {count} {phrase} a la bóveda actual.",
                )

    def test_translation_preserves_parameter_placeholder(self):
        self.assertEqual(self.translate("ImportWindow", "Desde %1"), "Desde %1")


if __name__ == "__main__":
    unittest.main()
