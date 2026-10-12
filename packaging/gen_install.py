#!/usr/bin/env python3
"""Generate deterministic desktop and AppStream files from Cargo.toml."""

import argparse
from html import escape
from pathlib import Path
import sys

from metadata import ROOT, load_metadata


def render():
    meta = load_metadata()
    values = {
        "APP_ID": meta["app-id"], "APP_NAME": meta["display-name"],
        "APP_VERSION": meta["version"], "APP_COMMENT": meta["summary"],
        "APP_BIN": meta["binary"], "APP_ICON": meta["app-id"],
        "APP_LICENSE": meta["license"], "RELEASE_DATE": meta["release-date"],
        "APP_HOMEPAGE": meta["homepage"], "APP_REPOSITORY": meta["repository"],
        "DEVELOPER_ID": meta["developer-id"], "DEVELOPER_NAME": meta["developer-name"],
    }
    for template in sorted((ROOT / "packaging").glob("*.in")):
        content = template.read_text()
        for key, value in values.items():
            encoded = escape(value, quote=True) if template.name.endswith("xml.in") else (
                value.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r")
            )
            content = content.replace(f"@{key}@", encoded)
        if "@APP_" in content or "@DEVELOPER_" in content or "@RELEASE_" in content:
            raise ValueError(f"Unresolved metadata in {template.name}")
        suffix = ".metainfo.xml" if template.name.endswith("xml.in") else ".desktop"
        yield meta["app-id"] + suffix, content


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "packaging/gen")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    stale = []
    if not args.check:
        args.output.mkdir(parents=True, exist_ok=True)
    for name, content in render():
        path = args.output / name
        if args.check:
            if not path.exists() or path.read_text() != content:
                stale.append(str(path))
        else:
            path.write_text(content)
    if stale:
        print("Stale installation metadata: " + ", ".join(stale), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
