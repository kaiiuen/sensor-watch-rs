//! Developer-only SAM L22 USB full-speed enumeration layer.
//!
//! This module contains the Developer-only CDC contract and the bounded
//! controller-facing layer. Bulk hardware remains fail closed until the
//! SAM L22 USB SRAM/endpoint behavior has been proven on hardware. Hardware
//! constants are taken from Microchip.SAML22_DFP.3.8.203.atpack and the pinned
//! TinyUSB reference at 5572168994a29266df6cbf12b46919498d3ece66.

#![cfg(any(feature = "usb-enum", feature = "usb-cdc"))]

pub const MAX_PACKET_SIZE: usize = 64;
pub const USB_DPRAM_ORIGIN: usize = 0x2000_0000;
pub const USB_DPRAM_SIZE: usize = 512;
pub const USB_ENDPOINT_COUNT: usize = 8;
pub const USB_DESCRIPTOR_BANK_SIZE: usize = 16;
pub const USB_DESCRIPTOR_BANK_STRIDE: usize = 32;
pub const USB_DESCRIPTOR_TABLE_SIZE: usize = USB_ENDPOINT_COUNT * USB_DESCRIPTOR_BANK_STRIDE;
pub const USB_EP0_OUT_BUFFER: usize = USB_DESCRIPTOR_TABLE_SIZE;
pub const USB_EP0_IN_BUFFER: usize = USB_EP0_OUT_BUFFER + MAX_PACKET_SIZE;

pub const NOTIFICATION_ENDPOINT: u8 = 0x81;
pub const RX_ENDPOINT: u8 = 0x02;
pub const TX_ENDPOINT: u8 = 0x82;

pub const USB_REGISTER_BASE: usize = 0x4100_0000;
pub const USB_CTRLA_OFFSET: usize = 0x000;
pub const USB_SYNCBUSY_OFFSET: usize = 0x002;
pub const USB_CTRLB_OFFSET: usize = 0x008;
pub const USB_DADD_OFFSET: usize = 0x00a;
pub const USB_INTFLAG_OFFSET: usize = 0x01c;
pub const USB_DESCADD_OFFSET: usize = 0x024;
pub const USB_PADCAL_OFFSET: usize = 0x028;
pub const USB_ENDPOINTS_OFFSET: usize = 0x100;
pub const USB_ENDPOINT_STRIDE: usize = 0x20;

// OSCCTRL register offsets from Microchip.SAML22_DFP.3.8.203.atpack.
pub const OSCCTRL_STATUS_OFFSET: usize = 0x0c;
pub const OSCCTRL_DFLLCTRL_OFFSET: usize = 0x18;
pub const OSCCTRL_DFLLVAL_OFFSET: usize = 0x1c;
pub const OSCCTRL_DFLLMUL_OFFSET: usize = 0x20;
pub const OSCCTRL_DFLLSYNC_OFFSET: usize = 0x24;
pub const GCLK_GENCTRL_OFFSET: usize = 0x20;
pub const GCLK_PCHCTRL_OFFSET: usize = 0x80;
pub const GCLK1_INDEX: usize = 1;
pub const GCLK_USB_INDEX: usize = 6;

const USB_CTRLA_SWRST: u8 = 1 << 0;
const USB_CTRLA_ENABLE: u8 = 1 << 1;
const USB_CTRLA_RUNSTDBY: u8 = 1 << 2;
const USB_CTRLB_SPDCONF_FS: u16 = 0;
const USB_CTRLB_DETACH: u16 = 1 << 0;
const USB_INTFLAG_SUSPEND: u16 = 1 << 0;
const USB_INTFLAG_EORST: u16 = 1 << 3;
const USB_INTFLAG_WAKEUP: u16 = 1 << 4;
const USB_INTFLAG_EORSM: u16 = 1 << 5;
const USB_EPINT_TRFAIL0: u8 = 1 << 2;
const USB_EPINT_TRFAIL1: u8 = 1 << 3;
const USB_EPINT_STALL0: u8 = 1 << 5;
const USB_EPINT_STALL1: u8 = 1 << 6;
const USB_EPINT_ERRORS: u8 =
    USB_EPINT_TRFAIL0 | USB_EPINT_TRFAIL1 | USB_EPINT_STALL0 | USB_EPINT_STALL1;
const USB_EPCFG_CONTROL: u8 = 1;
const USB_EPINT_RXSTP: u8 = 1 << 4;
const USB_EPINT_TRCPT0: u8 = 1 << 0;
const USB_EPINT_TRCPT1: u8 = 1 << 1;
const USB_EPSTATUS_BK0RDY: u8 = 1 << 6;
const USB_EPSTATUS_BK1RDY: u8 = 1 << 7;
const USB_EPSTATUSCLR_BK0RDY: u8 = 1 << 6;
const USB_EPSTATUSCLR_BK1RDY: u8 = 1 << 7;
const USB_PCKSIZE_SIZE_64: u32 = 6 << 28;
const USB_PCKSIZE_BYTE_COUNT_MASK: u32 = 0x3fff;
const USB_PCKSIZE_MULTI_PACKET_64: u32 = 64 << 14;

/// USB device descriptor from the pinned TinyUSB reference.
pub const DEVICE_DESCRIPTOR: [u8; 18] = [
    18, 1, 0x00, 0x02, 0xef, 0x02, 0x01, 64, 0x09, 0x12, 0x51, 0x21, 0x00, 0x01, 1, 2, 3, 1,
];

