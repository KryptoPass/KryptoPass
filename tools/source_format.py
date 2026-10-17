#!/usr/bin/env python3
"""Check or apply the repository's KDE C++ and Qt QML formatters."""

import argparse
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "packaging"))
from metadata import qt_tool


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("language", choices=("cpp", "qml"))
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    desktop = ROOT / "crates/kp-desktop"
    if args.language == "cpp":
        version = subprocess.check_output(["clang-format", "--version"], text=True)
        if not re.search(r"version 23\.", version):
            raise RuntimeError("KDE C++ formatting requires clang-format 23")
        paths = sorted(
            path
            for directory in (desktop / "src/native", desktop / "native-tests")
            for path in directory.rglob("*")
            if path.suffix in (".cpp", ".h")
        )
        command = ["clang-format", f"--style=file:{ROOT / '.clang-format'}"]
    else:
        paths = sorted((desktop / "ui").rglob("*.qml"))
        command = [str(qt_tool("qmlformat")), "--settings", str(ROOT / ".qmlformat.ini")]

    failed = False
    for path in paths:
        result = subprocess.run([*command, str(path)], capture_output=True, check=True)
        if result.stderr:
            sys.stderr.buffer.write(result.stderr)
        if result.stdout != path.read_bytes():
            if args.write:
                path.write_bytes(result.stdout)
            else:
                print(f"Formatting differs: {path.relative_to(ROOT)}", file=sys.stderr)
                failed = True
    if failed:
        print("Run `just format` to apply the repository style.", file=sys.stderr)
    return int(failed)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Source formatting: {error}", file=sys.stderr)
        raise SystemExit(1) from error
