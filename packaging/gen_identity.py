#!/usr/bin/env python3
"""Generate native identity constants from the workspace manifest."""
import json
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
from metadata import ROOT, load_metadata

meta = load_metadata()
out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
fields = {
    "display_name": "display-name", "version": "version", "organization": "organization",
    "organization_domain": "organization-domain", "desktop_file_id": "app-id",
    "description": "summary", "license": "license", "authors": "developer-name",
    "copyright": "copyright", "homepage": "homepage", "repository": "repository",
}
quote = lambda s: json.dumps(s, ensure_ascii=False)


def write_if_changed(name, content):
    path = out / name
    if not path.is_file() or path.read_text(encoding="utf-8") != content:
        path.write_text(content, encoding="utf-8")


write_if_changed("app_identity.rs", "pub const APP: AppInfo = AppInfo {\n" + "".join(
    f"    {field}: {quote(meta[key])},\n" for field, key in fields.items()
) + "};\n")
write_if_changed("app_identity.h", "#pragma once\nnamespace kryptopass::identity {\n" + "".join(
    f"inline constexpr const char *{field} = {quote(meta[key])};\n" for field, key in fields.items()
) + "}\n")

write_if_changed("app_id", meta["app-id"])
resources = ET.Element("RCC")
group = ET.SubElement(resources, "qresource", prefix="/" + meta["app-id"].replace(".", "/"))
ET.SubElement(group, "file", alias="icon.svg").text = str(ROOT / f"packaging/icons/{meta['app-id']}.svg")
write_if_changed("app_resources.qrc", ET.tostring(resources, encoding="unicode"))
