# Firmware build and UF2 flashing

This page describes the complete user-facing path from Rust source to a UF2 file and then to the watch's USB bootloader. It also explains which sizes describe real flash, which sizes describe a file container, and which checks do not prove that hardware was successfully programmed.

## Flash layout

The onboard MCU is a Microchip SAM L22J18A with 256 KiB of flash. The firmware layout reserves the first 8 KiB for the bootloader and the last 8 KiB for the RWW EEPROM emulation area.

| Region | Start | End | Size |
| --- | ---: | ---: | ---: |
| Bootloader | `0x00000000` | `0x00002000` | 8 KiB |
| Application | `0x00002000` | `0x0003C000` | 232 KiB, 237568 bytes |
| RWW EEPROM emulation | `0x0003C000` | `0x00040000` | 8 KiB |

The application linker region starts at `0x2000` and is limited to `0x3A000`, which is 232 KiB or 237568 bytes. The application must fit inside that region. It must not overwrite the bootloader or the RWW EEPROM area.

The RWW EEPROM area is flash used by the firmware's persistent storage scheme. RWW means read-while-write support for the relevant flash arrangement. It is not additional application capacity.

## Build the firmware

Run these commands from the repository root, `sensor-watch-rs`.

### Prerequisites

Install Rust with the `thumbv6m-none-eabi` target. The build also needs an ARM `objcopy` implementation. The repository linker configuration supplies the SAM L22 memory layout when the build is run from the repository.

On Windows, the direct release firmware build is:

```powershell
cargo build --release --target thumbv6m-none-eabi -p sensor-watch --bin sensor-watch
```

The repository wrapper does the same embedded build and is useful when invoking it from another directory:

```powershell
.\scripts\build-firmware.ps1
```

The resulting ELF is normally:

```text
target/thumbv6m-none-eabi/release/sensor-watch
```

The wrapper builds the firmware ELF. It does not by itself create the final UF2.

### Build the UF2 and sidecars

The Rust host tool performs the rest of the pipeline:

```powershell
cargo run -p sensor-watch-tools -- build
```

That command performs these steps:

1. Build the release `sensor-watch` binary for `thumbv6m-none-eabi`.
2. Convert the ELF to a raw binary with ARM `objcopy`.
3. Reject a binary larger than the 237568-byte application limit.
4. Encode the binary as UF2 blocks beginning at application address `0x2000`.
5. Write the UF2 to `target/thumbv6m-none-eabi/release/sensor-watch.uf2`.
6. Write the host-side manifest to `sensor-watch.uf2.json`.
7. Write the host-side local digest sidecar to `sensor-watch.uf2.json.sig`.

If a previous UF2 exists, the build tool preserves a generation under `target/thumbv6m-none-eabi/release/recovery/generations/` before replacing the generated artifact. Do not treat that backup as a device-side rollback feature.

The compatibility shell script is also available in environments with a POSIX shell:

```sh
./build.sh
```

It invokes the same Rust host build command.

## What is inside a UF2 file

A UF2 file is a sequence of fixed 512-byte blocks. Each block carries 256 bytes of firmware payload. The remaining bytes are framing and padding.

For this board, each block contains, in order:

- 32 bytes of header metadata
- 256 bytes of payload
- 220 bytes of zero padding
- 4 bytes of UF2 end magic

The header includes the UF2 start magic values, flags indicating that a family ID is present, the target flash address, the payload size of 256, the zero-based block number, the total block count, and the SAM L22 family ID `0x2C29472F`. The first target address is `0x2000`, and each later block advances by 256 bytes. The final four bytes contain the UF2 end magic.

The last payload block is padded with zero bytes if the raw binary is not an exact multiple of 256. UF2 does not store the original unpadded binary length in the reconstructed image. A parser therefore reconstructs a payload rounded up to a 256-byte boundary. The manifest records the reconstructed payload size and other local metadata. It does not change what the bootloader writes from each UF2 block.

The UF2 encoder rejects an empty input and an input larger than the application region. At the maximum application size, 237568 bytes is exactly 928 payload blocks, producing 475136 bytes of UF2 data.

### Why the UF2 can be about 437 KiB

The container is twice the payload size because one 512-byte UF2 block carries only 256 bytes of payload. For example:

```text
447488 UF2 bytes / 512 bytes per block = 874 blocks
874 blocks * 256 payload bytes = 223744 payload bytes
223744 bytes / 1024 = 218.5 KiB payload
```

Therefore a UF2 file of 447488 bytes is about 437 KiB as a logical file, while representing about 218.5 KiB of application payload. The extra space is the per-block header, padding, and end marker. It is not extra firmware flash and it does not imply an 8 MiB or 8 MB MCU.

