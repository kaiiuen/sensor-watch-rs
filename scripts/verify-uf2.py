#!/usr/bin/env python3
"""Validate and stage Sensor-Watch UF2 recovery artifacts.

This tool never writes the ROM bootloader and never flashes a board. It checks
UF2 framing and SAM L22 metadata, records CRC32/SHA256, and makes explicit,
non-destructive backup and rollback copies for USB drag-and-drop recovery.
"""

import argparse
import hashlib
import json
import shutil
import struct
import sys
import zlib
from pathlib import Path
from typing import NoReturn

START0 = 0x0A324655
START1 = 0x9E5D5157
END = 0x0AB16F30
FAMILY = 0x2C29472F
APP_START = 0x2000
PAYLOAD = 256
BLOCK = 512
MAX_APP_BYTES = 0x3C000 - APP_START


def fail(message) -> NoReturn:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def inspect(path):
    try:
        file_size = path.stat().st_size
    except OSError as exc:
        fail(f"cannot inspect {path}: {exc}")
    if not file_size or file_size % BLOCK:
        fail("UF2 must be non-empty and a multiple of 512 bytes")
    max_uf2_bytes = ((MAX_APP_BYTES + PAYLOAD - 1) // PAYLOAD) * BLOCK
    if file_size > max_uf2_bytes:
        fail(f"UF2 is {file_size} bytes; maximum is {max_uf2_bytes}")
    try:
        data = path.read_bytes()
    except OSError as exc:
        fail(f"cannot read {path}: {exc}")
    if len(data) != file_size:
        fail("UF2 changed while it was being read")
    count = len(data) // BLOCK
    image = bytearray()
    for index in range(count):
        block = data[index * BLOCK:(index + 1) * BLOCK]
        words = struct.unpack_from("<8I", block, 0)
        if words[0] != START0 or words[1] != START1:
            fail(f"block {index}: invalid UF2 start magic")
        flags, address, size, number, total, family = words[2:]
        if not flags & 0x2000:
            fail(f"block {index}: family-ID flag is missing")
        if (address != APP_START + index * PAYLOAD or size != PAYLOAD or
                number != index or total != count or family != FAMILY):
            fail(f"block {index}: board or block metadata is invalid")
        if struct.unpack_from("<I", block, 508)[0] != END:
            fail(f"block {index}: invalid UF2 end magic")
        image.extend(block[32:32 + PAYLOAD])
    if len(image) > MAX_APP_BYTES:
        fail(f"firmware payload is {len(image)} bytes; maximum is {MAX_APP_BYTES}")
    return data, bytes(image), count


def record(path):
    data, image, blocks = inspect(path)
    return {
        "format": "sensor-watch-recovery-manifest-v1",
        "board": "ATSAML22J18A",
        "family_id": f"0x{FAMILY:08X}",
        "application_start": f"0x{APP_START:08X}",
        "maximum_application_bytes": MAX_APP_BYTES,
        "uf2_bytes": len(data),
        "uf2_blocks": blocks,
        "payload_bytes": len(image),
        "crc32_ieee": f"0x{zlib.crc32(image) & 0xffffffff:08X}",
        "sha256": hashlib.sha256(data).hexdigest(),
        "payload_sha256": hashlib.sha256(image).hexdigest(),
        "artifact": str(path),
    }


def write_manifest(path, manifest):
    path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="ascii")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    verify = sub.add_parser("verify", help="validate a UF2 and print its manifest")
    verify.add_argument("uf2", type=Path)
    verify.add_argument("--manifest", type=Path)

    backup = sub.add_parser("backup", help="validate and preserve a known-good UF2")
    backup.add_argument("uf2", type=Path)
    backup.add_argument("backup", type=Path)
    backup.add_argument("--manifest", type=Path)

    rollback = sub.add_parser("rollback", help="validate a backup and stage it for flashing")
    rollback.add_argument("backup", type=Path)
    rollback.add_argument("output", type=Path)

    args = parser.parse_args()
    if args.command == "verify":
        manifest = record(args.uf2)
        if args.manifest:
            write_manifest(args.manifest, manifest)
        print(json.dumps(manifest, indent=2))
        return

    if args.command == "backup":
        manifest = record(args.uf2)
        if args.backup.exists():
            fail(f"refusing to overwrite existing known-good backup: {args.backup}")
        args.backup.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(args.uf2, args.backup)
        if args.manifest:
            write_manifest(args.manifest, manifest)
        print(f"preserved known-good UF2 at {args.backup}")
        print(f"sha256 {manifest['sha256']}")
        return

    manifest = record(args.backup)
    if args.output.resolve() == args.backup.resolve():
        fail("rollback output must differ from the backup")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(args.backup, args.output)
    print(f"staged rollback UF2 at {args.output}")
    print(f"sha256 {manifest['sha256']}")
    print("Drag the staged UF2 to the watch USB drive; this tool does not flash hardware.")


if __name__ == "__main__":
    try:
        main()
    except PermissionError as exc:
        fail(f"permission denied: {exc}")
