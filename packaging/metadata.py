"""Read the application identity shared by Cargo, Qt and installation files."""

from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def load_metadata():
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]
    package = workspace["package"]
    identity = workspace["metadata"]["kryptopass"]
    desktop = tomllib.loads((ROOT / "crates/desktop/Cargo.toml").read_text())["package"]
    return {**package, **identity, "binary": desktop["name"]}
