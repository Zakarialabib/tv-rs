//! Native CEC bindings wrapping libcec / cec-rs FFI.
//!
//! Confines all `unsafe` code for native CEC interaction strictly to this module.

#![allow(unsafe_code)]
#![allow(missing_docs)]

use std::ffi::{c_char, c_int, c_void, CString};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::cec::error::CecError;
use crate::cec::transport::CecTransport;
use crate::cec::types::CecCommand;

/// The maximum parameter bytes attached to a CEC frame.
pub const CEC_MAX_DATA_PACKET_SIZE: usize = 64;

/// Size of OSD device name buffer in libcec_configuration (including null terminator).
/// Libcec configuration allows at most 14 characters + null terminator = 15 bytes.
pub const LIBCEC_OSD_NAME_SIZE: usize = 15;

/// Maximum length of OSD name characters according to HDMI-CEC specification (14 bytes).
pub const CEC_MAX_OSD_NAME_LEN: usize = 14;

/// CEC Opcode for SET_OSD_NAME (0x47 = 71).
pub const CEC_OPCODE_SET_OSD_NAME: u8 = 0x47;

/// Opaque libcec connection pointer.
pub type LibcecConnectionT = *mut c_void;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct CecDataPacket {
    pub data: [u8; CEC_MAX_DATA_PACKET_SIZE],
    pub size: u8,
}

