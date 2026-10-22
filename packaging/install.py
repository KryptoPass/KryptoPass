#!/usr/bin/env python3
"""Install a built application and its desktop resources into a staging prefix."""
import argparse
from pathlib import Path
import shutil
import subprocess
from metadata import ROOT, desktop_binary, load_metadata

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--prefix", type=Path, required=True)
parser.add_argument("--binary", type=Path)
args = parser.parse_args()
if args.binary is None:
    args.binary = desktop_binary("release")
meta = load_metadata()
subprocess.run(["python3", str(ROOT / "packaging/gen_install.py"), "--check"], check=True)
if not args.binary.is_file():
    parser.error("build the release binary with just build --release first, or pass --binary")
files = {
    args.binary: f"bin/{meta['binary']}",
    ROOT / f"packaging/gen/{meta['app-id']}.desktop": f"share/applications/{meta['app-id']}.desktop",
    ROOT / f"packaging/gen/{meta['app-id']}.metainfo.xml": f"share/metainfo/{meta['app-id']}.metainfo.xml",
    ROOT / f"packaging/gen/{meta['app-id']}.mime.xml": f"share/mime/packages/{meta['app-id']}.xml",
    ROOT / "doc/index.docbook": f"share/doc/HTML/es/{meta['app-id']}/index.docbook",
    ROOT / f"packaging/icons/{meta['app-id']}.svg": f"share/icons/hicolor/scalable/apps/{meta['app-id']}.svg",
    ROOT / "COPYING": f"share/licenses/{meta['app-id']}/COPYING",
}
for source, relative in files.items():
    target = args.prefix / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)
