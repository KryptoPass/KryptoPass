#!/usr/bin/env python3
"""Render install files from Cargo.toml metadata (single source of truth).

Reads name/version/description/license of kryptopass-desktop via
`cargo metadata` and fills packaging/*.in templates into packaging/gen/.
Never edit gen/ by hand: fix the template or Cargo.toml and re-run.
"""

import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent
GEN = ROOT / "gen"
APP_ID = "org.kryptopass.desktop"

FIELDS = subprocess.run(
    [
        "cargo",
        "metadata",
        "--no-deps",
        "--format-version",
        "1",
        "--filter-platform",
        "x86_64-unknown-linux-gnu",
    ],
    cwd=ROOT.parent,
    capture_output=True,
    text=True,
    check=True,
).stdout
pkg = next(
    p for p in json.loads(FIELDS)["packages"] if p["name"] == "kryptopass-desktop"
)

release_date = os.environ.get("RELEASE_DATE")
if release_date is None:
    release_date = datetime.now(timezone.utc).date().isoformat()
    print("RELEASE_DATE unset: using today (UTC) for the release stanza", file=sys.stderr)

tokens = {
    "@APP_ID@": APP_ID,
    "@APP_NAME@": "KryptoPass",
    "@APP_VERSION@": pkg["version"],
    "@APP_COMMENT@": pkg["description"],
    "@APP_BIN@": pkg["name"],
    "@APP_ICON@": APP_ID,  # pending real asset under packaging/icons/
    "@APP_LICENSE@": pkg["license"],
    "@RELEASE_DATE@": release_date,
}

GEN.mkdir(exist_ok=True)
for template in sorted(ROOT.glob("*.in")):
    text = template.read_text(encoding="utf-8")
    for key, value in tokens.items():
        text = text.replace(key, value)
    leftovers = [line for line in text.splitlines() if "@APP_" in line]
    if leftovers:
        print(f"unreplaced tokens in {template.name}: {leftovers}", file=sys.stderr)
        sys.exit(1)
    out = GEN / template.name.removesuffix(".in")
    out.write_text(text, encoding="utf-8")
    print(f"wrote {out.relative_to(ROOT.parent)}")
