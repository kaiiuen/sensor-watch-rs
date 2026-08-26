# USB CDC status

The reference TinyUSB application uses SAM L22 full-speed CDC with VID:PID
`0x1209:0x2151`, 64-byte packets, notification endpoint `0x81`, bulk OUT
endpoint `0x02`, and bulk IN endpoint `0x82`.

The Rust feasibility work is currently exposed as the opt-in `usb-enum`
feature. It keeps the reviewed descriptors and a host-testable standard
control-state machine, but does not activate USB hardware. The `atsaml22j`
0.1.0 PAC omits the descriptor-bank and packet SRAM types required to safely
service EP0 and endpoint transfers. The raw layout is documented in
`src/watch/usb.rs`; it is not a packet I/O implementation.

`usb-cdc` remains a compatibility opt-in that includes `usb-enum`, but it is
not a claim of CDC support. `init`, `poll`, `read`, and `write` fail closed with
`MissingPacketSram`. No terminal, shell, or `PING` response exists yet.

Host contract tests:

```sh
cargo test -p sensor-watch --lib --features usb-enum
cargo test -p sensor-watch --lib --features usb-cdc
```

Target feasibility checks:

```sh
cargo check --target thumbv6m-none-eabi -p sensor-watch --bin minimal-usb \
  --features minimal-usb,usb-enum
cargo build --release --target thumbv6m-none-eabi -p sensor-watch --bin minimal-usb \
  --features minimal-usb,usb-enum
```

The default production firmware remains unchanged. Completing USB requires a
reviewed PAC/raw SRAM implementation, clock and VBUS sequencing, suspend and
disconnect safety, endpoint tests, host enumeration traces, and only then real
CDC bulk transfer tests.