impl Default for CecDataPacket {
    fn default() -> Self {
        Self {
            data: [0u8; CEC_MAX_DATA_PACKET_SIZE],
            size: 0,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct CecRawCommand {
    pub initiator: c_int,
    pub destination: c_int,
    pub ack: i8,
    pub eom: i8,
    pub opcode: c_int,
    pub parameters: CecDataPacket,
    pub opcode_set: i8,
    pub transmit_timeout: i32,
}

impl Default for CecRawCommand {
    fn default() -> Self {
        Self {
            initiator: -1,
            destination: 0,
            ack: 0,
            eom: 1,
            opcode: 0,
            parameters: CecDataPacket::default(),
            opcode_set: 1,
            transmit_timeout: 1000,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct CecLogicalAddressesRaw {
    pub primary: c_int,
    pub addresses: [c_int; 16],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct CecDeviceTypeListRaw {
    pub types: [c_int; 5],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct IcecCallbacksRaw {
    pub log_message: Option<extern "C" fn(*mut c_void, *const c_void)>,
    pub key_press: Option<extern "C" fn(*mut c_void, *const c_void)>,
    pub command_received: Option<extern "C" fn(*mut c_void, *const c_void)>,
    pub configuration_changed: Option<extern "C" fn(*mut c_void, *const c_void)>,
    pub alert: Option<extern "C" fn(*mut c_void, c_int, *const c_void)>,
    pub menu_state_changed: Option<extern "C" fn(*mut c_void, c_int) -> c_int>,
    pub source_activated: Option<extern "C" fn(*mut c_void, c_int, u8)>,
    pub command_handler: Option<extern "C" fn(*mut c_void, *const c_void) -> c_int>,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct LibcecConfigurationRaw {
    pub client_version: u32,
    pub str_device_name: [c_char; LIBCEC_OSD_NAME_SIZE],
    pub device_types: CecDeviceTypeListRaw,
    pub b_autodetect_address: u8,
    pub i_physical_address: u16,
    pub base_device: c_int,
    pub i_hdmi_port: u8,
    pub tv_vendor: u32,
    pub wake_devices: CecLogicalAddressesRaw,
    pub power_off_devices: CecLogicalAddressesRaw,
    pub server_version: u32,
    pub b_get_settings_from_rom: u8,
    pub b_activate_source: u8,
    pub b_power_off_on_standby: u8,
    pub callback_param: *mut c_void,
    pub callbacks: *mut IcecCallbacksRaw,
    pub logical_addresses: CecLogicalAddressesRaw,
    pub i_firmware_version: u16,
    pub str_device_language: [c_char; 3],
    pub i_firmware_build_date: u32,
    pub b_monitor_only: u8,
    pub cec_version: c_int,
    pub adapter_type: c_int,
    pub combo_key: c_int,
    pub i_combo_key_timeout_ms: u32,
    pub i_button_repeat_rate_ms: u32,
    pub i_button_release_delay_ms: u32,
    pub i_double_tap_timeout_ms: u32,
    pub b_auto_wake_avr: u8,
    pub b_auto_power_on: u8,
    pub b_autonomous_mode: u8,
    pub i_button_repeat_delay_ms: u32,
    pub i_device_vendor_id: u32,
}

impl Default for LibcecConfigurationRaw {
    fn default() -> Self {
        // SAFETY: LibcecConfigurationRaw is plain data and valid as zeroed memory.
        unsafe { std::mem::zeroed() }
    }
}

type FnLibcecInitialise = unsafe extern "C" fn(*mut LibcecConfigurationRaw) -> LibcecConnectionT;
type FnLibcecDestroy = unsafe extern "C" fn(LibcecConnectionT);
type FnLibcecOpen = unsafe extern "C" fn(LibcecConnectionT, *const c_char, u32) -> c_int;
type FnLibcecClose = unsafe extern "C" fn(LibcecConnectionT);
type FnLibcecPowerOnDevices = unsafe extern "C" fn(LibcecConnectionT, c_int) -> c_int;
type FnLibcecStandbyDevices = unsafe extern "C" fn(LibcecConnectionT, c_int) -> c_int;
type FnLibcecSetActiveSource = unsafe extern "C" fn(LibcecConnectionT, c_int) -> c_int;
type FnLibcecTransmit = unsafe extern "C" fn(LibcecConnectionT, *const CecRawCommand) -> c_int;

/// Function table for libcec C symbols.
#[allow(dead_code)]
struct LibcecSymbols {
    _lib: *mut c_void,
    initialise: FnLibcecInitialise,
    destroy: FnLibcecDestroy,
    open: FnLibcecOpen,
    close: FnLibcecClose,
    power_on_devices: FnLibcecPowerOnDevices,
    standby_devices: FnLibcecStandbyDevices,
    set_active_source: FnLibcecSetActiveSource,
    transmit: FnLibcecTransmit,
}

impl Drop for LibcecSymbols {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            if !self._lib.is_null() {
                libc::dlclose(self._lib);
            }
        }
    }
}

// SAFETY: LibcecSymbols function pointers are immutable once loaded and safe across threads.
unsafe impl Send for LibcecSymbols {}
unsafe impl Sync for LibcecSymbols {}

impl LibcecSymbols {
    #[cfg(unix)]
    fn load() -> Result<Self, CecError> {
        let candidates = [
            "libcec.so",
            "libcec.so.8",
            "libcec.so.7",
            "libcec.so.6",
            "libcec.so.5",
            "libcec.so.4",
            "libcec.dylib",
        ];

        let mut handle = std::ptr::null_mut();
        for &name in &candidates {
            let c_name = CString::new(name).map_err(|e| CecError::Io(e.to_string()))?;
            handle = unsafe { libc::dlopen(c_name.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
            if !handle.is_null() {
                break;
            }
        }

        if handle.is_null() {
            return Err(CecError::Unavailable(
                "native libcec shared library not found (install libcec or libcec-dev)".to_string(),
            ));
        }

        unsafe fn load_sym<T>(lib: *mut c_void, name: &str) -> Result<T, CecError> {
            let c_name = CString::new(name).map_err(|e| CecError::Io(e.to_string()))?;
            let sym = libc::dlsym(lib, c_name.as_ptr());
            if sym.is_null() {
                Err(CecError::Unavailable(format!(
                    "missing symbol '{name}' in libcec"
                )))
            } else {
                Ok(std::mem::transmute_copy(&sym))
            }
        }

        unsafe {
            Ok(Self {
                _lib: handle,
                initialise: load_sym(handle, "libcec_initialise")?,
                destroy: load_sym(handle, "libcec_destroy")?,
                open: load_sym(handle, "libcec_open")?,
                close: load_sym(handle, "libcec_close")?,
                power_on_devices: load_sym(handle, "libcec_power_on_devices")?,
                standby_devices: load_sym(handle, "libcec_standby_devices")?,
                set_active_source: load_sym(handle, "libcec_set_active_source")?,
                transmit: load_sym(handle, "libcec_transmit")?,
            })
        }
    }

    #[cfg(not(unix))]
    fn load() -> Result<Self, CecError> {
        Err(CecError::Unavailable(
            "native libcec dynamic loading is only supported on Unix targets".to_string(),
        ))
    }
}

/// A handle to an open native libcec connection.
pub struct NativeConnection {
    symbols: Arc<LibcecSymbols>,
    conn: LibcecConnectionT,
}

impl Drop for NativeConnection {
    fn drop(&mut self) {
        if !self.conn.is_null() {
            unsafe {
                (self.symbols.close)(self.conn);
                (self.symbols.destroy)(self.conn);
            }
            self.conn = std::ptr::null_mut();
        }
    }
}

// SAFETY: libcec serializes access to ICECAdapter internally and connection pointers are movable across threads.
unsafe impl Send for NativeConnection {}
unsafe impl Sync for NativeConnection {}

/// Validates raw CEC frame bytes for protocol compliance and security bounds before FFI transmission.
pub fn validate_cec_frame(bytes: &[u8]) -> Result<(), CecError> {
    if bytes.is_empty() {
        return Err(CecError::Rejected("empty cec frame".to_string()));
    }

    let header = bytes[0];
    let initiator = header >> 4;
    let destination = header & 0x0F;

    if initiator > 15 || destination > 15 {
        return Err(CecError::Rejected(format!(
            "invalid cec logical address in header: initiator={initiator}, destination={destination}"
        )));
    }

    if bytes.len() > 1 {
        let opcode = bytes[1];
        let parameters = &bytes[2..];

        if parameters.len() > CEC_MAX_DATA_PACKET_SIZE {
            return Err(CecError::Rejected(format!(
                "cec frame parameters exceed max size (got {} bytes, max {})",
                parameters.len(),
                CEC_MAX_DATA_PACKET_SIZE
            )));
        }

        // Validate SET_OSD_NAME (0x47 / 71)
        if opcode == CEC_OPCODE_SET_OSD_NAME {
            if parameters.is_empty() {
                return Err(CecError::Rejected(
                    "malformed SET_OSD_NAME: missing OSD name payload".to_string(),
                ));
            }
            if parameters.len() > CEC_MAX_OSD_NAME_LEN {
                return Err(CecError::Rejected(format!(
                    "malformed SET_OSD_NAME: length {} exceeds maximum allowed length of {} characters",
                    parameters.len(),
                    CEC_MAX_OSD_NAME_LEN
                )));
            }
            // Ensure characters are valid ASCII displayable / text
            if !parameters.iter().all(|&b| (0x20..=0x7E).contains(&b)) {
                return Err(CecError::Rejected(
                    "malformed SET_OSD_NAME: contains non-displayable characters".to_string(),
                ));
            }
        }
    }

    Ok(())
}

/// Safely formats an OSD device name into a fixed buffer ensuring null-termination.
pub fn format_osd_name(name: &str) -> Result<[c_char; LIBCEC_OSD_NAME_SIZE], CecError> {
    if name.is_empty() {
        return Err(CecError::Rejected(
            "device name cannot be empty".to_string(),
        ));
    }
    let name_bytes = name.as_bytes();
    if name_bytes.len() > CEC_MAX_OSD_NAME_LEN {
        return Err(CecError::Rejected(format!(
            "device name '{}' is too long ({} bytes, max {})",
            name,
            name_bytes.len(),
            CEC_MAX_OSD_NAME_LEN
        )));
    }
    if !name_bytes.iter().all(|&b| (0x20..=0x7E).contains(&b)) {
        return Err(CecError::Rejected(format!(
            "device name '{name}' contains illegal characters"
        )));
    }

    let mut buf = [0 as c_char; LIBCEC_OSD_NAME_SIZE];
    for (i, &b) in name_bytes.iter().enumerate() {
        buf[i] = b as c_char;
    }
    buf[name_bytes.len()] = 0; // Explicit null terminator
    Ok(buf)
}

/// Native CEC transport wrapping libcec via dynamic FFI binding.
pub struct NativeTransport {
    symbols: Result<Arc<LibcecSymbols>, CecError>,
    port: Option<String>,
    device_name: String,
    connection: Mutex<Option<NativeConnection>>,
}

impl std::fmt::Debug for NativeTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeTransport")
            .field("port", &self.port)
            .field("device_name", &self.device_name)
            .field("available", &self.symbols.is_ok())
            .finish()
    }
}

impl Default for NativeTransport {
    fn default() -> Self {
        Self::try_new()
    }
}

impl NativeTransport {
    /// Attempts to dynamically locate and initialize native libcec symbols.
    #[must_use]
    pub fn try_new() -> Self {
        let symbols = LibcecSymbols::load().map(Arc::new);
        Self {
            symbols,
            port: None,
            device_name: "tv-rs-cec".to_string(),
            connection: Mutex::new(None),
        }
    }

    /// Sets the specific serial/adapter port (e.g. `/dev/ttyACM0`).
    #[must_use]
    pub fn with_port(mut self, port: impl Into<String>) -> Self {
        self.port = Some(port.into());
        self
    }

    /// Sets the OSD device name for this adapter.
    #[must_use]
    pub fn with_device_name(mut self, name: impl Into<String>) -> Self {
        self.device_name = name.into();
        self
    }

    fn ensure_connection<'a>(
        &'a self,
        guard: &'a mut Option<NativeConnection>,
    ) -> Result<(&'a NativeConnection, &'a Arc<LibcecSymbols>), CecError> {
        let symbols = self
            .symbols
            .as_ref()
            .map_err(|e| CecError::Unavailable(e.to_string()))?;

        if guard.is_none() {
            let str_device_name = format_osd_name(&self.device_name)?;
            let mut device_types = CecDeviceTypeListRaw::default();
            device_types.types[0] = 1; // RecordingDevice

            let mut config = LibcecConfigurationRaw {
                client_version: 0x80000, // v8.0.0
                str_device_name,
                device_types,
                b_monitor_only: 0,
                ..Default::default()
            };

            let conn = unsafe { (symbols.initialise)(&mut config) };
            if conn.is_null() {
                return Err(CecError::Unavailable(
                    "failed to initialise native libcec".to_string(),
                ));
            }

            let port_cstr = self
                .port
                .as_deref()
                .map(|p| CString::new(p).map_err(|e| CecError::Io(e.to_string())))
                .transpose()?;

            let port_ptr = port_cstr.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());
            let open_ok = unsafe { (symbols.open)(conn, port_ptr, 5000) };
            if open_ok == 0 {
                unsafe { (symbols.destroy)(conn) };
                return Err(CecError::Unavailable(
                    "failed to open native CEC adapter".to_string(),
                ));
            }

            *guard = Some(NativeConnection {
                symbols: symbols.clone(),
                conn,
            });
        }

        let native_conn = guard.as_ref().unwrap();
        Ok((native_conn, symbols))
    }
}

#[async_trait]
impl CecTransport for NativeTransport {
    async fn send(&self, command: &str) -> Result<String, CecError> {
        let cmd = CecCommand::try_from(command)?;
        self.send_command(&cmd).await
    }

    async fn send_command(&self, command: &CecCommand) -> Result<String, CecError> {
        command.validate()?;

        let mut lock = self.connection.lock().unwrap();
        let (conn, symbols) = self.ensure_connection(&mut lock)?;

        match command {
            CecCommand::PowerOn(addr) => {
                let ret = unsafe { (symbols.power_on_devices)(conn.conn, addr.as_u8() as c_int) };
                if ret == 0 {
                    return Err(CecError::Rejected(format!(
                        "native libcec rejected power on for {addr}"
                    )));
                }
                Ok(format!("TRAFFIC: [native] power on {addr}"))
            }
            CecCommand::Standby(addr) => {
                let ret = unsafe { (symbols.standby_devices)(conn.conn, addr.as_u8() as c_int) };
                if ret == 0 {
                    return Err(CecError::Rejected(format!(
                        "native libcec rejected standby for {addr}"
                    )));
                }
                Ok(format!("TRAFFIC: [native] standby {addr}"))
            }
            CecCommand::SetActiveSource(phys) => {
                // Active source opcode: 0x82 (130) with physical address payload
                let mut data = [0u8; CEC_MAX_DATA_PACKET_SIZE];
                data[0] = (phys >> 8) as u8;
                data[1] = (phys & 0xFF) as u8;
                let parameters = CecDataPacket { size: 2, data };

                let raw = CecRawCommand {
                    initiator: 4,    // Playback1 / client
                    destination: 15, // Broadcast
                    opcode: 130,     // ActiveSource opcode
                    opcode_set: 1,
                    parameters,
                    ..Default::default()
                };

                let ret = unsafe { (symbols.transmit)(conn.conn, &raw) };
                if ret == 0 {
                    return Err(CecError::Rejected(format!(
                        "native libcec rejected active source 0x{phys:04X}"
                    )));
                }
                Ok(format!("TRAFFIC: [native] active source 0x{phys:04X}"))
            }
            CecCommand::CustomHexFrame(hex_str) => {
                let clean = hex_str.trim().strip_prefix("tx ").unwrap_or(hex_str.trim());
                let bytes_hex = clean.replace([':', ' '], "");
                if bytes_hex.len() % 2 != 0 {
                    return Err(CecError::Rejected(format!(
                        "invalid hex frame length: '{hex_str}'"
                    )));
                }

                let mut bytes = Vec::new();
                for i in (0..bytes_hex.len()).step_by(2) {
                    let b = u8::from_str_radix(&bytes_hex[i..i + 2], 16).map_err(|_| {
                        CecError::Rejected(format!("invalid hex byte in '{hex_str}'"))
                    })?;
                    bytes.push(b);
                }

                // Strictly validate frame before reaching unsafe code
                validate_cec_frame(&bytes)?;

                let header = bytes[0];
                let initiator = (header >> 4) as c_int;
                let destination = (header & 0x0F) as c_int;

                let (opcode, opcode_set, parameters) = if bytes.len() > 1 {
                    let param_bytes = &bytes[2..];
                    let mut data = [0u8; CEC_MAX_DATA_PACKET_SIZE];
                    data[..param_bytes.len()].copy_from_slice(param_bytes);
                    (
                        bytes[1] as c_int,
                        1,
                        CecDataPacket {
                            size: param_bytes.len() as u8,
                            data,
                        },
                    )
                } else {
                    (0, 0, CecDataPacket::default())
                };

                let raw = CecRawCommand {
                    initiator,
                    destination,
                    opcode,
                    opcode_set,
                    parameters,
                    ..Default::default()
                };

                let ret = unsafe { (symbols.transmit)(conn.conn, &raw) };
                if ret == 0 {
                    return Err(CecError::Rejected(format!(
                        "native libcec rejected transmit: '{hex_str}'"
                    )));
                }
                Ok(format!("TRAFFIC: [native] tx {hex_str}"))
            }
            CecCommand::Raw(raw_str) => Err(CecError::Rejected(format!(
                "arbitrary string commands not supported by native transport: '{raw_str}'"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_transport_without_library_reports_unavailable() {
        let transport = NativeTransport::try_new();
        // Since libcec is not installed in the test container, symbols loading must fail with Unavailable
        assert!(
            transport.symbols.is_err(),
            "libcec should not be available in environment"
        );
        let err = transport.symbols.as_ref().err().unwrap();
        assert!(matches!(err, CecError::Unavailable(_)));
    }

    #[test]
    fn test_format_osd_name_bounds() {
        // Valid name
        let buf = format_osd_name("TestDisplay").unwrap();
        assert_eq!(buf[0], b'T' as c_char);
        assert_eq!(buf[11], 0);

        // Max length (14 chars)
        let exact14 = "12345678901234";
        let buf14 = format_osd_name(exact14).unwrap();
        assert_eq!(buf14[14], 0);

        // Empty name
        assert!(format_osd_name("").is_err());

        // Oversized name (> 14 chars)
        let too_long = "123456789012345";
        let err = format_osd_name(too_long).unwrap_err();
        assert!(matches!(err, CecError::Rejected(_)));

        // Non-displayable characters
        assert!(format_osd_name("bad\x00name").is_err());
        assert!(format_osd_name("bad\nname").is_err());
    }

    #[test]
    fn test_validate_cec_frame_set_osd_name() {
        // Valid SET_OSD_NAME frame: header (0x10 = from 1 to 0), opcode (0x47), payload "TV"
        let valid_frame = [0x10, CEC_OPCODE_SET_OSD_NAME, b'T', b'V'];
        assert!(validate_cec_frame(&valid_frame).is_ok());

        // Max 14 characters OSD name
        let mut max_frame = vec![0x10, CEC_OPCODE_SET_OSD_NAME];
        max_frame.extend_from_slice(b"12345678901234");
        assert!(validate_cec_frame(&max_frame).is_ok());

        // Malformed SET_OSD_NAME: empty payload
        let empty_payload = [0x10, CEC_OPCODE_SET_OSD_NAME];
        let err = validate_cec_frame(&empty_payload).unwrap_err();
        assert!(matches!(err, CecError::Rejected(_)));

        // Malformed SET_OSD_NAME: 15 characters (buffer overflow attempt)
        let mut overflow_frame = vec![0x10, CEC_OPCODE_SET_OSD_NAME];
        overflow_frame.extend_from_slice(b"123456789012345");
        let err = validate_cec_frame(&overflow_frame).unwrap_err();
        assert!(matches!(err, CecError::Rejected(_)));
        assert!(err.to_string().contains("exceeds maximum allowed length"));

        // Malformed SET_OSD_NAME: control characters / non-ASCII
        let bad_char_frame = [0x10, CEC_OPCODE_SET_OSD_NAME, b'T', 0x1B, b'V'];
        let err = validate_cec_frame(&bad_char_frame).unwrap_err();
        assert!(matches!(err, CecError::Rejected(_)));
    }

    #[test]
    fn test_validate_cec_frame_oversized_payload() {
        // Exceeding CEC_MAX_DATA_PACKET_SIZE
        let mut huge_frame = vec![0x10, 0x00];
        huge_frame.resize(2 + CEC_MAX_DATA_PACKET_SIZE + 1, 0xAA);
        let err = validate_cec_frame(&huge_frame).unwrap_err();
        assert!(matches!(err, CecError::Rejected(_)));
    }
}
