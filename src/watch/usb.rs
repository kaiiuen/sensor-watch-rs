//! Developer-only SAM L22 USB full-speed enumeration layer.
//!
//! This is intentionally limited to EP0 enumeration. It does not implement
//! CDC bulk endpoints or expose a shell transport. Hardware constants are
//! taken from Microchip.SAML22_DFP.3.8.203.atpack and the pinned TinyUSB
//! reference at 5572168994a29266df6cbf12b46919498d3ece66.

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

/// CDC configuration is retained as a descriptor review fixture only.
/// No CDC bulk endpoint is enabled by this module.
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
pub use hardware::{init, poll};
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
    fn descriptor_contract_is_16_byte_banks_and_32_byte_groups() {
        assert_eq!(USB_DESCRIPTOR_BANK_SIZE, 16);
        assert_eq!(USB_DESCRIPTOR_BANK_STRIDE, 32);
        assert_eq!(USB_DESCRIPTOR_TABLE_SIZE, 256);
        assert_eq!(USB_EP0_OUT_BUFFER, 256);
        assert_eq!(USB_EP0_IN_BUFFER, 320);
        assert_eq!(DEVICE_DESCRIPTOR.len(), 18);
        assert_eq!(CONFIGURATION_DESCRIPTOR.len(), 75);
    }
}
