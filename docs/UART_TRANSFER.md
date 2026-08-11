# UART settings transfer protocol

This protocol is intentionally limited to non-secret watch data. It is a UART
application protocol, not a firmware-update path. It never accepts filesystem
paths, raw flash addresses, UF2 records, or executable data.

The host-testable implementation is `core/src/transfer.rs`.

## Frame format

Every frame is exactly 256 bytes and is sent as one binary record:

| Bytes | Meaning |
|---:|---|
| 0..2 | ASCII `SW` magic |
| 2 | Protocol version (`1`) |
| 3 | Command: `1` read, `2` write data, `3` commit, `4` abort |
| 4 | Allowlisted object: `1` settings, `2` activity, `3` TOTP metadata |
| 5 | Reserved; must be zero when generating frames |
| 6..8 | Little-endian sequence number |
| 8..12 | Little-endian object offset |
| 12..16 | Little-endian total object length |
| 16..18 | Little-endian payload length (0..230) |
| 18..26 | Eight-byte authentication tag / hook input |
| 26..254 | Payload, followed by zero padding |
| 254..256 | CRC-16/CCITT-FALSE over bytes 0..254 |

The decoder validates the magic, version, command, allowlist, payload bounds,
object-specific size bound, offset arithmetic, and CRC before the authentication
hook is called.

## Write behavior

A write is a sequence of authenticated `WriteData` frames followed by an
authenticated `Commit` frame. Data chunks must be contiguous and sequence
numbers must increase. The receiver rejects out-of-order, mixed-object, and
incomplete transfers. The store's `commit` method is the only point at which
new data becomes durable.

The embedded adapter should implement `AtomicStore` using a fixed RAM staging
buffer and the existing `watch::storage::wear_leveled_write` API. It should:

1. validate the object and total size again;
2. stage chunks without exposing a partially written configuration;
3. on `commit`, write a versioned record through wear-level storage and verify it;
4. publish the new record only after the write succeeds; and
5. call `abort` on timeout, reset, CRC/authentication error, or insufficient RAM.

Do not implement the adapter by accepting a path or address from the host. The
three `ObjectId` values are the complete storage allowlist. Activity exports and
TOTP metadata must remain non-secret; secret TOTP seeds are intentionally not a
supported object.

`RejectAll` is the default authenticator. Firmware must provide an authenticated
session policy (for example a challenge-response backed by a device-specific
key) before enabling writes. CRC detects corruption; it is not authentication.

## UART integration boundary

`src/watch/uart.rs` remains a byte transport only. A future command loop may call
`try_getc`, collect exactly 256 bytes, and pass them to `Receiver::receive`.
It must enforce an idle timeout and clear the receiver on reset. No firmware
flashing command should be added to this loop; firmware updates remain on the
existing bootloader/SWD path.

There is no hardware test in this change. The protocol codec and receiver are
covered by host tests in the core crate.
