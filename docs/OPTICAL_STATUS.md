# Pro IrDA receive status

The `pro-irda-rx` Cargo feature is disabled by default and is intended only for
Sensor Watch Pro hardware. Lite and other boards keep `SensorUnavailable`.

The opt-in path configures the checked-in Pro reference wiring:

- `IR_ENABLE`: PB22, driven low (active low)
- `IRSENSE`: PA04, SERCOM0 PAD0, PMUX D
- SERCOM0 USART receive-only, `FORM=2` (IrDA), experimental 900 baud
- no transmitter is provided by this firmware path

The PAC (`atsaml22j 0.1.0`) has no IrDA-specific API, so the implementation uses
the USART `FORM` register field directly. The receive service drains bounded
fixed-size buffers from the main loop and does not use the community file
upload/delete protocol.

TimeSync frames use the core optical framing, CRC, frame sequence/replay checks,
and an 8-byte payload containing packed RTC `DateTime` followed by a freshness
seconds value. The firmware integration is receive-only: authentication/key
provisioning is not production-ready, and no RTC mutation is enabled. A CRC,
valid date, fresh sequence, authentication result, and physical-presence
authorization would all be required before a future mutation path could call
`rtc::set_date_time`.

**NOT TESTED ON HARDWARE.** The Rust port has only been compile-tested. The
community/reference result does not prove this firmware works on a physical Pro
board, and Studio's optical preview is not a transmitter or receiver test.
The 900-baud setting, optical polarity, electrical levels, standby behavior,
and end-to-end reception still require a real Pro board and an IrDA source.

## Session skeleton boundary

`core/src/optical.rs` contains the replaceable, fixed-storage session seam. Its
explicit states are `Idle`, `Receiving`, `Authenticated`, `Authorized`,
`Applied`, `AckQueued`, and `Expired`. `OpticalIo` bounds polling to 64 bytes,
keeps ACK delivery as an injected operation, and delegates any RTC application
to an injected adapter. The default `TimeSyncPolicy::receive_only()` never
permits authentication, physical authorization, or RTC mutation. ACKs are
queued only in memory by the current watch adapter; they are not transmitted.

Studio's `preview_waveform` and frame preview are deterministic software
representations only. They do not drive an LED, GPIO, serial port, camera, or
receiver and make no hardware claim.

## Unsupported hardware steps

Before enabling a production receiver or RTC mutation, a hardware owner must:

1. Select and document a supported optical receiver/electrical interface for a
   specific board revision; verify polarity, voltage levels, and power behavior.
2. Validate the Pro IR enable and sense routing against the assembled board,
   including the UART mode and 900-baud timing with an oscilloscope or logic
   analyzer.
3. Implement and review real key provisioning/authentication and the physical
   presence authorization path; do not substitute the Studio preview tag.
4. Validate bounded reception, replay/freshness/duty-cycle behavior, ACK
   delivery, standby/wake behavior, and failure recovery on hardware.
5. Only after those checks, provide an explicit RTC adapter and enable mutation
   in a board-specific feature/configuration. No default feature should change.
