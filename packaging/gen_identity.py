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
(out / "app_identity.rs").write_text("pub const APP: AppInfo = AppInfo {\n" + "".join(
    f"    {field}: {quote(meta[key])},\n" for field, key in fields.items()
) + "};\n")
(out / "app_identity.h").write_text("#pragma once\nnamespace kryptopass::identity {\n" + "".join(
    f"inline constexpr const char *{field} = {quote(meta[key])};\n" for field, key in fields.items()
) + "}\n")

(out / "app_id").write_text(meta["app-id"])
resources = ET.Element("RCC")
group = ET.SubElement(resources, "qresource", prefix="/" + meta["app-id"].replace(".", "/"))
ET.SubElement(group, "file", alias="icon.svg").text = str(ROOT / f"packaging/icons/{meta['app-id']}.svg")
ET.ElementTree(resources).write(out / "app_resources.qrc", encoding="utf-8")
