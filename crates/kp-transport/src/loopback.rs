//! Local development framing for the Android emulator, never a LAN listener.

use std::time::Duration;

use async_trait::async_trait;
use kryptopass_protocol::{Channel, MAX_FRAME_SIZE};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};

use crate::{PeerTransport, TransportError, TransportFrame};

/// A bounded FIFO stream used only by the explicitly enabled pairing laboratory.
/// Frames are lane (0=control, 1=data), u32 big-endian length, then opaque bytes.
pub struct LoopbackTransport {
    stream: TcpStream,
    closed: bool,
}

impl LoopbackTransport {
    /// Refuses sockets with non-loopback local or remote addresses.
    pub fn new(stream: TcpStream) -> Result<Self, TransportError> {
        let local = stream.local_addr().map_err(|_| TransportError::Closed)?;
        let remote = stream.peer_addr().map_err(|_| TransportError::Closed)?;
        if !local.ip().is_loopback() || !remote.ip().is_loopback() {
            return Err(TransportError::InvalidConfig {
                setting: "loopback_socket",
            });
        }
        stream
            .set_nodelay(true)
            .map_err(|_| TransportError::Closed)?;
        Ok(Self {
            stream,
            closed: false,
        })
    }
}

#[async_trait]
impl PeerTransport for LoopbackTransport {
    async fn send(&mut self, channel: Channel, data: &[u8]) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::Closed);
        }
        if data.is_empty() || data.len() > MAX_FRAME_SIZE {
            return Err(TransportError::FrameTooLarge {
                actual: data.len(),
                maximum: MAX_FRAME_SIZE,
            });
        }
        let lane = match channel {
            Channel::Control => 0,
            Channel::Data => 1,
        };
        let length = u32::try_from(data.len()).map_err(|_| TransportError::Closed)?;
        let write = async {
            self.stream.write_u8(lane).await?;
            self.stream.write_all(&length.to_be_bytes()).await?;
            self.stream.write_all(data).await?;
            self.stream.flush().await
        };
        match timeout(Duration::from_secs(120), write).await {
            Ok(Ok(())) => Ok(()),
            result => {
                self.closed = true;
                if result.is_err() {
                    Err(TransportError::SendTimedOut)
                } else {
                    Err(TransportError::Closed)
                }
            }
        }
    }

    async fn receive(&mut self) -> Result<TransportFrame, TransportError> {
        if self.closed {
            return Err(TransportError::Closed);
        }
        let read = async {
            let mut header = [0; 5];
            self.stream
                .read_exact(&mut header)
                .await
                .map_err(|_| TransportError::Closed)?;
            let channel = match header[0] {
                0 => Channel::Control,
                1 => Channel::Data,
                _ => {
                    return Err(TransportError::InvalidConfig {
                        setting: "frame_lane",
                    });
                }
            };
            let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
            if length == 0 || length > MAX_FRAME_SIZE {
                return Err(TransportError::FrameTooLarge {
                    actual: length,
                    maximum: MAX_FRAME_SIZE,
                });
            }
            let mut data = vec![0; length];
            self.stream
                .read_exact(&mut data)
                .await
                .map_err(|_| TransportError::Closed)?;
            Ok(TransportFrame { channel, data })
        };
        let result = timeout(Duration::from_secs(120), read)
            .await
            .unwrap_or(Err(TransportError::ReceiveTimedOut));
        if result.is_err() {
            self.closed = true;
        }
        result
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.closed = true;
        let _ = self.stream.shutdown().await;
        Ok(())
    }
}
