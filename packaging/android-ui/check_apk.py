"""Check APK ZIP alignment and every bundled native library's ELF alignment."""

import argparse
from pathlib import Path
import subprocess
import tempfile
import zipfile


def check(apk, ndk, sdk):
    zipalign = sdk / "build-tools/36.0.0/zipalign"
    readelf = ndk / "toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-readelf"
    subprocess.run([str(zipalign), "-c", "-P", "16", "4", str(apk)], check=True)
    with zipfile.ZipFile(apk) as archive, tempfile.TemporaryDirectory() as temporary:
        libraries = [name for name in archive.namelist() if name.startswith("lib/") and name.endswith(".so")]
        if not libraries:
            raise ValueError("APK contains no native libraries")
        library = Path(temporary) / "library.so"
        for name in libraries:
            library.write_bytes(archive.read(name))
            output = subprocess.check_output([str(readelf), "--program-headers", "--wide", str(library)], text=True)
            loads = [line.split() for line in output.splitlines() if line.strip().startswith("LOAD ")]
            if not loads or any(int(load[-1], 0) < 16384 for load in loads):
                raise ValueError(f"{name}: ELF LOAD segments are not aligned to 16 KB")
        print(f"apk-check: ZIP and {len(libraries)} native libraries aligned to 16 KB")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk", type=Path)
    parser.add_argument("--ndk", type=Path, required=True)
    parser.add_argument("--sdk", type=Path, required=True)
    args = parser.parse_args()
    check(args.apk, args.ndk, args.sdk)
