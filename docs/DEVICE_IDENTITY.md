# Device identity

The firmware exposes a bounded, read-only `id` (alias `identity`) UART shell
command. It is available only on the documented SERCOM3 UART jig at 9600
8-N-1; the UF2 USB mass-storage device is not USB CDC and does not provide this
runtime command.

The command returns a line such as:

```text
ID UID=00112233445566778899AABBCCDDEEFF SOURCE=SAM-L22-signature-row BOARD=unknown REV=unknown CONF=unknown NOTE=identifier-not-authentication
```

`UID` is a 128-bit masked display fingerprint, not the raw silicon UID. The
source is the SAM L22 signature row, and board/revision remain unknown unless a
future board-specific, reviewed source can establish them. The fingerprint is
for device-profile selection and diagnostics only. It is not authentication,
a secret, or authorization; `UID` and `INFO_UF2.TXT` must never be treated as
proof of ownership.

## SAM L22 address and byte order

Microchip's **SAM L22 Family Data Sheet**, DS60001479, section “Serial Number”
defines the 128-bit serial number in the signature row at
`0x0080_A00C` through `0x0080_A01B`. The firmware reads four 32-bit words at
those increasing addresses. Each word is converted with little-endian byte
order, preserving increasing memory-address order in the 16-byte UID. This is
also consistent with the current `atsaml22j` PAC/reference memory map: the PAC
does not expose this signature row as a peripheral register.

Authoritative references:

- [SAM L22 Family Data Sheet (Microchip, DS60001479)](https://www.microchip.com/en-us/product/SAM-L22)
- [atsaml22j PAC source](https://docs.rs/atsaml22j/0.1.0/atsaml22j/)

Studio refuses profile mismatches by default. Matching uses the masked
fingerprint plus board and revision; an explicit user confirmation is required
for a mismatch, and no persisted preference can bypass that check.
