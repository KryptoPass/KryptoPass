# SPDX-License-Identifier: GPL-3.0-or-later
# SPDX-FileCopyrightText: 2026 KryptoPass contributors
"""Prepare the machine-local OpenPGP fixture used by debug Sync builds."""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
from typing import Any


FIXTURE_ID = "kp-synthetic-key-v1"
PASSPHRASE = "synthetic ART ECC passphrase only"
WARNING = "PUBLIC SYNTHETIC TEST KEY. NEVER USE FOR REAL SECRETS."
CHALLENGE = b"KryptoPass synthetic fixture protection check v1\n"
KEY_NAME = "secret-key.pgp"
MANIFEST_NAME = "fixture.json"
MAX_KEY_SIZE = 1024 * 1024
MAX_MANIFEST_SIZE = 8 * 1024


class FixtureError(Exception):
    """An expected fixture preparation or validation failure."""


def cache_directory() -> Path:
    override = os.environ.get("KP_SYNTHETIC_FIXTURE_DIR")
    if override:
        return Path(override).expanduser().absolute()
    cache_home = os.environ.get("XDG_CACHE_HOME")
    if cache_home:
        root = Path(cache_home).expanduser()
    else:
        home = os.environ.get("HOME")
        if not home:
            raise FixtureError("HOME is unavailable for the synthetic fixture cache")
        root = Path(home).expanduser() / ".cache"
    return (root / "kryptopass" / "synthetic-sync-v1").absolute()


def _lstat(path: Path) -> os.stat_result | None:
    try:
        return path.lstat()
    except FileNotFoundError:
        return None


def _ensure_directory(path: Path, *, private: bool) -> None:
    """Create a real directory, rejecting symlinks at every path component."""
    absolute = path.absolute()
    current = Path(absolute.anchor)
    for part in absolute.parts[1:]:
        current = current / part
        info = _lstat(current)
        if info is None:
            try:
                current.mkdir(mode=0o700)
            except FileExistsError:
                info = _lstat(current)
            else:
                info = _lstat(current)
        if info is None or not stat.S_ISDIR(info.st_mode):
            raise FixtureError("synthetic fixture path contains a non-directory or symlink")
    info = _lstat(absolute)
    if info is None or not stat.S_ISDIR(info.st_mode):
        raise FixtureError("synthetic fixture directory is unavailable")
    if private:
        if hasattr(os, "getuid") and info.st_uid != os.getuid():
            raise FixtureError("synthetic fixture directory is owned by another user")
        if stat.S_IMODE(info.st_mode) != 0o700:
            raise FixtureError("synthetic fixture directory must have mode 0700")


def _check_private_file(path: Path) -> os.stat_result:
    info = _lstat(path)
    if info is None or not stat.S_ISREG(info.st_mode):
        raise FixtureError("synthetic fixture file is missing or is not a regular file")
    if hasattr(os, "getuid") and info.st_uid != os.getuid():
        raise FixtureError("synthetic fixture file is owned by another user")
    if stat.S_IMODE(info.st_mode) != 0o600:
        raise FixtureError("synthetic fixture file must have mode 0600")
    return info


