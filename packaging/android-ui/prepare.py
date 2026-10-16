"""Prepare existing QML and icons for the explicit Rust demo composition."""

import json
from pathlib import Path
import re
import subprocess
import sys
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "packaging"))
from metadata import load_metadata


def write_if_changed(path, content):
    if not path.is_file() or path.read_text(encoding="utf-8") != content:
        path.write_text(content, encoding="utf-8")


def prepare(output):
    output.mkdir(parents=True, exist_ok=True)
    meta = load_metadata()
    subprocess.run([sys.executable, str(ROOT / "packaging/gen_identity.py"), str(output)], check=True)
    identity = {"QML_URI": meta["app-id"], "BACKEND_URI": meta["app-id"] + ".previewbackend", "PACKAGE": meta["app-id"] + ".uipreview",
                "NAME": meta["display-name"], "VERSION": meta["version"]}
    write_if_changed(output / "identity.cmake", "".join(
        f'set(PREVIEW_{key} {json.dumps(value)})\n' for key, value in identity.items()))
    sources = list((ROOT / "crates/desktop/ui/qml").glob("*.qml"))
    sources.append(Path(__file__).with_name("BridgeSmoke.qml"))
    names = {source.name for source in sources}
    for stale in output.glob("*.qml"):
        if stale.name not in names:
            stale.unlink()
    for source in sources:
        content = source.read_text().replace(
            f'import {meta["app-id"]} as Backend',
            f'import {identity["BACKEND_URI"]} as Backend')
        content = re.sub(rf'^import {re.escape(meta["app-id"])}$',
                         f'import {meta["app-id"]}\nimport {identity["BACKEND_URI"]}',
                         content, flags=re.MULTILINE)
        content = content.replace("import org.kryptopass.desktop.previewbackend as Backend",
                                  f'import {identity["BACKEND_URI"]} as Backend')
        write_if_changed(output / source.name, content)
    theme = Path("/usr/share/icons/breeze")
    if not theme.is_dir():
        raise FileNotFoundError("The UI preview needs the host Breeze icon theme")
    resources = ET.Element("RCC")
    group = ET.SubElement(resources, "qresource", prefix="/icons/breeze")
    index = output / "index.theme"
    write_if_changed(index, "[Icon Theme]\nName=Breeze\nDirectories=icons\nFollowsColorScheme=true\n[icons]\nSize=32\nType=Scalable\n")
    ET.SubElement(group, "file", alias="index.theme").text = str(index)
    used = set()
    for source in output.glob("*.qml"):
        used.update(re.findall(r'"([a-z][a-z0-9-]+)"', source.read_text()))
    used.update(["folder", "folder-symbolic", "edit-clear", "go-next", "go-previous", "dialog-close"])
    by_name = {}
    for path in theme.rglob("*.svg"):
        by_name.setdefault(path.stem, []).append(path)
    for icon in sorted(used):
        candidates = sorted(by_name.get(icon, []))
        if candidates:
            preferred = next((path for path in candidates if "/32/" in str(path)), candidates[0])
            ET.SubElement(group, "file", alias=f"icons/{icon}.svg").text = str(preferred)
    write_if_changed(output / "icons.qrc", ET.tostring(resources, encoding="unicode"))


if __name__ == "__main__":
    prepare(Path(sys.argv[1]))
