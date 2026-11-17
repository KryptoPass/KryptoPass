#!/usr/bin/env python3
"""Gettext catalogs for KI18n: extract with Messages.sh, merge, check and add languages.

`po/<app id>.pot` is the template; each language lives in
`po/<language>/<app id>.po`, the layout `ki18n_install(po)` installs.
"""

import argparse
from pathlib import Path
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "packaging"))

from metadata import load_metadata

PO_DIR = ROOT / "po"

# What KDE's scripty passes as $XGETTEXT (l10n-scripty, extract-messages.sh), so
# Messages.sh extracts the same here as on KDE's infrastructure.
KDE_XGETTEXT_KEYWORDS = [
    "-ci18n",
    "-ki18n:1", "-ki18nc:1c,2", "-ki18np:1,2", "-ki18ncp:1c,2,3",
    "-kki18n:1", "-kki18nc:1c,2", "-kki18np:1,2", "-kki18ncp:1c,2,3",
    "-kxi18n:1", "-kxi18nc:1c,2", "-kxi18np:1,2", "-kxi18ncp:1c,2,3",
    "-kkxi18n:1", "-kkxi18nc:1c,2", "-kkxi18np:1,2", "-kkxi18ncp:1c,2,3",
    "-kI18N_NOOP:1", "-kI18NC_NOOP:1c,2",
    "-kkli18n:1", "-kkli18nc:1c,2", "-kkli18np:1,2", "-kkli18ncp:1c,2,3",
    "-kklxi18n:1", "-kklxi18nc:1c,2", "-kklxi18np:1,2", "-kklxi18ncp:1c,2,3",
]


def domain() -> str:
    return load_metadata()["app-id"]


def template() -> Path:
    return PO_DIR / f"{domain()}.pot"


def catalogs() -> list[Path]:
    return sorted(PO_DIR.glob(f"*/{domain()}.po"))


def require(*tools: str) -> None:
    for tool in tools:
        if shutil.which(tool) is None:
            sys.exit(f"i18n: falta `{tool}` (paquete gettext)")


def xgettext_command() -> str:
    metadata = load_metadata()
    return " ".join([
        "xgettext",
        f"--package-name={domain()}",
        f"--package-version={metadata['version']}",
        f"--msgid-bugs-address={metadata['repository']}/issues",
        "--copyright-holder=KryptoPass_contributors",
        "--from-code=UTF-8",
        "--add-comments=i18n",
        "-C",
        "--kde",
        *KDE_XGETTEXT_KEYWORDS,
    ])


def run_messages(podir: Path) -> Path:
    environment = dict(os.environ, XGETTEXT=xgettext_command(), podir=str(podir))
    subprocess.run(["bash", str(ROOT / "Messages.sh")], cwd=ROOT, env=environment, check=True)
    return podir / f"{domain()}.pot"


def without_creation_date(path: Path) -> str:
    return "".join(
        line for line in path.read_text(encoding="utf-8").splitlines(keepends=True)
        if not line.startswith('"POT-Creation-Date:')
    )


def extract() -> int:
    require("xgettext", "msgmerge")
    with tempfile.TemporaryDirectory(prefix="kryptopass-i18n-") as temporary:
        fresh = run_messages(Path(temporary))
        # A new date alone is not a change; keep the file untouched.
        if not template().exists() or without_creation_date(fresh) != without_creation_date(template()):
            shutil.copyfile(fresh, template())
            print(f"actualizado: {template().relative_to(ROOT)}")
    for catalog in catalogs():
        subprocess.run(
            ["msgmerge", "--quiet", "--update", "--backup=none", "--previous", str(catalog), str(template())],
            check=True,
        )
    return 0


def check() -> int:
    require("xgettext", "msgfmt")
    failed = False
    with tempfile.TemporaryDirectory(prefix="kryptopass-i18n-") as temporary:
        fresh = run_messages(Path(temporary))
        if not template().exists() or without_creation_date(fresh) != without_creation_date(template()):
            print(f"{template().relative_to(ROOT)} está desactualizado; corre `just i18n-extract`")
            failed = True
    for catalog in catalogs():
        result = subprocess.run(["msgfmt", "--check", "--statistics", "-o", os.devnull, str(catalog)])
        failed |= result.returncode != 0
    return 1 if failed else 0


def new_language(language: str) -> int:
    require("msginit", "msgfilter")
    if not template().exists():
        sys.exit("i18n: falta la plantilla; corre `just i18n-extract`")
    catalog = PO_DIR / language / f"{domain()}.po"
    if catalog.exists():
        sys.exit(f"i18n: {catalog.relative_to(ROOT)} ya existe")
    catalog.parent.mkdir(parents=True)
    subprocess.run(
        ["msginit", "--no-translator", f"--locale={language}", "-i", str(template()), "-o", str(catalog)],
        check=True,
    )
    # msginit copies each msgid as the English translation, which assumes an English
    # source; ours is Spanish, so every language starts untranslated.
    subprocess.run(
        ["msgfilter", "--keep-header", "-i", str(catalog), "-o", str(catalog), "true"],
        check=True,
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("extract", help="regenera la plantilla y fusiona los catálogos")
    commands.add_parser("check", help="falla si la plantilla está vieja o un catálogo es inválido")
    language = commands.add_parser("new-language", help="crea el catálogo de un idioma")
    language.add_argument("language", help="código de idioma, por ejemplo en o pt_BR")
    arguments = parser.parse_args()
    if arguments.command == "extract":
        return extract()
    if arguments.command == "check":
        return check()
    return new_language(arguments.language)


if __name__ == "__main__":
    sys.exit(main())
