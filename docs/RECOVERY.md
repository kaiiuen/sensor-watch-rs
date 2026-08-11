# Software-only firmware recovery

This procedure hardens artifact handling on the host. It does **not** add a
golden image, recovery partition, or rollback selector to the watch firmware.
The ROM/UF2 bootloader remains the device-side recovery mechanism.

## Create a known-good generation

1. Build the release UF2 with `./build.sh` (or Studio's **Build UF2**).
2. Validate and create a signed manifest:

   ```sh
   python scripts/verify-uf2.py verify target/thumbv6m-none-eabi/release/sensor-watch.uf2 \
     --manifest target/thumbv6m-none-eabi/release/sensor-watch.uf2.json
   ```

   The manifest records the board, SAM L22 family ID, application address,
   size, block count, payload CRC-32, UF2 SHA-256, payload SHA-256, and a
   generation ID. The adjacent `.sig` is a tamper-evident SHA-256 signature of
   the manifest fields. It is not a public-key signature; obtain release
   manifests through a trusted channel.
3. Preserve a known-good copy before distributing or replacing it:

   ```sh
   python scripts/verify-uf2.py backup \
     target/thumbv6m-none-eabi/release/sensor-watch.uf2 \
     recovery/known-good/<generation>.uf2
   ```

   Existing backup paths are never overwritten. Studio similarly keeps prior
   validated generations under the output directory's
   `recovery/generations/` folder.

## Select and stage rollback

Choose a `.uf2` whose adjacent `.json` manifest is trusted, then run:

```sh
python scripts/verify-uf2.py rollback \
  recovery/known-good/<generation>.uf2 \
  recovery/staged/sensor-watch.uf2
```

Rollback revalidates the manifest and artifact, copies through a temporary file,
and checks the final size. It only stages a file; it does not flash hardware.
Drag the staged file to the watch's USB bootloader drive. Never rename an
unvalidated file to `CURRENT.UF2` manually.

Every deployment path validates UF2 magic, block order, target address, board
family, payload size, and end magic before copying. Malformed, oversized,
wrong-family, wrong-board, or CRC-mismatched artifacts must be rejected. A
checksum fetched from the network is optional; when offline, Studio reports
that release checksum status is **unverified**, while local manifest and UF2
checks remain available.

After copying, eject the USB drive normally and wait for the bootloader to
finish. If the watch does not boot, re-enter bootloader mode and stage a
previous known-good generation. This is host-side rollback only: it cannot
recover a board whose bootloader itself is damaged.
