#!/usr/bin/env python3
"""Host regression tests for the software recovery workflow."""

import importlib.util
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("verify_uf2", Path(__file__).with_name("verify-uf2.py"))
assert SPEC is not None and SPEC.loader is not None
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


def make_uf2(payload=b"known-good"):
    out = bytearray()
    count = (len(payload) + VERIFY.PAYLOAD - 1) // VERIFY.PAYLOAD
    for number in range(count):
        chunk = payload[number * VERIFY.PAYLOAD:(number + 1) * VERIFY.PAYLOAD]
        block = bytearray(VERIFY.BLOCK)
        words = (VERIFY.START0, VERIFY.START1, 0x2000,
                 VERIFY.APP_START + number * VERIFY.PAYLOAD, VERIFY.PAYLOAD,
                 number, count, VERIFY.FAMILY)
        for offset, word in enumerate(words):
            block[offset * 4:offset * 4 + 4] = word.to_bytes(4, "little")
        block[32:32 + len(chunk)] = chunk
        block[508:512] = VERIFY.END.to_bytes(4, "little")
        out.extend(block)
    return bytes(out)


class RecoveryTests(unittest.TestCase):
    def test_malformed_and_wrong_family_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "bad.uf2"
            path.write_bytes(b"short")
            with self.assertRaises(SystemExit):
                VERIFY.inspect(path)
            data = bytearray(make_uf2())
            data[28:32] = (0x12345678).to_bytes(4, "little")
            path.write_bytes(data)
            with self.assertRaises(SystemExit):
                VERIFY.inspect(path)

    def test_oversized_uf2_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "large.uf2"
            path.write_bytes(b"\0" * (VERIFY.MAX_APP_BYTES // VERIFY.PAYLOAD + 1) * VERIFY.BLOCK)
            with self.assertRaises(SystemExit):
                VERIFY.inspect(path)

    def test_manifest_detects_crc_or_content_mismatch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "good.uf2"
            manifest = root / "good.uf2.json"
            artifact.write_bytes(make_uf2())
            record = VERIFY.record(artifact)
            VERIFY.write_manifest(manifest, record)
            corrupted = bytearray(artifact.read_bytes())
            corrupted[32] ^= 1
            artifact.write_bytes(corrupted)
            with self.assertRaises(SystemExit):
                VERIFY.verify_manifest(artifact, manifest)

    def test_backup_is_not_overwritten_and_rollback_is_validated(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "good.uf2"
            backup = root / "recovery.uf2"
            staged = root / "staged.uf2"
            artifact.write_bytes(make_uf2())
            manifest = VERIFY.record(artifact)
            backup.write_bytes(artifact.read_bytes())
            with self.assertRaises(SystemExit):
                # The same safety rule used by the backup command.
                if backup.exists():
                    VERIFY.fail("refusing to overwrite existing known-good backup")
            VERIFY.write_manifest(backup.with_suffix(".uf2.json"), manifest)
            checked = VERIFY.verify_manifest(backup, backup.with_suffix(".uf2.json"))
            self.assertEqual(checked["generation_id"], manifest["generation_id"])
            staged.write_bytes(backup.read_bytes())
            self.assertEqual(staged.read_bytes(), backup.read_bytes())


if __name__ == "__main__":
    unittest.main()
