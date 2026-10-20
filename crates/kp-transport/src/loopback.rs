//! Local development framing for the Android emulator, never a LAN listener.

use std::time::Duration;

use async_trait::async_trait;
use kryptopass_protocol::{Channel, MAX_FRAME_SIZE};
use std::pin::Pin;
use std::task::Poll;
use tokio::{
    io::{AsyncRead, AsyncWriteExt, ReadBuf},
    net::TcpStream,
    time::timeout,
};

use crate::{PeerTransport, TransportError, TransportFrame};

const LOOPBACK_HEADER_SIZE: usize = 5;
const MAX_BUFFERED_FRAME_SIZE: usize = LOOPBACK_HEADER_SIZE + MAX_FRAME_SIZE;

/// A bounded FIFO stream used only by the explicitly enabled pairing laboratory.
/// Frames are lane (0=control, 1=data), u32 big-endian length, then opaque bytes.
pub struct LoopbackTransport {
    stream: TcpStream,
    closed: bool,
    receive_buffer: Vec<u8>,
    receive_len: usize,
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
            receive_buffer: vec![0; MAX_BUFFERED_FRAME_SIZE],
            receive_len: 0,
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
            loop {
                if self.receive_len >= LOOPBACK_HEADER_SIZE {
                    let lane = self.receive_buffer[0];
                    let channel = match lane {
                        0 => Channel::Control,
                        1 => Channel::Data,
                        _ => {
                            return Err(TransportError::InvalidConfig {
                                setting: "frame_lane",
                            });
                        }
                    };
                    let length = u32::from_be_bytes([
                        self.receive_buffer[1],
                        self.receive_buffer[2],
                        self.receive_buffer[3],
                        self.receive_buffer[4],
                    ]) as usize;
                    if length == 0 || length > MAX_FRAME_SIZE {
                        return Err(TransportError::FrameTooLarge {
                            actual: length,
                            maximum: MAX_FRAME_SIZE,
                        });
                    }
                    let frame_end = LOOPBACK_HEADER_SIZE + length;
                    if self.receive_len >= frame_end {
                        let data = self.receive_buffer[LOOPBACK_HEADER_SIZE..frame_end].to_vec();
                        self.receive_buffer
                            .copy_within(frame_end..self.receive_len, 0);
                        self.receive_len -= frame_end;
                        return Ok(TransportFrame { channel, data });
                    }
                }

                if self.receive_len >= MAX_BUFFERED_FRAME_SIZE {
                    return Err(TransportError::FrameTooLarge {
                        actual: self.receive_len.saturating_sub(LOOPBACK_HEADER_SIZE),
                        maximum: MAX_FRAME_SIZE,
                    });
                }
                let filled = self.receive_len;
                let stream = &mut self.stream;
                let buffer = &mut self.receive_buffer;
                let read = std::future::poll_fn(|cx| {
                    let mut read_buffer = ReadBuf::new(&mut buffer[filled..]);
                    match Pin::new(&mut *stream).poll_read(cx, &mut read_buffer) {
                        Poll::Pending => Poll::Pending,
                        Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buffer.filled().len())),
                        Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                    }
                })
                .await
                .map_err(|_| TransportError::Closed)?;
                if read == 0 {
                    return Err(TransportError::Closed);
                }
                self.receive_len += read;
            }
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
