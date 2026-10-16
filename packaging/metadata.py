"""Read the application identity shared by Cargo, Qt and installation files."""

from pathlib import Path
import json
import os
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def cargo_target_directory():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=ROOT, text=True,
    ))
    return Path(metadata["target_directory"])


def desktop_binary(profile="debug"):
    return cargo_target_directory() / profile / load_metadata()["binary"]


def qt_query(key):
    return subprocess.check_output(
        [os.environ.get("QMAKE") or "qmake6", "-query", key], text=True,
    ).strip()


def qt_tool(name):
    return str(Path(qt_query("QT_HOST_BINS")) / name)


def load_metadata():
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]
    package = workspace["package"]
    identity = workspace["metadata"]["kryptopass"]
    desktop = tomllib.loads((ROOT / "crates/desktop/Cargo.toml").read_text())["package"]
    return {**package, **identity, "binary": desktop["name"]}
