#!/usr/bin/env python3
"""Install or remove KryptoPass's per-user development launcher."""

import argparse
import json
import os
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

from metadata import ROOT, load_metadata
MARKER = "X-KryptoPass-Development=true"


def entry_source():
    templates = list((ROOT / "packaging").glob("*.desktop.in"))
    if len(templates) != 1:
        raise RuntimeError("expected one .desktop.in template")
    template = templates[0]
    return load_metadata()["app-id"], template


def app_metadata():
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    )
    metadata = json.loads(result.stdout)
    desktop_name = load_metadata()["binary"]
    binaries = [(p, t) for p in metadata["packages"] if p["name"] == desktop_name
                for t in p["targets"] if "bin" in t["kind"] and t["name"] == desktop_name]
    if len(binaries) != 1:
        raise RuntimeError("expected one desktop binary in Cargo metadata")
    package, target = binaries[0]
    target_dir = Path(metadata["target_directory"])
    if not target_dir.is_absolute():
        target_dir = ROOT / target_dir
    return package, target_dir / "debug" / target["name"]



def keyfile_escape(value):
    return value.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r")


def exec_value(binary):
    value = str(binary)
    if not binary.is_absolute() or any(ord(c) < 32 or ord(c) == 127 for c in value):
        raise RuntimeError("invalid development executable path")
    quoted = '"' + "".join(
        "%%" if c == "%" else "\\" + c if c in {'\\', '"', "$", "`"} else c
        for c in value
    ) + '"'
    return keyfile_escape(quoted)


def rendered_entry(app_id, template):
    package, binary = app_metadata()
    description = load_metadata()["summary"]
    values = {
        "@APP_NAME@": keyfile_escape(load_metadata()["display-name"]),
        "@APP_COMMENT@": keyfile_escape(description),
        "@APP_BIN@": exec_value(binary),
        "@APP_ICON@": app_id,
    }
    content = template.read_text(encoding="utf-8")
    for placeholder, value in values.items():
        content = content.replace(placeholder, value)
    if "@APP_" in content or not content.startswith("[Desktop Entry]\n"):
        raise RuntimeError("invalid desktop template")
    return content.replace("[Desktop Entry]\n", f"[Desktop Entry]\n{MARKER}\n", 1)


def applications_dir(create):
    data_home = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
    if not data_home.is_absolute() or ".." in data_home.parts:
        raise RuntimeError("XDG_DATA_HOME must be absolute and contain no '..'")
    directory = data_home / "applications"
    if create:
        directory.mkdir(parents=True, exist_ok=True)
    try:
        mode = directory.lstat().st_mode
    except FileNotFoundError:
        return None
    if not stat.S_ISDIR(mode):
        raise RuntimeError("applications path is not a real directory")
    return directory


def read_entry(path):
    try:
        before = path.lstat()
    except FileNotFoundError:
        return None
    if not stat.S_ISREG(before.st_mode):
        raise RuntimeError("desktop entry is not a regular file")
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    with os.fdopen(descriptor, "rb") as entry:
        opened = os.fstat(entry.fileno())
        if (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
            raise RuntimeError("desktop entry changed while opening")
        content = entry.read().decode("utf-8", errors="replace")
    lines = content.splitlines()
    fields, owned, in_section = {}, False, False
    for line in lines:
        if line.startswith("[") and line.endswith("]"):
            in_section = line == "[Desktop Entry]"
        elif in_section:
            owned |= line == MARKER
            if "=" in line and not line.startswith("#"):
                key, value = line.split("=", 1)
                fields[key] = value
    valid = fields.get("Type") == "Application" and bool(fields.get("Name")) and bool(fields.get("Exec"))
    return owned, valid


def write_atomic(directory, path, content, replace):
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=directory)
    try:
        os.fchmod(descriptor, 0o644)
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            output.write(content)
            output.flush()
            os.fsync(output.fileno())
        if replace:
            os.replace(temporary, path)
        else:
            os.link(temporary, path, follow_symlinks=False)
    finally:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass


def install():
    app_id, template = entry_source()
    directory = applications_dir(create=True)
    path = directory / f"{app_id}.desktop"
    existing = read_entry(path)
    if existing:
        owned, valid = existing
        if not owned:
            if valid:
                print(f"Preserved existing desktop entry: {path.name}")
                return
            raise RuntimeError(f"refusing to overwrite unowned entry: {path.name}")
    write_atomic(directory, path, rendered_entry(app_id, template), replace=existing is not None)
    print(f"Installed development desktop entry: {path.name}")


def uninstall():
    app_id, _template = entry_source()
    directory = applications_dir(create=False)
    path = directory / f"{app_id}.desktop" if directory else None
    existing = read_entry(path) if path else None
    if existing and existing[0]:
        path.unlink()
        print(f"Removed development desktop entry: {path.name}")
    else:
        print("No development desktop entry is installed; unowned entries are preserved.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("install", "uninstall"))
    try:
        install() if parser.parse_args().action == "install" else uninstall()
    except Exception as error:
        print(f"desktop development integration failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