/// CDC ACM configuration descriptor from the pinned TinyUSB layout.
///
/// The endpoint addresses are part of the host-visible contract even while the
/// controller bulk path remains disabled by the hardware proof gate.
pub const CONFIGURATION_DESCRIPTOR: [u8; 75] = [
    9,
    2,
    75,
    0,
    2,
    1,
    0,
    0xA0,
    50,
    8,
    0x0B,
    0,
    2,
    0x02,
    0x02,
    0x01,
    0,
    9,
    4,
    0,
    0,
    1,
    0x02,
    0x02,
    0x01,
    0,
    5,
    0x24,
    0x00,
    0x10,
    0x01,
    5,
    0x24,
    0x01,
    0x00,
    0x01,
    4,
    0x24,
    0x02,
    0x02,
    5,
    0x24,
    0x06,
    0,
    1,
    7,
    5,
    NOTIFICATION_ENDPOINT,
    0x03,
    8,
    0,
    16,
    9,
    4,
    1,
    0,
    2,
    0x0A,
    0,
    0,
    0,
    7,
    5,
    RX_ENDPOINT,
    0x02,
    64,
    0,
    0,
    7,
    5,
    TX_ENDPOINT,
    0x02,
    64,
    0,
    0,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlState {
    Default,
    Addressed,
    Configured,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlMachine {
    state: ControlState,
    configuration: u8,
    pending_address: Option<u8>,
}

impl ControlMachine {
    pub const fn new() -> Self {
        Self {
            state: ControlState::Default,
            configuration: 0,
            pending_address: None,
        }
    }

    pub const fn state(self) -> ControlState {
        self.state
    }

    /// Queue SET_ADDRESS; the USB address becomes active after status-IN.
    pub fn set_address(&mut self, address: u8) -> bool {
        if address > 127 || self.state == ControlState::Configured {
            return false;
        }
        self.pending_address = Some(address);
        true
    }

    pub fn complete_status(&mut self) -> Option<u8> {
        let address = self.pending_address.take()?;
        self.state = if address == 0 {
            ControlState::Default
        } else {
            ControlState::Addressed
        };
        Some(address)
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn set_configured(&mut self, value: u8) -> bool {
        match (self.state, value) {
            (ControlState::Addressed, 1) => {
                self.configuration = 1;
                self.state = ControlState::Configured;
                true
            }
            (ControlState::Configured, 0) => {
                self.configuration = 0;
                self.state = ControlState::Addressed;
                true
            }
            _ => false,
        }
    }

    pub const fn configuration(self) -> u8 {
        self.configuration
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetupPacket {
    pub bm_request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlResponse {
    Data { offset: usize, length: usize },
    DataValue(u8),
    Status,
    Stall,
}

/// Host-testable standard requests supported by the minimal profile.
pub fn handle_setup(machine: &mut ControlMachine, setup: SetupPacket) -> ControlResponse {
    let direction_in = setup.bm_request_type & 0x80 != 0;
    let standard_device = setup.bm_request_type & 0x60 == 0;
    if !standard_device || setup.index != 0 {
        return ControlResponse::Stall;
    }
    match (
        direction_in,
        setup.request,
        setup.value >> 8,
        setup.value as u8,
    ) {
        (true, 6, 1, _) => ControlResponse::Data {
            offset: 0,
            length: core::cmp::min(setup.length as usize, DEVICE_DESCRIPTOR.len()),
        },
        (true, 6, 2, _) => ControlResponse::Data {
            offset: DEVICE_DESCRIPTOR.len(),
            length: core::cmp::min(setup.length as usize, CONFIGURATION_DESCRIPTOR.len()),
        },
        (false, 5, 0, _)
            if setup.index == 0 && setup.length == 0 && machine.set_address(setup.value as u8) =>
        {
            ControlResponse::Status
        }
        (true, 8, 0, _) if setup.length == 1 => ControlResponse::DataValue(machine.configuration()),
        (false, 9, 0, 1) if setup.length == 0 && machine.set_configured(1) => {
            ControlResponse::Status
        }
        (false, 9, 0, 0) if setup.length == 0 && machine.set_configured(0) => {
            ControlResponse::Status
        }
        _ => ControlResponse::Stall,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbError {
    NoVbus,
    Timeout,
    UnsafeHardware,
}

/// A fixed-size CDC packet buffer. No heap allocation is used by the transport.
pub const CDC_ENDPOINT_BUFFER_SIZE: usize = 64;
/// All three CDC endpoint payload buffers use the same full-speed size.
pub const CDC_NOTIFICATION_BUFFER_SIZE: usize = 64;
pub const CDC_BULK_OUT_BUFFER_SIZE: usize = 64;
pub const CDC_BULK_IN_BUFFER_SIZE: usize = 64;
pub const CDC_QUEUE_DEPTH: usize = 4;
pub const CDC_LINE_MAX: usize = 32;

const _: () = assert!(CDC_ENDPOINT_BUFFER_SIZE == MAX_PACKET_SIZE);
const _: () = assert!(CDC_QUEUE_DEPTH > 0);

/// The SAM L22 packet-memory placement and completion semantics are not yet
/// proven. Keep this false until a protocol-analyzer-backed review enables it.
const BULK_HARDWARE_PROVEN: bool = false;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connected,
    Suspended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbState {
    Detached,
    Default,
    Addressed,
    Configured,
    Suspended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CdcError {
    Unsupported,
    NotEnumerated,
    Overflow,
    InvalidRequest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineCoding {
    pub baud_rate: u32,
    pub stop_bits: u8,
    pub parity: u8,
    pub data_bits: u8,
}

impl Default for LineCoding {
    fn default() -> Self {
        Self {
            baud_rate: 115_200,
            stop_bits: 0,
            parity: 0,
            data_bits: 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CdcControlRequest {
    GetLineCoding { length: u16 },
    SetLineCoding { length: u16, coding: LineCoding },
    SetControlLineState { value: u16 },
}

pub const CDC_REQ_SET_LINE_CODING: u8 = 0x20;
pub const CDC_REQ_GET_LINE_CODING: u8 = 0x21;
pub const CDC_REQ_SET_CONTROL_LINE_STATE: u8 = 0x22;
pub const CDC_LINE_CODING_SIZE: u16 = 7;

impl LineCoding {
    pub const fn from_bytes(bytes: &[u8; 7]) -> Self {
        Self {
            baud_rate: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            stop_bits: bytes[4],
            parity: bytes[5],
            data_bits: bytes[6],
        }
    }

    pub fn to_bytes(self) -> [u8; 7] {
        let baud = self.baud_rate.to_le_bytes();
        [
            baud[0],
            baud[1],
            baud[2],
            baud[3],
            self.stop_bits,
            self.parity,
            self.data_bits,
        ]
    }
}

/// Decode a CDC ACM class request received on interface 0.
///
/// SET_LINE_CODING data is supplied only after the bounded EP0 OUT stage has
/// completed. Requests for other interfaces, directions, or lengths stall.
pub fn handle_cdc_setup(
    transport: &mut CdcTransport,
    setup: SetupPacket,
    data: Option<[u8; 7]>,
) -> Result<CdcControlResponse, CdcError> {
    if setup.index != 0 || setup.bm_request_type & 0x60 != 0x20 {
        return Err(CdcError::InvalidRequest);
    }
    if setup.request != CDC_REQ_SET_CONTROL_LINE_STATE && setup.value != 0 {
        return Err(CdcError::InvalidRequest);
    }
    let request = match (setup.bm_request_type, setup.request) {
        (0xA1, CDC_REQ_GET_LINE_CODING) => CdcControlRequest::GetLineCoding {
            length: setup.length,
        },
        (0x21, CDC_REQ_SET_LINE_CODING) => CdcControlRequest::SetLineCoding {
            length: setup.length,
            coding: data
                .map(|bytes| LineCoding::from_bytes(&bytes))
                .ok_or(CdcError::InvalidRequest)?,
        },
        (0x21, CDC_REQ_SET_CONTROL_LINE_STATE) if setup.length == 0 => {
            CdcControlRequest::SetControlLineState { value: setup.value }
        }
        _ => return Err(CdcError::Unsupported),
    };
    transport.handle_control_request(request)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CdcControlResponse {
    LineCoding(LineCoding),
    Accepted,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadOnlyCommand {
    TestAll,
    TestIdentity,
    TestRtc,
    TestStorage,
    TestLed,
    Status,
    Ping,
    Help,
}

impl ReadOnlyCommand {
    pub fn parse(input: &[u8]) -> Option<Self> {
        match input {
            b"test all" => Some(Self::TestAll),
            b"test identity" => Some(Self::TestIdentity),
            b"test rtc" => Some(Self::TestRtc),
            b"test storage" => Some(Self::TestStorage),
            b"test led" => Some(Self::TestLed),
            b"status" => Some(Self::Status),
            b"ping" => Some(Self::Ping),
            b"help" => Some(Self::Help),
            _ => None,
        }
    }
}

/// A CDC transport boundary with deliberately inert transfer operations.
///
/// The buffers and contracts are ready for a reviewed physical implementation,
/// but no method fabricates enumeration, packets, or shell responses.
pub struct CdcTransport {
    rx: [[u8; CDC_ENDPOINT_BUFFER_SIZE]; CDC_QUEUE_DEPTH],
    rx_lengths: [u8; CDC_QUEUE_DEPTH],
    rx_head: usize,
    rx_tail: usize,
    rx_count: usize,
    tx: [[u8; CDC_ENDPOINT_BUFFER_SIZE]; CDC_QUEUE_DEPTH],
    tx_lengths: [u8; CDC_QUEUE_DEPTH],
    tx_head: usize,
    tx_tail: usize,
    tx_count: usize,
    line: [u8; CDC_LINE_MAX],
    line_len: usize,
    usb_state: UsbState,
    connection: ConnectionState,
    suspended_state: UsbState,
    line_coding: LineCoding,
    control_line_state: u16,
}

impl CdcTransport {
    pub const fn new() -> Self {
        Self {
            rx: [[0; CDC_ENDPOINT_BUFFER_SIZE]; CDC_QUEUE_DEPTH],
            rx_lengths: [0; CDC_QUEUE_DEPTH],
            rx_head: 0,
            rx_tail: 0,
            rx_count: 0,
            tx: [[0; CDC_ENDPOINT_BUFFER_SIZE]; CDC_QUEUE_DEPTH],
            tx_lengths: [0; CDC_QUEUE_DEPTH],
            tx_head: 0,
            tx_tail: 0,
            tx_count: 0,
            line: [0; CDC_LINE_MAX],
            line_len: 0,
            usb_state: UsbState::Detached,
            connection: ConnectionState::Disconnected,
            suspended_state: UsbState::Detached,
            line_coding: LineCoding {
                baud_rate: 115_200,
                stop_bits: 0,
                parity: 0,
                data_bits: 8,
            },
            control_line_state: 0,
        }
    }

    pub const fn state(&self) -> UsbState {
        self.usb_state
    }

    pub const fn connection_state(&self) -> ConnectionState {
        self.connection
    }

    pub const fn line_coding(&self) -> LineCoding {
        self.line_coding
    }

    pub fn on_vbus(&mut self, present: bool) {
        if present {
            if self.connection == ConnectionState::Disconnected {
                self.connection = ConnectionState::Connected;
                self.usb_state = UsbState::Default;
            }
        } else {
            self.disconnect();
        }
    }

    fn clear_queues(&mut self) {
        self.rx_head = 0;
        self.rx_tail = 0;
        self.rx_count = 0;
        self.tx_head = 0;
        self.tx_tail = 0;
        self.tx_count = 0;
        self.line_len = 0;
    }

    pub fn on_bus_reset(&mut self) {
        self.clear_queues();
        self.control_line_state = 0;
        if self.connection != ConnectionState::Disconnected {
            self.connection = ConnectionState::Connected;
        }
        self.usb_state = if self.connection == ConnectionState::Disconnected {
            UsbState::Detached
        } else {
            UsbState::Default
        };
    }

    pub fn on_addressed(&mut self) {
        if self.connection == ConnectionState::Connected && self.usb_state != UsbState::Configured {
            self.usb_state = UsbState::Addressed;
        }
    }

    pub fn on_configured(&mut self, configured: bool) {
        if configured
            && self.connection == ConnectionState::Connected
            && self.usb_state != UsbState::Configured
        {
            self.clear_queues();
            self.usb_state = UsbState::Configured;
        } else if !configured
            && self.connection == ConnectionState::Connected
            && self.usb_state != UsbState::Addressed
        {
            self.usb_state = UsbState::Addressed;
            self.clear_queues();
        }
    }

    pub fn on_suspend(&mut self) {
        if self.connection == ConnectionState::Connected {
            self.suspended_state = self.usb_state;
            self.connection = ConnectionState::Suspended;
            self.usb_state = UsbState::Suspended;
        }
    }

    pub fn on_resume(&mut self) {
        if self.connection == ConnectionState::Suspended {
            self.connection = ConnectionState::Connected;
            self.usb_state = self.suspended_state;
        }
    }

    pub fn disconnect(&mut self) {
        self.clear_queues();
        self.control_line_state = 0;
        self.connection = ConnectionState::Disconnected;
        self.usb_state = UsbState::Detached;
    }

    pub const fn control_line_state(&self) -> u16 {
        self.control_line_state
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<usize, CdcError> {
        if bytes.len() > CDC_ENDPOINT_BUFFER_SIZE {
            return Err(CdcError::Overflow);
        }
        if self.usb_state != UsbState::Configured {
            return Err(CdcError::NotEnumerated);
        }
        if self.tx_count == CDC_QUEUE_DEPTH {
            return Err(CdcError::Overflow);
        }
        let slot = self.tx_tail;
        self.tx[slot][..bytes.len()].copy_from_slice(bytes);
        self.tx_lengths[slot] = bytes.len() as u8;
        self.tx_tail = (slot + 1) % CDC_QUEUE_DEPTH;
        self.tx_count += 1;
        Ok(bytes.len())
    }

    /// Remove one complete packet for the proven hardware IN completion seam.
    pub fn take_tx_packet(&mut self, out: &mut [u8; CDC_ENDPOINT_BUFFER_SIZE]) -> Option<usize> {
        if self.tx_count == 0 {
            return None;
        }
        let slot = self.tx_head;
        let length = self.tx_lengths[slot] as usize;
        out[..length].copy_from_slice(&self.tx[slot][..length]);
        self.tx_head = (slot + 1) % CDC_QUEUE_DEPTH;
        self.tx_count -= 1;
        Some(length)
    }

    pub fn read(&mut self) -> Result<Option<u8>, CdcError> {
        if self.usb_state != UsbState::Configured {
            return Err(CdcError::NotEnumerated);
        }
        if self.rx_count == 0 {
            return Ok(None);
        }
        let slot = self.rx_head;
        let value = self.rx[slot][0];
        if self.rx_lengths[slot] > 1 {
            self.rx[slot].copy_within(1..self.rx_lengths[slot] as usize, 0);
            self.rx_lengths[slot] -= 1;
        } else {
            self.rx_lengths[slot] = 0;
            self.rx_head = (slot + 1) % CDC_QUEUE_DEPTH;
            self.rx_count -= 1;
        }
        Ok(Some(value))
    }

    /// Queue one full-size-or-smaller hardware OUT packet. It never executes a command.
    pub fn accept_rx_packet(&mut self, packet: &[u8]) -> Result<(), CdcError> {
        if packet.len() > CDC_ENDPOINT_BUFFER_SIZE {
            return Err(CdcError::Overflow);
        }
        if self.usb_state != UsbState::Configured {
            return Err(CdcError::NotEnumerated);
        }
        if self.rx_count == CDC_QUEUE_DEPTH {
            return Err(CdcError::Overflow);
        }
        let slot = self.rx_tail;
        self.rx[slot][..packet.len()].copy_from_slice(packet);
        self.rx_lengths[slot] = packet.len() as u8;
        self.rx_tail = (slot + 1) % CDC_QUEUE_DEPTH;
        self.rx_count += 1;
        Ok(())
    }

    /// Consume a line-delimited packet and return only the read-only commands.
    pub fn next_command(&mut self) -> Result<Option<ReadOnlyCommand>, CdcError> {
        while let Some(byte) = self.read()? {
            match byte {
                b'\r' => {}
                b'\n' => {
                    let command = Self::allow_read_only_command(&self.line[..self.line_len]);
                    self.line_len = 0;
                    return command.map(Some);
                }
                byte if self.line_len < CDC_LINE_MAX => {
                    self.line[self.line_len] = byte;
                    self.line_len += 1;
                }
                _ => {
                    self.line_len = 0;
                    return Err(CdcError::Overflow);
                }
            }
        }
        Ok(None)
    }

    pub fn handle_control_request(
        &mut self,
        request: CdcControlRequest,
    ) -> Result<CdcControlResponse, CdcError> {
        if self.usb_state != UsbState::Configured {
            return Err(CdcError::NotEnumerated);
        }
        match request {
            CdcControlRequest::GetLineCoding { length } if length == 7 => {
                Ok(CdcControlResponse::LineCoding(self.line_coding))
            }
            CdcControlRequest::SetLineCoding { length, coding } if length == 7 => {
                self.line_coding = coding;
                Ok(CdcControlResponse::Accepted)
            }
            CdcControlRequest::SetControlLineState { value } => {
                self.control_line_state = value;
                Ok(CdcControlResponse::Accepted)
            }
            _ => Err(CdcError::Unsupported),
        }
    }

    /// Recognize only read-only command names; execution is intentionally absent.
    pub fn allow_read_only_command(input: &[u8]) -> Result<ReadOnlyCommand, CdcError> {
        ReadOnlyCommand::parse(input).ok_or(CdcError::Unsupported)
    }
}

impl Default for CdcTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_arch = "arm")]
mod hardware {
    #![allow(unsafe_op_in_unsafe_fn)]
    use super::*;
    use core::ptr::{read_volatile, write_volatile};

    const GCLK: usize = 0x4000_1c00;
    const MCLK: usize = 0x4000_0800;
    const OSCCTRL: usize = 0x4000_1000;
    const PORTA: usize = 0x4100_4400;
    // VBUS_DET is PA05 on the Sensor-Watch SAM L22 package.
    const VBUS_DET_MASK: u32 = 1 << 5;
    const PORT_DIRCLR_OFFSET: usize = 0x04;
    const PORT_PINCFG_OFFSET: usize = 0x40;
    const PORT_PINCFG_INEN: u8 = 1 << 1;
    const PORT_PINCFG_PULLEN: u8 = 1 << 2;
    const DFLL_STATUS_READY: u32 = 1 << 8;
    const DFLL_STATUS_LOCK_FINE: u32 = 1 << 10;
    const DFLL_STATUS_LOCK_COARSE: u32 = 1 << 11;
    const GCLK_SOURCE_DFLL48M: u32 = 7 << 8;
    const GCLK_CHEN: u32 = 1 << 6;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct DescriptorBank {
        addr: u32,
        pcksize: u32,
        extreg: u16,
        status_bk: u8,
        reserved: [u8; 5],
    }
    const _: () = assert!(core::mem::size_of::<DescriptorBank>() == USB_DESCRIPTOR_BANK_SIZE);
    const _: () =
        assert!(core::mem::size_of::<[DescriptorBank; 2]>() == USB_DESCRIPTOR_BANK_STRIDE);
    #[used]
    #[unsafe(link_section = ".usb_ram")]
    static mut USB_RAM: [u8; USB_DPRAM_SIZE] = [0; USB_DPRAM_SIZE];
    static mut MACHINE: ControlMachine = ControlMachine::new();
    static mut TX_CURSOR: usize = 0;
    static mut TX_LIMIT: usize = 0;
    static mut TX_CONFIG: bool = false;
    static mut CONTROLLER_ACTIVE: bool = false;
    static mut SUSPENDED: bool = false;

    unsafe fn r8(offset: usize) -> u8 {
        read_volatile((USB_REGISTER_BASE + offset) as *const u8)
    }
    unsafe fn w8(offset: usize, value: u8) {
        write_volatile((USB_REGISTER_BASE + offset) as *mut u8, value);
    }
    unsafe fn r16(offset: usize) -> u16 {
        read_volatile((USB_REGISTER_BASE + offset) as *const u16)
    }
    unsafe fn w16(offset: usize, value: u16) {
        write_volatile((USB_REGISTER_BASE + offset) as *mut u16, value);
    }
    unsafe fn w32_at(address: usize, value: u32) {
        write_volatile(address as *mut u32, value);
    }
    fn usb_ram_base() -> usize {
        core::ptr::addr_of!(USB_RAM) as usize
    }

    fn wait_not_busy() -> Result<(), UsbError> {
        for _ in 0..100_000 {
            if unsafe { r8(USB_SYNCBUSY_OFFSET) } == 0 {
                return Ok(());
            }
        }
        Err(UsbError::Timeout)
    }

    fn vbus_present() -> bool {
        unsafe { read_volatile((PORTA + 0x20) as *const u32) & VBUS_DET_MASK != 0 }
    }

    fn reset_software_state() {
        unsafe {
            let machine = &raw mut MACHINE;
            (*machine).reset();
            TX_CURSOR = 0;
            TX_LIMIT = 0;
            TX_CONFIG = false;
        }
    }

    pub fn sync_transport(transport: &mut CdcTransport) {
        if !vbus_present() {
            transport.disconnect();
            return;
        }
        transport.on_vbus(true);
        let state = unsafe { (&*(&raw const MACHINE)).state() };
        match state {
            ControlState::Default => transport.on_bus_reset(),
            ControlState::Addressed => transport.on_addressed(),
            ControlState::Configured => transport.on_configured(true),
        }
    }

    fn clock_and_pins() -> Result<(), UsbError> {
        unsafe {
            // PA05 is VBUS_DET: configure it before sampling the cable state.
            write_volatile((PORTA + PORT_DIRCLR_OFFSET) as *mut u32, VBUS_DET_MASK);
            write_volatile(
                (PORTA + PORT_PINCFG_OFFSET + 5) as *mut u8,
                PORT_PINCFG_INEN | PORT_PINCFG_PULLEN,
            );
        }
        if !vbus_present() {
            return Err(UsbError::NoVbus);
        }
        unsafe {
            // DFLL48M from the documented 32 kHz reference, USBCRM enabled.
            // DFLLCTRL is 16-bit; DFLLVAL and DFLLMUL are 32-bit.
            w16(OSCCTRL + OSCCTRL_DFLLCTRL_OFFSET, (1 << 5) | (1 << 6));
            // DFLLSYNC.READREQ is the documented latch request for DFLLVAL.
            write_volatile((OSCCTRL + OSCCTRL_DFLLSYNC_OFFSET) as *mut u8, 1 << 7);
            let otp5 = read_volatile(0x0080_6020 as *const u32);
            let coarse = (otp5 >> 26) & 0x3f;
            w32_at(
                OSCCTRL + OSCCTRL_DFLLVAL_OFFSET,
                (0x200 & 0x3ff) | (coarse << 10),
            );
            w32_at(
                OSCCTRL + OSCCTRL_DFLLMUL_OFFSET,
                1465 | (1 << 16) | (1 << 26),
            );
            w16(
                OSCCTRL + OSCCTRL_DFLLCTRL_OFFSET,
                (1 << 1) | (1 << 2) | (1 << 5) | (1 << 6),
            );
            let mut locked = false;
            for _ in 0..100_000 {
                let status = read_volatile((OSCCTRL + OSCCTRL_STATUS_OFFSET) as *const u32);
                if status & (DFLL_STATUS_READY | DFLL_STATUS_LOCK_FINE | DFLL_STATUS_LOCK_COARSE)
                    == (DFLL_STATUS_READY | DFLL_STATUS_LOCK_FINE | DFLL_STATUS_LOCK_COARSE)
                {
                    locked = true;
                    break;
                }
            }
            if !locked {
                return Err(UsbError::Timeout);
            }
            // GCLK1 is sourced by DFLL48M; USB peripheral channel 6 uses GCLK1.
            w32_at(
                GCLK + GCLK_GENCTRL_OFFSET + GCLK1_INDEX * 4,
                GCLK_SOURCE_DFLL48M | (1 << 16),
            );
            for _ in 0..100_000 {
                if read_volatile((GCLK + 0x04) as *const u32) & (1 << GCLK1_INDEX) == 0 {
                    break;
                }
            }
            w32_at(
                GCLK + GCLK_PCHCTRL_OFFSET + GCLK_USB_INDEX * 4,
                GCLK1_INDEX as u32 | GCLK_CHEN,
            );
            write_volatile(
                (MCLK + 0x10) as *mut u32,
                read_volatile((MCLK + 0x10) as *const u32) | (1 << 4),
            );
            write_volatile(
                (MCLK + 0x18) as *mut u32,
                read_volatile((MCLK + 0x18) as *const u32) | 1,
            );
            // PA24/PA25: output-low before switching to mux G (USB DM/DP).
            write_volatile(
                (PORTA + 0x00) as *mut u32,
                read_volatile((PORTA + 0x00) as *const u32) | (3 << 24),
            );
            write_volatile(
                (PORTA + 0x10) as *mut u32,
                read_volatile((PORTA + 0x10) as *const u32) & !(3 << 24),
            );
            write_volatile((PORTA + 0x30 + 12) as *mut u8, 0x66);
            write_volatile((PORTA + 0x40 + 24) as *mut u8, 1);
            // The VBUS input was configured before the initial cable check.
        }
        Ok(())
    }

    fn bank(endpoint: usize, bank: usize) -> *mut DescriptorBank {
        (usb_ram_base() + endpoint * USB_DESCRIPTOR_BANK_STRIDE + bank * USB_DESCRIPTOR_BANK_SIZE)
            as *mut DescriptorBank
    }

    fn configure_ep0() {
        unsafe {
            let out = bank(0, 0);
            let input = bank(0, 1);
            (*out).addr = (usb_ram_base() + USB_EP0_OUT_BUFFER) as u32;
            (*out).pcksize = USB_PCKSIZE_SIZE_64 | USB_PCKSIZE_MULTI_PACKET_64;
            (*out).extreg = 0;
            (*out).status_bk = 0;
            (*input).addr = (usb_ram_base() + USB_EP0_IN_BUFFER) as u32;
            (*input).pcksize = USB_PCKSIZE_SIZE_64;
            (*input).extreg = 0;
            (*input).status_bk = 0;
            w8(
                USB_ENDPOINTS_OFFSET,
                USB_EPCFG_CONTROL | (USB_EPCFG_CONTROL << 4),
            );
            w8(
                USB_ENDPOINTS_OFFSET + 0x04,
                USB_EPSTATUSCLR_BK0RDY | USB_EPSTATUSCLR_BK1RDY,
            );
            w8(
                USB_ENDPOINTS_OFFSET + 0x09,
                USB_EPINT_RXSTP | USB_EPINT_TRCPT0 | USB_EPINT_TRCPT1,
            );
            w8(USB_ENDPOINTS_OFFSET + 0x05, USB_EPSTATUS_BK0RDY);
            // Bulk endpoint packet-memory layout is deliberately not enabled.
            // The DFP register map is known, but SRAM ownership/completion
            // behavior still requires an analyzer-backed hardware pass.
            if BULK_HARDWARE_PROVEN {
                // Kept as a review gate; no unproven endpoint writes occur.
                core::hint::black_box(CDC_NOTIFICATION_BUFFER_SIZE);
                core::hint::black_box(CDC_BULK_OUT_BUFFER_SIZE);
                core::hint::black_box(CDC_BULK_IN_BUFFER_SIZE);
            }
        }
    }

    pub fn init() -> Result<(), UsbError> {
        clock_and_pins()?;
        unsafe {
            w8(USB_CTRLA_OFFSET, USB_CTRLA_SWRST);
            wait_not_busy()?;
            w16(USB_CTRLB_OFFSET, USB_CTRLB_SPDCONF_FS | USB_CTRLB_DETACH);
            w32_at(
                USB_REGISTER_BASE + USB_DESCADD_OFFSET,
                usb_ram_base() as u32,
            );
            // OTP5 and its USB calibration masks are documented by the pack.
            let otp5 = read_volatile(0x0080_6020 as *const u32);
            let padcal =
                ((otp5 >> 18) & 0x1f) | (((otp5 >> 13) & 0x1f) << 6) | (((otp5 >> 23) & 0x7) << 12);
            w16(USB_PADCAL_OFFSET, padcal as u16);
            configure_ep0();
            w16(USB_INTFLAG_OFFSET, 0xffff);
            w8(USB_CTRLA_OFFSET, USB_CTRLA_ENABLE | USB_CTRLA_RUNSTDBY);
            wait_not_busy()?;
            w16(USB_CTRLB_OFFSET, USB_CTRLB_SPDCONF_FS);
        }
        unsafe {
            CONTROLLER_ACTIVE = true;
            SUSPENDED = false;
        }
        Ok(())
    }

    pub fn poll() -> Result<(), UsbError> {
        if !vbus_present() {
            unsafe {
                w16(USB_CTRLB_OFFSET, USB_CTRLB_DETACH);
                w8(USB_CTRLA_OFFSET, 0);
                w8(USB_DADD_OFFSET, 0);
                CONTROLLER_ACTIVE = false;
                SUSPENDED = false;
            }
            reset_software_state();
            return Err(UsbError::NoVbus);
        }
        unsafe {
            if !CONTROLLER_ACTIVE || SUSPENDED {
                let _ = init();
                return Ok(());
            }
            let flags = r16(USB_INTFLAG_OFFSET);
            if flags & USB_INTFLAG_SUSPEND != 0 {
                w16(USB_INTFLAG_OFFSET, USB_INTFLAG_SUSPEND);
                w16(USB_CTRLB_OFFSET, USB_CTRLB_DETACH);
                SUSPENDED = true;
                return Ok(());
            }
            if flags & USB_INTFLAG_EORST != 0 {
                w16(USB_INTFLAG_OFFSET, USB_INTFLAG_EORST);
                w8(USB_DADD_OFFSET, 0);
                reset_software_state();
                configure_ep0();
            }
            if flags & (USB_INTFLAG_WAKEUP | USB_INTFLAG_EORSM) != 0 {
                w16(
                    USB_INTFLAG_OFFSET,
                    flags & (USB_INTFLAG_WAKEUP | USB_INTFLAG_EORSM),
                );
                let _ = init();
                return Ok(());
            }
            let ep_flags = r8(USB_ENDPOINTS_OFFSET + 0x07);
            // Clear error/stall flags every pass; never spin on a failed transfer.
            if ep_flags & USB_EPINT_ERRORS != 0 {
                w8(USB_ENDPOINTS_OFFSET + 0x07, ep_flags & USB_EPINT_ERRORS);
                configure_ep0();
            }
            if ep_flags & USB_EPINT_RXSTP != 0 {
                service_setup();
                w8(USB_ENDPOINTS_OFFSET + 0x07, USB_EPINT_RXSTP);
            }
            if ep_flags & USB_EPINT_TRCPT1 != 0 {
                service_in();
                w8(USB_ENDPOINTS_OFFSET + 0x07, USB_EPINT_TRCPT1);
            }
            // Re-arm only after bounded handling; no bulk endpoints are enabled.
            if r8(USB_ENDPOINTS_OFFSET + 0x06) & USB_EPSTATUS_BK0RDY == 0 {
                w8(USB_ENDPOINTS_OFFSET + 0x05, USB_EPSTATUS_BK0RDY);
            }
        }
        Ok(())
    }

    unsafe fn service_setup() {
        let p = (usb_ram_base() + USB_EP0_OUT_BUFFER) as *const u8;
        let setup = SetupPacket {
            bm_request_type: read_volatile(p),
            request: read_volatile(p.add(1)),
            value: u16::from_le_bytes([read_volatile(p.add(2)), read_volatile(p.add(3))]),
            index: u16::from_le_bytes([read_volatile(p.add(4)), read_volatile(p.add(5))]),
            length: u16::from_le_bytes([read_volatile(p.add(6)), read_volatile(p.add(7))]),
        };
        let machine = &raw mut MACHINE;
        match handle_setup(&mut *machine, setup) {
            ControlResponse::Data { offset, length } => {
                TX_CONFIG = offset != 0;
                TX_CURSOR = core::cmp::min(length, MAX_PACKET_SIZE);
                TX_LIMIT = length;
                send_descriptor_packet(TX_CONFIG, 0, TX_CURSOR);
            }
            ControlResponse::DataValue(value) => {
                TX_CURSOR = 1;
                TX_LIMIT = 1;
                let dst = (usb_ram_base() + USB_EP0_IN_BUFFER) as *mut u8;
                write_volatile(dst, value);
                (*(bank(0, 1))).pcksize = USB_PCKSIZE_SIZE_64 | 1;
                w8(USB_ENDPOINTS_OFFSET + 0x05, USB_EPSTATUS_BK1RDY);
            }
            ControlResponse::Status => {
                TX_CURSOR = 0;
                TX_LIMIT = 0;
                (*(bank(0, 1))).pcksize = USB_PCKSIZE_SIZE_64;
                w8(USB_ENDPOINTS_OFFSET + 0x05, USB_EPSTATUS_BK1RDY);
            }
            ControlResponse::Stall => {
                TX_CURSOR = 0;
                TX_LIMIT = 0;
                w8(USB_ENDPOINTS_OFFSET + 0x05, 1 << 5);
            }
        }
    }

    unsafe fn send_descriptor_packet(config: bool, start: usize, length: usize) {
        let src = if config {
            &CONFIGURATION_DESCRIPTOR[..]
        } else {
            &DEVICE_DESCRIPTOR[..]
        };
        core::ptr::copy_nonoverlapping(
            src.as_ptr().add(start),
            (usb_ram_base() + USB_EP0_IN_BUFFER) as *mut u8,
            length,
        );
        (*(bank(0, 1))).pcksize = USB_PCKSIZE_SIZE_64 | length as u32;
        w8(USB_ENDPOINTS_OFFSET + 0x05, USB_EPSTATUS_BK1RDY);
    }

    unsafe fn service_in() {
        // SET_ADDRESS takes effect only after the status-IN handshake completes.
        if let Some(address) = (&mut *(&raw mut MACHINE)).complete_status() {
            w8(
                USB_DADD_OFFSET,
                if address == 0 { 0 } else { address | (1 << 7) },
            );
        }
        if TX_CURSOR >= TX_LIMIT {
            return;
        }
        let remaining = TX_LIMIT - TX_CURSOR;
        let count = core::cmp::min(remaining, MAX_PACKET_SIZE);
        let start = TX_CURSOR;
        TX_CURSOR += count;
        send_descriptor_packet(TX_CONFIG, start, count);
    }
}

#[cfg(target_arch = "arm")]
#[allow(unused_imports)]
pub use hardware::{init, poll};

/// Poll the controller and synchronize the one application CDC session with
/// the controller's EP0 state. Bulk endpoints remain disabled until the proof
/// gate is reviewed, but no second transport or shadow session is created.
#[cfg(target_arch = "arm")]
pub fn poll_transport(transport: &mut CdcTransport) -> Result<(), UsbError> {
    let result = hardware::poll();
    hardware::sync_transport(transport);
    result
}

#[cfg(not(target_arch = "arm"))]
pub fn poll_transport(transport: &mut CdcTransport) -> Result<(), UsbError> {
    transport.disconnect();
    Err(UsbError::NoVbus)
}

#[cfg(not(target_arch = "arm"))]
pub fn init() -> Result<(), UsbError> {
    Err(UsbError::NoVbus)
}
#[cfg(not(target_arch = "arm"))]
pub fn poll() -> Result<(), UsbError> {
    Err(UsbError::NoVbus)
}
pub fn write(_bytes: &[u8]) -> Result<usize, UsbError> {
    Err(UsbError::UnsafeHardware)
}
pub fn read() -> Result<Option<u8>, UsbError> {
    Err(UsbError::UnsafeHardware)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(direction: u8, request: u8, value: u16, length: u16) -> SetupPacket {
        SetupPacket {
            bm_request_type: direction,
            request,
            value,
            index: 0,
            length,
        }
    }

    #[test]
    fn host_control_state_covers_enumeration_sequence() {
        let mut machine = ControlMachine::new();
        assert_eq!(
            handle_setup(&mut machine, setup(0x80, 6, 0x0100, 18)),
            ControlResponse::Data {
                offset: 0,
                length: 18
            }
        );
        assert_eq!(
            handle_setup(&mut machine, setup(0, 5, 7, 0)),
            ControlResponse::Status
        );
        assert_eq!(machine.state(), ControlState::Default);
        assert_eq!(machine.complete_status(), Some(7));
        assert_eq!(machine.state(), ControlState::Addressed);
        assert_eq!(
            handle_setup(&mut machine, setup(0x80, 8, 0, 1)),
            ControlResponse::DataValue(0)
        );
        assert_eq!(
            handle_setup(&mut machine, setup(0, 9, 1, 0)),
            ControlResponse::Status
        );
        assert_eq!(machine.state(), ControlState::Configured);
        assert_eq!(
            handle_setup(&mut machine, setup(0x80, 8, 0, 1)),
            ControlResponse::DataValue(1)
        );
    }

    #[test]
    fn host_control_rejects_bad_requests_and_bounds_data() {
        let mut machine = ControlMachine::new();
        assert_eq!(
            handle_setup(&mut machine, setup(0x80, 6, 0x0200, 255)),
            ControlResponse::Data {
                offset: 18,
                length: 75
            }
        );
        assert_eq!(
            handle_setup(&mut machine, setup(0x80, 6, 0x0300, 4)),
            ControlResponse::Stall
        );
        assert_eq!(
            handle_setup(&mut machine, setup(0, 5, 128, 0)),
            ControlResponse::Stall
        );
        assert_eq!(
            handle_setup(&mut machine, setup(0x80, 8, 0, 2)),
            ControlResponse::Stall
        );
    }

    #[test]
    fn cdc_state_transitions_and_lifecycle_clear_buffers() {
        let mut transport = CdcTransport::new();
        assert_eq!(transport.state(), UsbState::Detached);
        assert_eq!(transport.connection_state(), ConnectionState::Disconnected);
        transport.on_vbus(true);
        assert_eq!(transport.state(), UsbState::Default);
        assert_eq!(transport.connection_state(), ConnectionState::Connected);
        transport.on_configured(true);
        assert_eq!(transport.state(), UsbState::Configured);
        transport.on_suspend();
        assert_eq!(transport.state(), UsbState::Suspended);
        assert_eq!(transport.connection_state(), ConnectionState::Suspended);
        transport.on_resume();
        assert_eq!(transport.state(), UsbState::Configured);
        transport.on_bus_reset();
        assert_eq!(transport.state(), UsbState::Default);
        transport.disconnect();
        assert_eq!(transport.state(), UsbState::Detached);
        assert_eq!(transport.connection_state(), ConnectionState::Disconnected);
    }

    #[test]
    fn cdc_bounds_and_not_enumerated_are_explicit() {
        let mut transport = CdcTransport::new();
        assert_eq!(transport.write(&[0; 65]), Err(CdcError::Overflow));
        assert_eq!(transport.write(b"ping"), Err(CdcError::NotEnumerated));
        assert_eq!(transport.read(), Err(CdcError::NotEnumerated));
        transport.on_vbus(true);
        transport.on_configured(true);
        assert_eq!(
            transport.accept_rx_packet(&[0; 65]),
            Err(CdcError::Overflow)
        );
        for _ in 0..CDC_QUEUE_DEPTH {
            assert_eq!(transport.write(b"x"), Ok(1));
        }
        assert_eq!(transport.write(b"x"), Err(CdcError::Overflow));
        transport.disconnect();
        assert_eq!(transport.read(), Err(CdcError::NotEnumerated));
    }

    #[test]
    fn cdc_queue_framing_and_read_only_adapter_are_bounded() {
        let mut transport = CdcTransport::new();
        transport.on_vbus(true);
        transport.on_configured(true);
        transport.accept_rx_packet(b"ping\r\n").unwrap();
        assert_eq!(transport.next_command(), Ok(Some(ReadOnlyCommand::Ping)));
        transport.accept_rx_packet(b"erase\n").unwrap();
        assert_eq!(transport.next_command(), Err(CdcError::Unsupported));
        transport
            .accept_rx_packet(&[b'a'; CDC_LINE_MAX + 1])
            .unwrap();
        assert_eq!(transport.next_command(), Err(CdcError::Overflow));
    }

    #[test]
    fn cdc_queue_overflow_does_not_drop_existing_packets() {
        let mut transport = CdcTransport::new();
        transport.on_vbus(true);
        transport.on_configured(true);
        for _ in 0..CDC_QUEUE_DEPTH {
            transport.accept_rx_packet(b"x").unwrap();
        }
        assert_eq!(transport.accept_rx_packet(b"y"), Err(CdcError::Overflow));
        for _ in 0..CDC_QUEUE_DEPTH {
            assert_eq!(transport.read(), Ok(Some(b'x')));
        }
    }

    #[test]
    fn cdc_requests_decode_line_coding_and_control_state() {
        let mut transport = CdcTransport::new();
        transport.on_vbus(true);
        transport.on_configured(true);
        let coding = [0x80, 0x25, 0, 0, 0, 0, 8];
        assert_eq!(
            handle_cdc_setup(
                &mut transport,
                setup(0x21, CDC_REQ_SET_LINE_CODING, 0, 7),
                Some(coding),
            ),
            Ok(CdcControlResponse::Accepted)
        );
        assert_eq!(transport.line_coding().baud_rate, 9_600);
        assert_eq!(
            handle_cdc_setup(
                &mut transport,
                setup(0xA1, CDC_REQ_GET_LINE_CODING, 0, 7),
                None,
            ),
            Ok(CdcControlResponse::LineCoding(transport.line_coding()))
        );
        assert_eq!(
            handle_cdc_setup(
                &mut transport,
                setup(0x21, CDC_REQ_SET_CONTROL_LINE_STATE, 3, 0),
                None,
            ),
            Ok(CdcControlResponse::Accepted)
        );
        assert_eq!(transport.control_line_state(), 3);
    }

    #[test]
    fn cdc_line_coding_contract_does_not_mutate_on_invalid_request() {
        let mut transport = CdcTransport::new();
        let original = transport.line_coding();
        assert_eq!(
            transport.handle_control_request(CdcControlRequest::GetLineCoding { length: 6 }),
            Err(CdcError::NotEnumerated)
        );
        transport.on_vbus(true);
        transport.on_configured(true);
        assert_eq!(
            transport.handle_control_request(CdcControlRequest::GetLineCoding { length: 6 }),
            Err(CdcError::Unsupported)
        );
        assert_eq!(transport.line_coding(), original);
        transport.disconnect();
        assert_eq!(
            transport.handle_control_request(CdcControlRequest::SetLineCoding {
                length: 7,
                coding: LineCoding {
                    baud_rate: 9_600,
                    stop_bits: 0,
                    parity: 0,
                    data_bits: 8,
                },
            }),
            Err(CdcError::NotEnumerated)
        );
        // Line coding is a control contract only until CDC transfers are proven.
        assert_eq!(transport.line_coding(), original);
    }

    #[test]
    fn cdc_allowlist_is_read_only_and_has_no_execution_path() {
        assert_eq!(
            CdcTransport::allow_read_only_command(b"ping"),
            Ok(ReadOnlyCommand::Ping)
        );
        assert_eq!(
            CdcTransport::allow_read_only_command(b"help"),
            Ok(ReadOnlyCommand::Help)
        );
        assert_eq!(
            CdcTransport::allow_read_only_command(b"test identity"),
            Ok(ReadOnlyCommand::TestIdentity)
        );
        for command in [b"set".as_slice(), b"write", b"erase", b"echo"] {
            assert_eq!(
                CdcTransport::allow_read_only_command(command),
                Err(CdcError::Unsupported)
            );
        }
    }

    #[test]
    fn descriptor_contract_is_16_byte_banks_and_32_byte_groups() {
        assert_eq!(USB_DESCRIPTOR_BANK_SIZE, 16);
        assert_eq!(USB_DESCRIPTOR_BANK_STRIDE, 32);
        assert_eq!(USB_DESCRIPTOR_TABLE_SIZE, 256);
        assert_eq!(USB_EP0_OUT_BUFFER, 256);
        assert_eq!(USB_EP0_IN_BUFFER, 320);
        assert_eq!(DEVICE_DESCRIPTOR.len(), 18);
        assert_eq!(CONFIGURATION_DESCRIPTOR.len(), 75);
        assert_eq!(CONFIGURATION_DESCRIPTOR[2], 75);
        assert_eq!(CONFIGURATION_DESCRIPTOR[4], 2);
        assert!(
            CONFIGURATION_DESCRIPTOR
                .windows(2)
                .any(|window| window == [5, NOTIFICATION_ENDPOINT])
        );
        assert!(
            CONFIGURATION_DESCRIPTOR
                .windows(2)
                .any(|window| window == [5, RX_ENDPOINT])
        );
        assert!(
            CONFIGURATION_DESCRIPTOR
                .windows(2)
                .any(|window| window == [5, TX_ENDPOINT])
        );
        assert_eq!(CDC_NOTIFICATION_BUFFER_SIZE, 64);
        assert!(!BULK_HARDWARE_PROVEN);
    }
}
