# Software-only firmware recovery

The Rust `sensor-watch-tools` host binary hardens artifact handling on the host. It does **not** add a
golden image, recovery partition, or rollback selector to the watch firmware.
The ROM/UF2 bootloader remains the device-side recovery mechanism.

## Create a known-good generation

1. Build the release UF2 with `./build.sh` (or Studio's **Build UF2**).
2. Validate and create a recovery manifest:

   ```sh
   cargo run -p sensor-watch-tools -- verify target/thumbv6m-none-eabi/release/sensor-watch.uf2 \
     --manifest target/thumbv6m-none-eabi/release/sensor-watch.uf2.json
   ```

   The manifest records the board, SAM L22 family ID, application address,
   size, block count, payload CRC-32, UF2 SHA-256, payload SHA-256, and a
   generation ID. Structural validation checks UF2 format, family, addresses,
   ordering, sizes, and CRCs. Local digest consistency checks that the manifest
   and adjacent `.sig` describe the same fields; `sha256:` is only a digest
   label, not a cryptographic signature. The legacy `signature` field and `.sig`
   filename are retained for compatibility. Neither establishes provenance or
   authenticity. Existing manifest or digest files are never overwritten.
3. Preserve a known-good copy before distributing or replacing it:

   ```sh
   cargo run -p sensor-watch-tools -- backup \
     target/thumbv6m-none-eabi/release/sensor-watch.uf2 \
     recovery/known-good/<generation>.uf2
   ```

   Existing backup paths are never overwritten. Studio similarly keeps prior
   validated generations under the output directory's
   `recovery/generations/` folder.

## Select and stage rollback

Choose a `.uf2` whose adjacent `.json` manifest is trusted, then run:

```sh
cargo run -p sensor-watch-tools -- rollback \
  recovery/known-good/<generation>.uf2 \
  recovery/staged/sensor-watch.uf2 \
  <trusted-sha256>
```

Rollback requires the adjacent or explicitly supplied manifest, revalidates the
artifact, and optionally compares its actual UF2 SHA-256 to the trusted release
SHA-256 supplied on the command line. It copies through a temporary file and
verifies the destination. It
refuses an existing destination rather than replacing it. It only stages a
file, it does not flash hardware.
Drag the staged file to the watch's USB bootloader drive. Never rename an
unvalidated file to `CURRENT.UF2` manually.

Every deployment path validates UF2 magic, block order, target address, board
family, payload size, and end magic before copying. Malformed, oversized,
wrong-family, wrong-board, or CRC-mismatched artifacts must be rejected. A
A trusted release SHA-256 is provenance for the exact release only when it was
obtained through a separately trusted channel and explicitly supplied. If it is
missing, verification reports trusted provenance as **not provided**; local
structural validation and digest consistency still remain available. A matching
SHA-256 is not a signature and does not prove publisher identity. No public-key
or other authenticity mechanism is implemented here.

## Recovery report

Create an auditable host-side report without touching the artifact:

```sh
cargo run -p sensor-watch-tools -- report \
  recovery/known-good/<generation>.uf2 \
  <trusted-sha256>
```

The report records the generation and trusted release SHA-256 status, and
explicitly states that CRC failure is recorded on-device as
`Fault::CorruptImage`, while backup and rollback are host-side only. It also records that no true dual boot, device-side rollback,
or ROM bootloader modification is provided. Report paths are no-overwrite.

After staging, eject the USB drive normally and wait for the bootloader to
finish. If the watch does not boot, re-enter bootloader mode and stage a
previous known-good generation. This is host-side rollback only: it cannot
recover a board whose bootloader itself is damaged. No hardware test is implied
by artifact validation or report generation.
