#!/usr/bin/env python3
"""Extract Qt catalogs, check freshness, and compile resource-embedded translations."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile
import xml.etree.ElementTree as ET
from metadata import ROOT, load_metadata

CATALOG = ROOT / "po/es.ts"
SPANISH_PLURALS = {
    "%n clave": "%n claves",
    "%n elemento": "%n elementos",
    "%n clave GPG cargada": "%n claves GPG cargadas",
    "Importación completada: %n elemento añadido a la bóveda actual.":
        "Importación completada: %n elementos añadidos a la bóveda actual.",
}


def qt_tool(name):
    binaries = subprocess.check_output(["qmake6", "-query", "QT_HOST_BINS"], text=True).strip()
    return str(Path(binaries) / name)


def backend_messages():
    messages = set()
    for path in sorted((ROOT / "crates/desktop/src").rglob("*.rs")):
        if "test" in path.name:
            continue
        for match in re.finditer(r'"((?:[^"\\]|\\.)*)"', path.read_text()):
            try:
                value = json.loads(match.group())
            except ValueError:
                continue
            if (" " in value or any(ord(c) > 127 for c in value) or (value[:1].isupper() and value.isalpha())) and any(c.isalpha() for c in value):
                if not any(token in value for token in ["SELECT ", "INSERT ", "CREATE ", "UPDATE ", "DELETE ", "#[", "{\""]):
                    messages.add(value)
    return messages


def extracted_catalog(directory, catalog=CATALOG):
    stub = directory / "backend.cpp"
    description = load_metadata()["description"]
    messages = backend_messages() | {description}
    declarations = [('QT_TRANSLATE_N_NOOP' if '%n' in m else 'QT_TRANSLATE_NOOP') + '("Backend", ' + json.dumps(m, ensure_ascii=False) + ');' for m in sorted(messages)]
    declarations.append('QT_TRANSLATE_NOOP("Metadata", ' + json.dumps(description) + ');')
    stub.write_text("\n".join(declarations))
    output = directory / catalog.name
    if catalog.exists():
        output.write_bytes(catalog.read_bytes())
    inputs = sorted((ROOT / "crates/desktop/ui/qml").glob("*.qml")) + sorted((ROOT / "crates/desktop/src/native").glob("*.cpp"))
    subprocess.run([qt_tool("lupdate"), *map(str, inputs), str(stub), "-locations", "none", "-no-obsolete", "-source-language", "es", "-target-language", catalog.stem, "-ts", str(output)], check=True, stdout=subprocess.DEVNULL)
    tree = ET.parse(output)
    for message in tree.findall(".//message") if catalog.stem == "es" else []:
        source = message.findtext("source", "")
        translation = message.find("translation")
        translation.attrib.pop("type", None)
        if message.get("numerus") == "yes":
            translation.clear()
            ET.SubElement(translation, "numerusform").text = source
            ET.SubElement(translation, "numerusform").text = SPANISH_PLURALS[source]
        else:
            translation.text = "Organiza credenciales y notas seguras con cifrado OpenPGP" if source == description else source
    ET.indent(tree, space="  ")
    return ET.tostring(tree.getroot(), encoding="utf-8", xml_declaration=True) + b"\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["update", "check", "compile"])
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.mode in ("update", "check"):
        catalogs = sorted(set(CATALOG.parent.glob("*.ts")) | {CATALOG})
        for catalog in catalogs:
            with tempfile.TemporaryDirectory() as temporary:
                content = extracted_catalog(Path(temporary), catalog)
            if args.mode == "update":
                catalog.parent.mkdir(parents=True, exist_ok=True)
                catalog.write_bytes(content)
            elif not catalog.exists() or catalog.read_bytes() != content:
                raise SystemExit(f"Translation catalog {catalog.name} is stale; run just translations")
    else:
        if args.output is None:
            parser.error("compile requires --output")
        args.output.mkdir(parents=True, exist_ok=True)
        resources = ET.Element("RCC")
        group = ET.SubElement(resources, "qresource", prefix="/i18n")
        domain = load_metadata()["app-id"]
        for catalog in sorted(CATALOG.parent.glob("*.ts")):
            name = f"{domain}_{catalog.stem}.qm"
            output = args.output / name
            subprocess.run([qt_tool("lrelease"), "-silent", str(catalog), "-qm", str(output)], check=True)
            ET.SubElement(group, "file", alias=name).text = str(output.resolve())
        ET.ElementTree(resources).write(args.output / "translations.qrc", encoding="utf-8")


if __name__ == "__main__":
    main()
