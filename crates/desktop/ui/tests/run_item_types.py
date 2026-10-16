#!/usr/bin/env python3
"""Run pure QML contracts against the module produced by `just check`.

The CXX-Qt plugin is linked into the application and cannot be loaded by
qmltestrunner. The Rust bridge is verified separately by application checks.
"""

import os
import shutil
import subprocess
import sys
from pathlib import Path
from tempfile import TemporaryDirectory


ROOT = Path(__file__).resolve().parents[4]
MODULE_PATH = Path("org/kryptopass/desktop")
GENERATED_MODULE = ROOT / "target/cxxqt/qml_modules" / MODULE_PATH


def prepare_module(destination: Path) -> None:
    qmldir = GENERATED_MODULE / "qmldir"
    if not qmldir.is_file():
        raise FileNotFoundError(f"Missing {qmldir}; run `just check` first")

    for source in sorted(GENERATED_MODULE.rglob("*.qml")):
        target = destination / source.relative_to(GENERATED_MODULE)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)

    lines = []
    for line in qmldir.read_text(encoding="utf-8").splitlines(keepends=True):
        tokens = line.split()
        if tokens[:2] == ["optional", "plugin"]:
            continue
        if tokens[:1] == ["classname"]:
            continue
        if (
            tokens[:1] == ["prefer"]
            and len(tokens) >= 2
            and tokens[1].startswith((":/", "qrc:/"))
        ):
            continue
        lines.append(line)
    shutil.copy2(
        Path(__file__).resolve().parent / "fixtures" / "MarkdownDocument.qml",
        destination / "MarkdownDocument.qml",
    )
    if not any(line.split()[:1] == ["MarkdownDocument"] for line in lines):
        lines.append("MarkdownDocument 1.0 MarkdownDocument.qml\n")
    (destination / "qmldir").write_text("".join(lines), encoding="utf-8")


def main() -> int:
    try:
        qt_bins = subprocess.run(
            ["qmake6", "-query", "QT_HOST_BINS"],
            check=True,
            stdout=subprocess.PIPE,
            text=True,
        ).stdout.strip()
        runner = Path(qt_bins) / "qmltestrunner"
        environment = os.environ.copy()
        environment.update(
            QT_QPA_PLATFORM="offscreen",
            QT_QUICK_CONTROLS_STYLE="org.kde.desktop",
            QT_FORCE_STDERR_LOGGING="1",
            LANG="C.UTF-8",
            LC_ALL="C.UTF-8",
        )
        with TemporaryDirectory(prefix="kryptopass-qml-tests-") as temporary:
            modules = Path(temporary)
            prepare_module(modules / MODULE_PATH)
            return subprocess.run(
                [
                    str(runner),
                    "-import",
                    str(modules),
                    "-input",
                    str(Path(__file__).resolve().parent),
                    *sys.argv[1:],
                ],
                cwd=ROOT,
                env=environment,
                check=False,
            ).returncode
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"QML contract runner: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
