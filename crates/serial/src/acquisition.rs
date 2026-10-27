//! Serial port acquisition and concurrency locking.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};
use tokio_serial::SerialPortBuilderExt;
use tv_rs_core::{TransportError, TvError};

static GLOBAL_MANAGER: OnceLock<SerialPortManager> = OnceLock::new();

/// Manages active serial port locks to prevent simultaneous access to the same hardware port.
#[derive(Debug, Clone)]
pub struct SerialPortManager {
    active_ports: Arc<Mutex<HashSet<String>>>,
}

impl Default for SerialPortManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SerialPortManager {
    /// Creates a new isolated [`SerialPortManager`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            active_ports: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Returns a reference to the process-global [`SerialPortManager`].
    pub fn global() -> &'static Self {
        GLOBAL_MANAGER.get_or_init(Self::new)
    }

    /// Normalizes a port path or name for consistent lock lookup across platforms.
    fn normalize_port_name(port_name: &str) -> String {
        let trimmed = port_name.trim();
        #[cfg(windows)]
        {
            trimmed.to_uppercase()
        }
        #[cfg(not(windows))]
        {
            trimmed.to_string()
        }
    }

    /// Attempts to acquire exclusive ownership of the specified port path.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::Transport`] with [`TransportError::PortBusy`] if the port is already acquired.
    pub fn acquire(&self, port_name: &str) -> Result<PortGuard, TvError> {
        let normalized = Self::normalize_port_name(port_name);
        let mut set = self
            .active_ports
            .lock()
            .map_err(|_| TvError::Transport(TransportError::PortBusy))?;

        if set.contains(&normalized) {
            return Err(TvError::Transport(TransportError::PortBusy));
        }

        set.insert(normalized.clone());

        Ok(PortGuard {
            manager: self.clone(),
            port_name: normalized,
        })
    }

    /// Acquires a port lock and opens an async [`tokio_serial::SerialStream`] for `path` at `baud_rate`.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::Transport`] with [`TransportError::PortBusy`] if the port is locked or OS open returns a resource busy/permission error.
    pub fn acquire_stream(
        &self,
        path: &str,
        baud_rate: u32,
    ) -> Result<(tokio_serial::SerialStream, PortGuard), TvError> {
        let builder = tokio_serial::new(path, baud_rate);
        self.acquire_stream_with(path, builder)
    }

    /// Acquires a port lock and opens an async [`tokio_serial::SerialStream`] using custom builder configuration.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::Transport`] with [`TransportError::PortBusy`] if the port is locked or OS open returns a resource busy/permission error.
    pub fn acquire_stream_with(
        &self,
        path: &str,
        builder: tokio_serial::SerialPortBuilder,
    ) -> Result<(tokio_serial::SerialStream, PortGuard), TvError> {
        let guard = self.acquire(path)?;

        let stream = builder.open_native_async().map_err(|err| {
            let io_err: std::io::Error = err.into();
            match io_err.kind() {
                std::io::ErrorKind::PermissionDenied
                | std::io::ErrorKind::AddrInUse
                | std::io::ErrorKind::ResourceBusy
                | std::io::ErrorKind::AlreadyExists => TvError::Transport(TransportError::PortBusy),
                _ => TvError::Transport(TransportError::from_io(&io_err)),
            }
        })?;

        Ok((stream, guard))
    }
}

/// RAII guard representing exclusive access to an acquired serial port.
///
/// Releasing this guard (via [`Drop`]) unlocks the port for future acquisition attempts.
#[derive(Debug)]
pub struct PortGuard {
    manager: SerialPortManager,
    port_name: String,
}

impl PortGuard {
    /// Returns the normalized port name associated with this guard.
    #[must_use]
    pub fn port_name(&self) -> &str {
        &self.port_name
    }
}

impl Drop for PortGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = self.manager.active_ports.lock() {
            set.remove(&self.port_name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_port_acquisition_and_release() {
        let manager = SerialPortManager::new();
        let port = "COM1";

        let guard1 = manager
            .acquire(port)
            .expect("first acquisition should succeed");
        assert_eq!(guard1.port_name(), "COM1");

        let err = manager.acquire(port).unwrap_err();
        assert!(matches!(err, TvError::Transport(TransportError::PortBusy)));

        drop(guard1);

        let guard2 = manager
            .acquire(port)
            .expect("acquisition should succeed after drop");
        assert_eq!(guard2.port_name(), "COM1");
    }
}
