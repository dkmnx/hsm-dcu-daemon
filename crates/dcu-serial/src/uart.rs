//! UART transport via `tokio-serial`.

use std::os::unix::io::{AsRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_serial::{SerialPortBuilderExt, SerialStream};

use crate::error::SerialError;
use crate::transport::Transport;

/// Configuration for a UART serial port.
#[derive(Debug, Clone)]
pub struct SerialConfig {
    /// Device path, e.g. `/dev/ttyUSB0`.
    pub path: String,
    /// Baud rate (default 115200).
    pub baud_rate: u32,
    /// Data bits (default 8).
    pub data_bits: u8,
    /// Stop bits (default 1).
    pub stop_bits: u8,
    /// Hardware flow control (RTS/CTS, default false).
    pub flow_control: bool,
    /// CLOCAL flag — ignore modem carrier detect (default true).
    pub clocal: bool,
    /// Software output flow control XON/XOFF (IXON, default false).
    pub ixon: bool,
    /// Software input flow control XON/XOFF (IXOFF, default false).
    pub ixoff: bool,
    /// Any-character restart (IXANY, default false).
    pub ixany: bool,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            path: "/dev/ttyACM0".into(),
            baud_rate: 115200,
            data_bits: 8,
            stop_bits: 1,
            flow_control: false,
            clocal: true,
            ixon: false,
            ixoff: false,
            ixany: false,
        }
    }
}

/// A UART transport wrapping `tokio_serial::SerialStream`.
pub struct UartTransport {
    inner: SerialStream,
    config: SerialConfig,
}

impl UartTransport {
    /// Open the serial port with the given configuration.
    pub fn open(config: SerialConfig) -> Result<Self, SerialError> {
        let inner = tokio_serial::new(&config.path, config.baud_rate)
            .data_bits(match config.data_bits {
                5 => tokio_serial::DataBits::Five,
                6 => tokio_serial::DataBits::Six,
                7 => tokio_serial::DataBits::Seven,
                8 => tokio_serial::DataBits::Eight,
                _ => {
                    return Err(SerialError::InvalidConfig(
                        "data bits must be 5-8".to_string(),
                    ));
                }
            })
            .stop_bits(match config.stop_bits {
                1 => tokio_serial::StopBits::One,
                2 => tokio_serial::StopBits::Two,
                _ => {
                    return Err(SerialError::InvalidConfig(
                        "stop bits must be 1 or 2".to_string(),
                    ));
                }
            })
            .flow_control(match config.flow_control {
                true => tokio_serial::FlowControl::Hardware,
                false => tokio_serial::FlowControl::None,
            })
            .open_native_async()?;

        // CLOCAL and software-flow flags (IXON/IXOFF/IXANY) are parsed
        // from path options and stored in `SerialConfig` for API completeness,
        // but the safe `tokio-serial` API does not expose raw termios flags.
        // Applying them would require `unsafe` libc calls; skip for now.
        // tokio-serial applies `cfmakeraw` internally, which sets CLOCAL
        // and clears IXON/IXOFF/IXANY — matching the C defaults.
        if !config.clocal {
            tracing::warn!(
                "clocal=0 requested but not supported via tokio-serial; \
                 using CLOCAL (default)"
            );
        }
        if config.ixon || config.ixoff || config.ixany {
            tracing::warn!(
                "Software flow control (ixon={}, ixoff={}, ixany={}) \
                 requested but not supported via tokio-serial; IXON/IXOFF/IXANY \
                 remain disabled (default)",
                config.ixon, config.ixoff, config.ixany,
            );
        }

        Ok(Self { inner, config })
    }

    /// Borrow the config.
    pub fn config(&self) -> &SerialConfig {
        &self.config
    }
}

impl Transport for UartTransport {
    fn raw_fd(&self) -> Option<RawFd> {
        Some(self.inner.as_raw_fd())
    }

    fn info(&self) -> String {
        format!("UART:{}@{}", self.config.path, self.config.baud_rate)
    }
}

impl AsyncRead for UartTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for UartTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
