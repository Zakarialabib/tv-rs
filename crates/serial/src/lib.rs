//! Serial HAL, port enumeration, acquisition, and framing abstractions for RS-232/RS-422 displays.

pub mod acquisition;
pub mod codec;
pub mod enumeration;

pub use acquisition::{PortGuard, SerialPortManager};
pub use codec::{FrameCodec, FramedTransport, LineCodec, LineCodecError, RawLineCodec};
pub use enumeration::{list_ports, SerialPortInfo};
