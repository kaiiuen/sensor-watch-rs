# Sensor Watch Lite headless hardware test

This is an **opt-in developer image**, not a replacement for the normal
production firmware. It targets only `Red/Lite` revision `OSO-SWAT-A1-02`.
Unknown board/revision pairs are rejected. The image contains no LCD, button,
watch-face, movement, optical, optional-sensor, or normal application code.

## Build

From the `sensor-watch-rs` root:

```powershell
$env:CARGO_TARGET_DIR = 'target/validation/lite-hw-test'
cargo build -p sensor-watch --bin lite-hw-test --release --target thumbv6m-none-eabi --features lite-hw-test
```

The ELF is:

```text
target/validation/lite-hw-test/thumbv6m-none-eabi/release/lite-hw-test
```

The image is intentionally not included in the stock Studio/launcher build.
Create a UF2 only by explicitly passing this ELF to the existing UF2 tooling;
never overwrite the stock `sensor-watch.uf2`.

## Protocol

The line-oriented read-only CDC contract is bounded to 32 input bytes and 128
response bytes:

```text
ping
help
status
test all
test identity
test rtc
test storage
test led
```

`erase`, `write`, calibration, update, shell escape, and arbitrary file/I/O
commands are not part of the parser. Host tests use mocks and therefore never
report hardware PASS. Before endpoint SRAM and bulk completion are proven on a
real SAM L22 with a protocol analyzer, CDC hardware stays fail-closed; the
current image may enumerate only as far as the reviewed USB skeleton allows.

The green LED is reserved for a one-second heartbeat only while a configured
USB test session is active. The red LED is used for activity/error indication.
The Red/Lite mapping uses PA20 red and PA21 green, common-anode (active-low).
The hardware storage backend remains fail-closed until the reserved scratch
row is confirmed against the deployed data layout. The host seam does prove
read/write/restore ordering without touching user data.

## Restore normal firmware

1. Disconnect the watch and exit any serial/Studio session.
2. Enter the SAM-BA/UF2 bootloader using the board's documented reset gesture.
3. Copy the known-good production `sensor-watch.uf2` to the bootloader volume.
4. Wait for the volume to eject, then reconnect and verify the normal firmware
   startup. Do not leave the Lite test image installed for normal use.

A failed or unproven test is reported as `FAIL` or `UNKNOWN`; absence of a
hardware response is not evidence of PASS.
