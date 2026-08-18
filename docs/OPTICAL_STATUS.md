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