For that example, the remaining application margin is:

```text
237568 byte application limit - 223744 byte payload = 13824 bytes
```

The 13824-byte margin is measured against the application region, not against the UF2 file length.

## Windows file size and Size on disk

Windows Explorer can show both `Size` and `Size on disk` for a file. `Size` is the logical length stored in the file metadata. `Size on disk` is the amount of filesystem allocation consumed, rounded up to the volume's cluster or allocation unit and affected by filesystem details.

Use the logical `Size` when comparing a UF2 with the arithmetic above. A larger `Size on disk` does not mean that the UF2 contains more blocks, that the firmware is larger, or that the watch has more flash. UF2 block count and payload size come from the file bytes and their headers, not from Windows allocation accounting.

## The virtual UF2 bootloader drive

When the watch enters its UF2 bootloader, it presents a virtual USB mass-storage drive. The drive has a mass-storage geometry chosen by the bootloader implementation. In particular, a device may report an apparent capacity such as 8 MB even though the SAM L22J18A has only 256 KiB of physical flash.

That apparent drive capacity is not a flash capacity report. It is a USB mass-storage presentation used to make file transfer work with common host operating systems. Exact values such as an 8 MB geometry, sector count, free-space display, volume label, and directory contents are bootloader implementation details. They are not proof that the MCU has 8 MB of flash or that all of the virtual drive is writable firmware storage.

Copy only the intended UF2 artifact to the bootloader drive, then eject the drive normally and wait for the bootloader to finish. The drive's reported free space is not a reliable way to estimate application headroom.

## What the bootloader does with the UF2

The UF2 contract lets the bootloader inspect each fixed-size block, check its framing and board metadata, take the 256-byte payload, and associate it with the target flash address. For this board, valid application addresses begin at `0x2000` and stay within the 237568-byte application region.

The bootloader can receive and process blocks through its USB mass-storage implementation, then write accepted payload data to the corresponding flash locations. This description intentionally does not assume a particular buffering strategy. The bootloader may buffer data in ways that are not visible through the UF2 file format. UF2 validity describes the artifact and its block metadata. It does not expose every device-side implementation detail.

The bootloader's target checks and the application's own integrity checks are separate stages. A block can have valid UF2 framing and still be unsuitable for a particular board if its address, family ID, count, or other metadata is wrong. A correctly formatted artifact can also fail to boot because of a physical connection problem, power loss, a damaged bootloader, flash failure, or a firmware defect.

## Metadata sidecars stay on the host

The `.json` manifest and `.json.sig` file are host-side metadata. They help tools record and check the board identity, family ID, application limit, UF2 byte count, block count, reconstructed payload size, CRC-32, SHA-256 values, and generation information.

They are not part of the UF2 byte stream. Do not expect the bootloader to parse them or copy them into flash. Copying only the `.uf2` file is the normal drag-and-drop operation.

The `.json.sig` name is retained for compatibility. In this repository it contains a local manifest digest, not a cryptographic signature and not proof of publisher identity. A trusted release signature, if introduced by a separate release system, is a different concern from this local sidecar.

## Verify before flashing

To validate a generated UF2 and its sidecars, use the host tool:

```powershell
cargo run -p sensor-watch-tools -- verify `
  target/thumbv6m-none-eabi/release/sensor-watch.uf2 `
  --manifest target/thumbv6m-none-eabi/release/sensor-watch.uf2.json
```

Structural validation checks that the file is nonempty and a multiple of 512 bytes, then checks UF2 magic, family flag, target addresses, payload size, block numbering, total block count, family ID, and end magic. It reconstructs the padded payload and checks the recorded CRC and digest fields when a manifest is supplied.

This is structural and local digest validation. It proves that the bytes and metadata are internally consistent on the host. It does not prove that the file came from a trusted publisher unless an independently trusted release-authentication process verifies that claim. It also does not prove hardware success. Hardware success requires the physical transfer to complete and the watch to leave the bootloader and run the newly written firmware.

If the watch does not start after flashing, re-enter bootloader mode and use a known-good UF2. Artifact validation cannot repair a damaged bootloader or establish that a physical flash operation completed.

## Size summary

- Physical flash: 256 KiB on the SAM L22J18A
- Bootloader reservation: 8 KiB
- Application region: 232 KiB or 237568 bytes
- RWW EEPROM area: 8 KiB
- One UF2 block: 512 bytes total
- Payload in one UF2 block: 256 bytes
- Worked UF2 example: 447488 bytes, 874 blocks, 223744 payload bytes
- Worked application margin: 13824 bytes
