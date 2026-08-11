#!/usr/bin/env python3
"""Validate, sign, backup, and stage Sensor-Watch UF2 recovery artifacts.

This tool is deliberately host-side. It never writes the ROM bootloader and
never flashes a board. A detached manifest signature is a tamper-evident
SHA-256 digest, not a vendor/public-key signature; authenticity still requires
checking the manifest/signature from a trusted release channel.
"""

import argparse
import hashlib
import hmac
import json
import os
import stat
import struct
import sys
import time
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
MAX_UF2_BYTES = ((MAX_APP_BYTES + PAYLOAD - 1) // PAYLOAD) * BLOCK
MAX_MANIFEST_BYTES = 512 * 1024
FORMAT = "sensor-watch-recovery-manifest-v2"


def fail(message) -> NoReturn:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def read_regular_file(path, maximum):
    """Read one regular file through a single descriptor, not a pathname twice."""
    if path.is_symlink():
        fail(f"refusing symlinked file path: {path}")
    flags = os.O_RDONLY
    nofollow = getattr(os, "O_NOFOLLOW", 0)
    try:
        fd = os.open(path, flags | nofollow)
        try:
            before = os.fstat(fd)
            if not stat.S_ISREG(before.st_mode):
                fail(f"refusing non-regular file: {path}")
            if before.st_size > maximum:
                fail(f"file is {before.st_size} bytes; maximum is {maximum}")
            with os.fdopen(fd, "rb", closefd=True) as source:
                data = source.read(maximum + 1)
                after = os.fstat(source.fileno())
                if len(data) != before.st_size or after.st_size != before.st_size:
                    fail(f"file changed while it was being read: {path}")
                return data

    except OSError as exc:
        fail(f"cannot read {path}: {exc}")


def inspect(path):
    data = read_regular_file(path, MAX_UF2_BYTES)
    file_size = len(data)
    if file_size > MAX_UF2_BYTES:
        fail(f"UF2 exceeds maximum size of {MAX_UF2_BYTES} bytes")
    if not file_size or file_size % BLOCK:
        fail(f"UF2 is {file_size} bytes; expected a non-empty multiple of {BLOCK}")
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
            fail(f"block {index}: board, family, or block metadata is invalid")
        if struct.unpack_from("<I", block, 508)[0] != END:
            fail(f"block {index}: invalid UF2 end magic")
        image.extend(block[32:32 + PAYLOAD])
    if len(image) > MAX_APP_BYTES:
        fail(f"firmware payload is {len(image)} bytes; maximum is {MAX_APP_BYTES}")
    return data, bytes(image), count


def generation_id(data):
    # Time makes each backup selectable even when two images have the same hash.
    return f"g{time.time_ns()}-{hashlib.sha256(data).hexdigest()[:12]}"


def record(path, generation=None):
    data, image, blocks = inspect(path)
    manifest = {
        "format": FORMAT,
        "generation_id": generation or generation_id(data),
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
    manifest["signature"] = sign_manifest(manifest)
    return manifest


def sign_manifest(manifest):
    unsigned = {key: value for key, value in manifest.items() if key != "signature"}
    canonical = json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode("ascii")
    return "sha256:" + hashlib.sha256(canonical).hexdigest()


def write_manifest(path, manifest):
    """Create a manifest and detached signature without replacing either file."""
    path.parent.mkdir(parents=True, exist_ok=True)
    signature_path = path.with_suffix(path.suffix + ".sig")
    if path.exists() or signature_path.exists():
        fail(f"refusing to overwrite existing recovery metadata: {path}")
    manifest_text = json.dumps(manifest, indent=2) + "\n"
    try:
        with path.open("x", encoding="ascii") as destination:
            destination.write(manifest_text)
        with signature_path.open("x", encoding="ascii") as destination:
            destination.write(manifest["signature"] + "\n")
    except OSError as exc:
        path.unlink(missing_ok=True)
        signature_path.unlink(missing_ok=True)
        fail(f"cannot create recovery metadata: {exc}")


def verify_manifest(path, manifest_path, trusted_sha256=None):
    try:
        if manifest_path.is_symlink() or manifest_path.stat().st_size > MAX_MANIFEST_BYTES:
            fail("manifest is symlinked or too large")
        manifest = json.loads(manifest_path.read_text(encoding="ascii"))
    except (OSError, ValueError) as exc:
        fail(f"cannot read manifest: {exc}")
    if manifest.get("format") != FORMAT or manifest.get("signature") != sign_manifest(manifest):
        fail("manifest signature is invalid")
    actual = record(path, generation=manifest.get("generation_id"))
    if trusted_sha256 is not None:
        trusted = trusted_sha256.strip().lower()
        if len(trusted) != 64 or any(character not in "0123456789abcdef" for character in trusted):
            fail("trusted SHA-256 must be exactly 64 hexadecimal characters")
        if not hmac.compare_digest(actual["sha256"], trusted):
            fail("UF2 does not match the trusted release SHA-256")
    for field in ("format", "generation_id", "board", "family_id", "application_start",
                  "maximum_application_bytes", "uf2_bytes", "uf2_blocks", "payload_bytes",
                  "crc32_ieee", "sha256", "payload_sha256"):
        if actual.get(field) != manifest.get(field):
            fail(f"manifest mismatch for {field}")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    verify = sub.add_parser("verify", help="validate a UF2 and print or create its manifest")
    verify.add_argument("uf2", type=Path)
    verify.add_argument("--manifest", type=Path)
    verify.add_argument("--trusted-sha256", required=False,
                        help="trusted release digest obtained out-of-band")

    backup = sub.add_parser("backup", help="validate and preserve a known-good UF2")
    backup.add_argument("uf2", type=Path)
    backup.add_argument("backup", type=Path)
    backup.add_argument("--manifest", type=Path)

    rollback = sub.add_parser("rollback", help="validate a backup and stage it for flashing")
    rollback.add_argument("backup", type=Path)
    rollback.add_argument("output", type=Path)
    rollback.add_argument("--manifest", type=Path)
    rollback.add_argument("--trusted-sha256", required=False,
                          help="trusted release digest obtained out-of-band")

    report = sub.add_parser("report", help="write a host-side recovery report")
    report.add_argument("uf2", type=Path)
    report.add_argument("--manifest", type=Path)
    report.add_argument("--output", type=Path)
    report.add_argument("--trusted-sha256", required=False,
                        help="trusted release digest obtained out-of-band")

    args = parser.parse_args()
    if args.command == "verify":
        if args.manifest and args.manifest.exists():
            manifest = verify_manifest(args.uf2, args.manifest, args.trusted_sha256)
        else:
            manifest = record(args.uf2)
            if args.manifest:
                write_manifest(args.manifest, manifest)
        print(json.dumps(manifest, indent=2))
        return

    if args.command == "backup":
        manifest = record(args.uf2)
        if args.backup.is_symlink():
            fail(f"refusing symlinked backup path: {args.backup}")
        manifest_path = args.manifest or args.backup.with_suffix(args.backup.suffix + ".json")
        signature_path = manifest_path.with_suffix(manifest_path.suffix + ".sig")
        if args.backup.exists():
            fail(f"refusing to overwrite existing known-good backup: {args.backup}")
        if manifest_path.exists() or signature_path.exists():
            fail(f"refusing to overwrite existing recovery metadata: {manifest_path}")
        args.backup.parent.mkdir(parents=True, exist_ok=True)
        temp = args.backup.with_suffix(args.backup.suffix + ".tmp")
        if temp.exists() or temp.is_symlink():
            fail(f"refusing to use existing backup temporary path: {temp}")
        try:
            source_data, _, _ = inspect(args.uf2)
            with temp.open("xb") as destination:
                destination.write(source_data)
                destination.flush()
                os.fsync(destination.fileno())
            copied_data, _, _ = inspect(temp)
            if copied_data != source_data:
                fail("backup copy content verification failed")
            # Link creation is atomic and fails if another backup appeared.
            os.link(temp, args.backup)
        except FileExistsError:
            fail(f"refusing to overwrite existing known-good backup: {args.backup}")
        finally:
            temp.unlink(missing_ok=True)
        # The preserved artifact is now the manifest's recorded artifact. Re-sign
        # after changing this informational path; payload identity is unchanged.
        manifest["artifact"] = str(args.backup)
        manifest["signature"] = sign_manifest(manifest)
        write_manifest(args.manifest or args.backup.with_suffix(args.backup.suffix + ".json"), manifest)
        print(f"preserved known-good UF2 at {args.backup}")
        print(f"generation {manifest['generation_id']}")
        print(f"sha256 {manifest['sha256']}")
        return

    if args.command == "report":
        manifest_path = args.manifest or args.uf2.with_suffix(args.uf2.suffix + ".json")
        if not manifest_path.exists():
            fail("recovery report requires an adjacent or explicit manifest")
        if not args.trusted_sha256:
            fail("report requires an out-of-band trusted SHA-256")
        manifest = verify_manifest(args.uf2, manifest_path, args.trusted_sha256)
        report = {
            "format": "sensor-watch-recovery-report-v1",
            "artifact": str(args.uf2),
            "manifest": str(manifest_path),
            "generation_id": manifest["generation_id"],
            "validated": True,
            "crc_fault_recording": "device-side CRC failure records Fault::CorruptImage only",
            "host_recovery": "backup and explicit rollback staging are available",
            "device_side_rollback": False,
            "true_dual_boot": False,
            "rom_bootloader_modified": False,
            "hardware_tested": False,
        }
        output = args.output
        if output:
            if output.exists() or output.is_symlink():
                fail(f"refusing to overwrite existing recovery report: {output}")
            output.parent.mkdir(parents=True, exist_ok=True)
            with output.open("x", encoding="ascii") as destination:
                json.dump(report, destination, indent=2)
                destination.write("\n")
        print(json.dumps(report, indent=2))
        return

    manifest_path = args.manifest or args.backup.with_suffix(args.backup.suffix + ".json")
    if not manifest_path.exists():
        fail("rollback requires a trusted adjacent or explicit manifest")
    if not args.trusted_sha256:
        fail("rollback requires an out-of-band trusted SHA-256")
    manifest = verify_manifest(args.backup, manifest_path, args.trusted_sha256)
    try:
        same_path = args.output.resolve() == args.backup.resolve()
    except OSError as exc:
        fail(f"cannot resolve rollback paths: {exc}")
    if same_path or args.output.is_symlink() or args.backup.is_symlink():
        fail("rollback paths must be distinct regular files")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    if args.output.exists() or args.output.is_symlink():
        fail(f"refusing to overwrite existing rollback staging path: {args.output}")
    temp = args.output.with_suffix(args.output.suffix + ".tmp")
    if temp.exists() or temp.is_symlink():
        fail(f"refusing to use existing rollback temporary path: {temp}")
    try:
        source_data, _, _ = inspect(args.backup)
        with temp.open("xb") as destination:
            destination.write(source_data)
            destination.flush()
            os.fsync(destination.fileno())
        copied_data, _, _ = inspect(temp)
        if copied_data != source_data:
            fail("rollback copy content verification failed")
        os.link(temp, args.output)
    except FileExistsError:
        fail(f"refusing to overwrite existing rollback staging path: {args.output}")
    finally:
        temp.unlink(missing_ok=True)
    inspect(args.output)
    print(f"staged rollback UF2 at {args.output}")
    print(f"generation {manifest['generation_id']}")
    print(f"sha256 {manifest['sha256']}")
    print("Drag the staged UF2 to the watch USB drive; this tool does not flash hardware.")


if __name__ == "__main__":
    try:
        main()
    except PermissionError as exc:
        fail(f"permission denied: {exc}")