def _run_gpg(arguments: list[str], home: Path, *, input_bytes: bytes | None = None) -> bytes:
    try:
        result = subprocess.run(
            ["gpg", "--no-options", "--homedir", str(home), "--batch", "--no-tty", *arguments],
            input=input_bytes,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=60,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise FixtureError("GnuPG is required to prepare the synthetic fixture") from error
    if result.returncode != 0:
        raise FixtureError("GnuPG could not prepare or validate the synthetic fixture")
    return result.stdout


def _gpg_home() -> tempfile.TemporaryDirectory[str]:
    temporary = tempfile.TemporaryDirectory(prefix="kp-synthetic-gpg-")
    os.chmod(temporary.name, 0o700)
    return temporary


def _stop_agent(home: Path) -> None:
    try:
        subprocess.run(
            ["gpgconf", "--homedir", str(home), "--kill", "gpg-agent"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
            timeout=5,
        )
    except (OSError, subprocess.TimeoutExpired):
        pass


def _primary_fingerprint(home: Path) -> tuple[str, bool]:
    output = _run_gpg(["--with-colons", "--fingerprint", "--list-secret-keys"], home)
    current_key = False
    primary: str | None = None
    has_encrypting_key = False
    primary_count = 0
    for line in output.decode("ascii", errors="strict").splitlines():
        fields = line.split(":")
        if fields[0] == "sec":
            primary_count += 1
            if primary_count > 1:
                raise FixtureError("synthetic fixture contains more than one primary key")
            current_key = True
            continue
        if not current_key:
            continue
        if fields[0] == "fpr" and primary is None and len(fields) > 9:
            primary = fields[9].upper()
        if fields[0] in {"sec", "ssb"} and len(fields) > 11 and "e" in fields[11].lower():
            has_encrypting_key = True
    if primary_count != 1 or primary is None or not re.fullmatch(r"(?:[0-9A-F]{40}|[0-9A-F]{64})", primary):
        raise FixtureError("synthetic OpenPGP secret-key fingerprint is unavailable")
    return primary, has_encrypting_key


def _import_and_verify(key_bytes: bytes, metadata: dict[str, Any]) -> None:
    temporary = _gpg_home()
    home = Path(temporary.name)
    try:
        _run_gpg(["--import"], home, input_bytes=key_bytes)
        fingerprint, can_encrypt = _primary_fingerprint(home)
        if fingerprint != metadata["fingerprint"] or not can_encrypt:
            raise FixtureError("synthetic OpenPGP key does not match its fixture metadata")

        ciphertext = _run_gpg(
            [
                "--trust-model",
                "always",
                "--recipient",
                fingerprint,
                "--output",
                "-",
                "--encrypt",
            ],
            home,
            input_bytes=CHALLENGE,
        )
        wrong_passphrase = _run_gpg_optional(
            ["--pinentry-mode", "loopback", "--passphrase-fd", "0", "--decrypt"],
            home,
            input_bytes=b"incorrect synthetic fixture passphrase\n" + ciphertext,
        )
        if wrong_passphrase is not None:
            raise FixtureError("synthetic OpenPGP key is not protected by its fixture passphrase")
        plaintext = _run_gpg(
            ["--pinentry-mode", "loopback", "--passphrase-fd", "0", "--decrypt"],
            home,
            input_bytes=metadata["passphrase"].encode("utf-8") + b"\n" + ciphertext,
        )
        if plaintext != CHALLENGE:
            raise FixtureError("synthetic OpenPGP fixture passphrase check failed")
    finally:
        _stop_agent(home)
        temporary.cleanup()


def _run_gpg_optional(arguments: list[str], home: Path, *, input_bytes: bytes) -> bytes | None:
    try:
        result = subprocess.run(
            ["gpg", "--no-options", "--homedir", str(home), "--batch", "--no-tty", *arguments],
            input=input_bytes,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=60,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise FixtureError("GnuPG is required to prepare the synthetic fixture") from error
    return result.stdout if result.returncode == 0 else None


def _metadata(key_bytes: bytes, raw: dict[str, Any]) -> dict[str, Any]:
    required = {"fixture_id", "fingerprint", "sha256", "size", "passphrase", "warning"}
    if not required.issubset(raw):
        raise FixtureError("synthetic fixture metadata is incomplete")
    fixture_id = raw["fixture_id"]
    fingerprint = raw["fingerprint"]
    sha256 = raw["sha256"]
    size = raw["size"]
    passphrase = raw["passphrase"]
    warning = raw["warning"]
    if not isinstance(fixture_id, str) or fixture_id != FIXTURE_ID:
        raise FixtureError("synthetic fixture identifier is invalid")
    if not isinstance(fingerprint, str) or not re.fullmatch(
        r"(?:[0-9A-Fa-f]{40}|[0-9A-Fa-f]{64})", fingerprint
    ):
        raise FixtureError("synthetic fixture fingerprint is invalid")
    if not isinstance(sha256, str) or not re.fullmatch(r"[0-9a-f]{64}", sha256):
        raise FixtureError("synthetic fixture checksum metadata is invalid")
    if not isinstance(size, int) or isinstance(size, bool) or size <= 0:
        raise FixtureError("synthetic fixture size metadata is invalid")
    if size > MAX_KEY_SIZE:
        raise FixtureError("synthetic fixture key is larger than the allowed size")
    if not isinstance(passphrase, str) or not passphrase:
        raise FixtureError("synthetic fixture passphrase metadata is invalid")
    if not isinstance(warning, str) or "PUBLIC SYNTHETIC" not in warning.upper() or "NEVER USE" not in warning.upper():
        raise FixtureError("synthetic fixture warning is missing")
    if len(key_bytes) != size or hashlib.sha256(key_bytes).hexdigest() != sha256:
        raise FixtureError("synthetic fixture key does not match its size and checksum metadata")
    return {
        "fixture_id": fixture_id,
        "fingerprint": fingerprint.upper(),
        "sha256": sha256,
        "size": size,
        "passphrase": passphrase,
        "warning": warning,
    }


def _read_existing(directory: Path) -> tuple[bytes, dict[str, Any]] | None:
    key_path = directory / KEY_NAME
    manifest_path = directory / MANIFEST_NAME
    key_info = _lstat(key_path)
    manifest_info = _lstat(manifest_path)
    if key_info is None and manifest_info is None:
        return None
    if key_info is None or manifest_info is None:
        raise FixtureError("synthetic fixture cache is incomplete; refusing to replace it")
    _check_private_file(key_path)
    _check_private_file(manifest_path)
    if (
        key_info.st_size <= 0
        or key_info.st_size > MAX_KEY_SIZE
        or manifest_info.st_size <= 0
        or manifest_info.st_size > MAX_MANIFEST_SIZE
    ):
        raise FixtureError("synthetic fixture cache file size is invalid")
    try:
        key_bytes = key_path.read_bytes()
        raw = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise FixtureError("synthetic fixture cache is unreadable or corrupt") from error
    if not isinstance(raw, dict):
        raise FixtureError("synthetic fixture metadata is corrupt")
    metadata = _metadata(key_bytes, raw)
    _import_and_verify(key_bytes, metadata)
    return key_bytes, metadata


def _generate_fixture() -> tuple[bytes, dict[str, Any]]:
    temporary = _gpg_home()
    home = Path(temporary.name)
    password_input = PASSPHRASE.encode("utf-8") + b"\n"
    try:
        _run_gpg(
            [
                "--pinentry-mode",
                "loopback",
                "--passphrase-fd",
                "0",
                "--quick-generate-key",
                "KryptoPass synthetic Sync fixture",
                "ed25519",
                "cert",
                "0",
            ],
            home,
            input_bytes=password_input,
        )
        fingerprint, _ = _primary_fingerprint(home)
        _run_gpg(
            [
                "--pinentry-mode",
                "loopback",
                "--passphrase-fd",
                "0",
                "--quick-add-key",
                fingerprint,
                "cv25519",
                "encr",
                "0",
            ],
            home,
            input_bytes=password_input,
        )
        key_bytes = _run_gpg(
            ["--pinentry-mode", "loopback", "--passphrase-fd", "0", "--export-secret-keys", fingerprint],
            home,
            input_bytes=password_input,
        )
        raw = {
            "fixture_id": FIXTURE_ID,
            "fingerprint": fingerprint,
            "sha256": hashlib.sha256(key_bytes).hexdigest(),
            "size": len(key_bytes),
            "passphrase": PASSPHRASE,
            "warning": WARNING,
        }
        metadata = _metadata(key_bytes, raw)
        _import_and_verify(key_bytes, metadata)
        return key_bytes, metadata
    finally:
        _stop_agent(home)
        temporary.cleanup()


def _write_private_file(path: Path, data: bytes) -> None:
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags, 0o600)
        with os.fdopen(descriptor, "wb") as output:
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
    except FileExistsError as error:
        raise FixtureError("refusing to overwrite an existing synthetic fixture file") from error
    os.chmod(path, 0o600)


def _manifest_bytes(metadata: dict[str, Any]) -> bytes:
    return (json.dumps(metadata, indent=2, ensure_ascii=True) + "\n").encode("utf-8")


def _prepare(directory: Path) -> dict[str, Any]:
    _ensure_directory(directory, private=True)
    lock_path = directory / ".prepare.lock"
    flags = os.O_CREAT | os.O_RDWR
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        lock_fd = os.open(lock_path, flags, 0o600)
    except OSError as error:
        raise FixtureError("could not lock the synthetic fixture cache") from error
    try:
        lock_info = os.fstat(lock_fd)
        if not stat.S_ISREG(lock_info.st_mode) or stat.S_IMODE(lock_info.st_mode) != 0o600:
            raise FixtureError("synthetic fixture lock file is invalid")
        if hasattr(os, "getuid") and lock_info.st_uid != os.getuid():
            raise FixtureError("synthetic fixture lock file is owned by another user")
        fcntl.flock(lock_fd, fcntl.LOCK_EX)
        existing = _read_existing(directory)
        if existing is not None:
            return existing[1]
        key_bytes, metadata = _generate_fixture()
        _write_private_file(directory / KEY_NAME, key_bytes)
        _write_private_file(directory / MANIFEST_NAME, _manifest_bytes(metadata))
        _import_and_verify(key_bytes, metadata)
        return metadata
    finally:
        os.close(lock_fd)


def _stage_assets(directory: Path, stage_directory: Path) -> None:
    metadata = _read_existing(directory)
    if metadata is None:
        raise FixtureError("synthetic fixture cache is empty")
    key_bytes, manifest = metadata
    asset_directory = stage_directory / "synthetic-sync-v1"
    _ensure_directory(asset_directory, private=True)
    for name, content in ((KEY_NAME, key_bytes), (MANIFEST_NAME, _manifest_bytes(manifest))):
        target = asset_directory / name
        info = _lstat(target)
        if info is not None:
            if not stat.S_ISREG(info.st_mode) or stat.S_IMODE(info.st_mode) != 0o600:
                raise FixtureError("generated synthetic asset path is invalid")
            if target.read_bytes() != content:
                raise FixtureError("refusing to replace a different generated synthetic asset")
            continue
        _write_private_file(target, content)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=cache_directory())
    parser.add_argument("--stage", type=Path, help="copy verified assets into a generated debug asset directory")
    arguments = parser.parse_args()
    try:
        _prepare(arguments.directory.expanduser().absolute())
        if arguments.stage is not None:
            _stage_assets(arguments.directory.expanduser().absolute(), arguments.stage.expanduser().absolute())
        return 0
    except FixtureError as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
