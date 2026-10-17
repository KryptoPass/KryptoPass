#!/usr/bin/env python3
"""Prepare the browser fork and install a user-scoped native messaging host."""

import argparse
import json
import os
from pathlib import Path
import re
import shlex
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "packaging"))
from metadata import load_metadata


def prepare(extension_dir):
    meta = load_metadata()
    extension_dir = Path(extension_dir).resolve(strict=True)
    client = extension_dir / "keepassxc-browser/background/client.js"
    source = client.read_text()
    source, count = re.subn(r"keepassClient.nativeHostName = '[^']+';",
                             f"keepassClient.nativeHostName = '{meta['browser-host-id']}';", source)
    if count != 1:
        raise ValueError("could not locate the extension native host setting")
    client.write_text(source)
    for relative in ["dist/manifest_firefox.json", "dist/manifest_chromium.json", "keepassxc-browser/manifest.json"]:
        path = extension_dir / relative
        data = json.loads(path.read_text())
        for field in ["applications", "browser_specific_settings"]:
            if "gecko" in data.get(field, {}):
                data[field]["gecko"]["id"] = meta["browser-extension-id"]
        # Store updates must not replace the local fork with the upstream extension.
        data.pop("update_url", None)
        path.write_text(json.dumps(data, indent=4) + "\n")
    path = extension_dir / "keepassxc-browser/background/keepass.js"
    source = path.read_text().replace("// Capabilities of the initial KryptoPass connection/association implementation.\n", "")
    start = source.index("keepass.updateFeaturesList = function (")
    end = source.index("\n};", start) + len("\n};")
    source = source[:start] + """// Capabilities of the initial KryptoPass connection/association implementation.
keepass.updateFeaturesList = function () {
    keepass.featuresList = {
        downloadFaviconAfterSave: false,
        newTotp: false,
        passwordGenerator: false,
        passkeys: false,
        passkeysDefaultGroup: false,
        requiredKeePassXCVersionFound: true,
        webSocket: false
    };
};""" + source[end:]
    path.write_text(source)


def install(proxy, browser, extension_id=None, home=None):
    meta = load_metadata()
    proxy = Path(proxy).resolve(strict=True)
    if not proxy.is_file() or not os.access(proxy, os.X_OK):
        raise ValueError("proxy must be an executable file")
    if browser in ("chromium", "brave", "brave-origin") and not re.fullmatch(r"[a-p]{32}", extension_id or ""):
        raise ValueError("This browser requires the 32-character ID of your unpacked extension")
    home = Path(home if home is not None else Path.home()).resolve()
    if browser == "firefox":
        hosts = home / ".mozilla/native-messaging-hosts"
        allowed = {"allowed_extensions": [meta["browser-extension-id"]]}
    elif browser in ("chromium", "brave", "brave-origin"):
        config = Path(os.environ.get("XDG_CONFIG_HOME", home / ".config"))
        if not config.is_absolute():
            raise ValueError("XDG_CONFIG_HOME must be absolute")
        browser_directory = {"chromium": "chromium", "brave": "BraveSoftware/Brave-Browser",
                             "brave-origin": "BraveSoftware/Brave-Origin"}[browser]
        hosts = config / browser_directory / "NativeMessagingHosts"
        allowed = {"allowed_origins": [f"chrome-extension://{extension_id}/"]}
    else:
        raise ValueError("unsupported browser")
    hosts.mkdir(parents=True, exist_ok=True)
    launcher = hosts / (meta["browser-host-id"] + "-launcher")
    manifest = hosts / (meta["browser-host-id"] + ".json")
    script = ('#!/bin/sh\nset -eu\n: "${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR is required}"\n'
              + f'exec {shlex.quote(str(proxy))} --socket "$XDG_RUNTIME_DIR"/{shlex.quote(meta["app-id"])}/browser.sock "$@"\n')
    # Reject symlinks and existing registrations rather than replacing another installation.
    with launcher.open("x") as file:
        file.write(script)
    launcher.chmod(0o700)
    try:
        with manifest.open("x") as file:
            json.dump({"name": meta["browser-host-id"], "description": meta["display-name"] + " Browser",
                       "path": str(launcher), "type": "stdio", **allowed}, file, indent=2)
            file.write("\n")
        manifest.chmod(0o600)
    except Exception:
        launcher.unlink()
        raise
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    fork = commands.add_parser("prepare")
    fork.add_argument("extension_dir", type=Path)
    host = commands.add_parser("install")
    host.add_argument("--proxy", type=Path, required=True)
    host.add_argument("--browser", choices=["firefox", "chromium", "brave", "brave-origin"], required=True)
    host.add_argument("--extension-id")
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.extension_dir)
        print("Extension fork prepared")
    else:
        print(install(args.proxy, args.browser, args.extension_id))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError) as error:
        sys.exit(str(error))
