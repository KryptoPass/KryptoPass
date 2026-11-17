#!/usr/bin/env python3
"""Run clang-tidy on kp-desktop's native C++ with the flags Cargo compiles it with.

`cc` does not write a compilation database, so the C++ build is captured once
with `intercept-build` and only the sources under `src/native` are kept.
"""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "packaging"))

from metadata import cargo_target_directory

NATIVE = ROOT / "crates/kp-desktop/src/native"
BUILD_SCRIPT = ROOT / "crates/kp-desktop/build.rs"


def require(tool: str) -> None:
    if shutil.which(tool) is None:
        sys.exit(f"cxx-tidy: falta `{tool}` (paquete clang)")


def capture(output: Path) -> None:
    # Touching build.rs makes Cargo rerun it, so `cc` compiles every source again.
    BUILD_SCRIPT.touch()
    captured = output / "all.json"
    subprocess.run(
        ["intercept-build", "--cdb", str(captured), "cargo", "build", "-p", "kp-desktop"],
        cwd=ROOT,
        check=True,
    )
    entries = [
        entry
        for entry in json.loads(captured.read_text(encoding="utf-8"))
        if Path(entry["file"]).is_relative_to(NATIVE)
    ]
    if not entries:
        sys.exit("cxx-tidy: el build no compiló ningún archivo de src/native")
    (output / "compile_commands.json").write_text(json.dumps(entries, indent=1), encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reuse", action="store_true", help="usa la base de compilación ya capturada")
    arguments = parser.parse_args()
    for tool in ("intercept-build", "run-clang-tidy", "clang-tidy"):
        require(tool)

    output = cargo_target_directory() / "cxx-tidy"
    output.mkdir(parents=True, exist_ok=True)
    if not arguments.reuse or not (output / "compile_commands.json").exists():
        capture(output)
    # GCC-only warnings such as -Wlogical-op are unknown to clang-tidy's Clang.
    return subprocess.run(
        ["run-clang-tidy", "-p", str(output), "-quiet", "-extra-arg=-Wno-unknown-warning-option", str(NATIVE)],
        cwd=ROOT,
    ).returncode


if __name__ == "__main__":
    sys.exit(main())
