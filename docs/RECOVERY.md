# Software-only firmware recovery

The Rust `sensor-watch-tools` host binary hardens artifact handling on the host. It does **not** add a
golden image, recovery partition, or rollback selector to the watch firmware.
The ROM/UF2 bootloader remains the device-side recovery mechanism.

## Create a known-good generation

1. Build the release UF2 with `./build.sh` (or Studio's **Build UF2**).
2. Validate and create a signed manifest:

   ```sh
   cargo run -p sensor-watch-tools -- verify target/thumbv6m-none-eabi/release/sensor-watch.uf2 \
     --manifest target/thumbv6m-none-eabi/release/sensor-watch.uf2.json
   ```

   The manifest records the board, SAM L22 family ID, application address,
   size, block count, payload CRC-32, UF2 SHA-256, payload SHA-256, and a
   generation ID. The adjacent `.sig` is a tamper-evident SHA-256 signature of
   the manifest fields. It is not a public-key signature, obtain release
   manifests through a trusted channel. Existing manifest or signature files
   are never overwritten.
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
artifact, copies through a temporary file, and verifies the destination. It
refuses an existing destination rather than replacing it. It only stages a
file, it does not flash hardware.
Drag the staged file to the watch's USB bootloader drive. Never rename an
unvalidated file to `CURRENT.UF2` manually.

Every deployment path validates UF2 magic, block order, target address, board
family, payload size, and end magic before copying. Malformed, oversized,
wrong-family, wrong-board, or CRC-mismatched artifacts must be rejected. A
checksum fetched from the network is optional, when offline, Studio reports
that release checksum status is **unverified**, while local manifest and UF2
checks remain available.

## Recovery report

Create an auditable host-side report without touching the artifact:

```sh
cargo run -p sensor-watch-tools -- report \
  recovery/known-good/<generation>.uf2 \
  <trusted-sha256>
```

The report records the generation and explicitly states that CRC failure is
recorded on-device as `Fault::CorruptImage`, while backup and rollback are
host-side only. It also records that no true dual boot, device-side rollback,
or ROM bootloader modification is provided. Report paths are no-overwrite.

After staging, eject the USB drive normally and wait for the bootloader to
finish. If the watch does not boot, re-enter bootloader mode and stage a
previous known-good generation. This is host-side rollback only: it cannot
recover a board whose bootloader itself is damaged. No hardware test is implied
by artifact validation or report generation.
