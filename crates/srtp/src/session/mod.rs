#[cfg(test)]
mod session_rtcp_test;
#[cfg(test)]
mod session_rtp_test;

use crate::{config::*, context::*, error::Error, option::*, stream::*};

use rtp::packetizer::Marshaller;
use util::{buffer::*, conn::Conn};

use bytes::{Bytes, BytesMut};
use std::{
    collections::HashMap,
    marker::{Send, Sync},
    sync::Arc,
};
use tokio::sync::{mpsc, Mutex};

const DEFAULT_SESSION_SRTP_REPLAY_PROTECTION_WINDOW: usize = 64;
const DEFAULT_SESSION_SRTCP_REPLAY_PROTECTION_WINDOW: usize = 64;

/// Session implements io.ReadWriteCloser and provides a bi-directional SRTP session
/// SRTP itself does not have a design like this, but it is common in most applications
/// for local/remote to each have their own keying material. This provides those patterns
/// instead of making everyone re-implement
pub struct Session {
    local_context: Arc<Mutex<Context>>,
    streams_map: Arc<Mutex<HashMap<u32, Buffer>>>,
    new_stream_rx: mpsc::Receiver<Stream>,
    close_stream_tx: mpsc::Sender<u32>,
    close_session_tx: mpsc::Sender<()>,
    pub(crate) udp_tx: Arc<dyn Conn + Send + Sync>,
    is_rtp: bool,
}

impl Session {
    pub async fn new(
        conn: Arc<dyn Conn + Send + Sync>,
        config: Config,
        is_rtp: bool,
    ) -> Result<Self, Error> {
        let local_context = Context::new(
            &config.keys.local_master_key,
            &config.keys.local_master_salt,
            config.profile,
            config.local_rtp_options,
            config.local_rtcp_options,
        )?;

        let mut remote_context = Context::new(
            &config.keys.remote_master_key,
            &config.keys.remote_master_salt,
            config.profile,
            if config.remote_rtp_options.is_none() {
                Some(srtp_replay_protection(
                    DEFAULT_SESSION_SRTP_REPLAY_PROTECTION_WINDOW,
                ))
            } else {
                config.remote_rtp_options
            },
            if config.remote_rtcp_options.is_none() {
                Some(srtcp_replay_protection(
                    DEFAULT_SESSION_SRTCP_REPLAY_PROTECTION_WINDOW,
                ))
            } else {
                config.remote_rtcp_options
            },
        )?;

        let streams_map = Arc::new(Mutex::new(HashMap::new()));
        let (mut new_stream_tx, new_stream_rx) = mpsc::channel(1);
        let (close_stream_tx, mut close_stream_rx) = mpsc::channel(1);
        let (close_session_tx, mut close_session_rx) = mpsc::channel(1);
        let udp_tx = Arc::clone(&conn);
        let udp_rx = Arc::clone(&conn);
        let cloned_streams_map = Arc::clone(&streams_map);
        let cloned_close_stream_tx = close_stream_tx.clone();

        tokio::spawn(async move {
            let mut buf = BytesMut::with_capacity(8192);
            buf.resize(8192, 0u8);

            loop {
                let incoming_stream = Session::incoming(
                    &udp_rx,
                    &mut buf,
                    &cloned_streams_map,
                    &cloned_close_stream_tx,
                    &mut new_stream_tx,
                    &mut remote_context,
                    is_rtp,
                );
                let close_stream = close_stream_rx.recv();
                let close_session = close_session_rx.recv();

                tokio::select! {
                    result = incoming_stream => match result{
                        Ok(()) => {},
                        Err(err) => log::info!("{}", err),
                    },
                    opt = close_stream => if let Some(ssrc) = opt {
                        Session::close_stream(&cloned_streams_map, ssrc).await
                    },
                    _ = close_session => break
                }
            }
        });

        Ok(Session {
            local_context: Arc::new(Mutex::new(local_context)),
            streams_map,
            new_stream_rx,
            close_stream_tx,
            close_session_tx,
            udp_tx,
            is_rtp,
        })
    }

    async fn close_stream(streams_map: &Arc<Mutex<HashMap<u32, Buffer>>>, ssrc: u32) {
        let mut streams = streams_map.lock().await;
        streams.remove(&ssrc);
    }

    async fn incoming(
        udp_rx: &Arc<dyn Conn + Send + Sync>,
        buf: &mut BytesMut,
        streams_map: &Arc<Mutex<HashMap<u32, Buffer>>>,
        close_stream_tx: &mpsc::Sender<u32>,
        new_stream_tx: &mut mpsc::Sender<Stream>,
        remote_context: &mut Context,
        is_rtp: bool,
    ) -> Result<(), Error> {
        let n = udp_rx.recv(buf).await?;
        if n == 0 {
            return Err(Error::SessionEof);
        }

        let encrypted = Bytes::from(buf[0..n].to_vec()); //TODO: how to avoid this memory allocation
        let decrypted = if is_rtp {
            remote_context.decrypt_rtp(&encrypted)?
        } else {
            remote_context.decrypt_rtcp(&encrypted)?
        };

        let ssrcs = if is_rtp {
            vec![rtp::header::Header::unmarshal(&decrypted)?.ssrc]
        } else {
            rtcp::packet::unmarshal(&decrypted)?.destination_ssrc()
        };

        let mut streams = streams_map.lock().await;

        for ssrc in ssrcs {
            if !streams.contains_key(&ssrc) {
                let stream = Stream::new(ssrc, close_stream_tx.clone(), is_rtp);
                streams.insert(ssrc, stream.get_cloned_buffer());
                new_stream_tx.send(stream).await?;
            }
            match streams.get_mut(&ssrc).unwrap().write(&decrypted).await {
                Ok(_) => {}
                Err(err) => {
                    // Silently drop data when the buffer is full.
                    if err != ERR_BUFFER_FULL.clone() {
                        return Err(Error::UtilError(err));
                    }
                }
            }
        }

        Ok(())
    }

    /// listen on the given SSRC to create a stream, it can be used
    /// if you want a certain SSRC, but don't want to wait for Accept
    pub async fn listen(&mut self, ssrc: u32) -> Result<Stream, Error> {
        let mut streams = self.streams_map.lock().await;

        if streams.contains_key(&ssrc) {
            Err(Error::StreamWithSsrcExists(ssrc))
        } else {
            let stream = Stream::new(ssrc, self.close_stream_tx.clone(), self.is_rtp);
            streams.insert(ssrc, stream.get_cloned_buffer());

            Ok(stream)
        }
    }

    /// accept returns a stream to handle RTCP for a single SSRC
    pub async fn accept(&mut self) -> Result<Stream, Error> {
        let result = self.new_stream_rx.recv().await;
        if let Some(stream) = result {
            Ok(stream)
        } else {
            Err(Error::SessionSrtpAlreadyClosed)
        }
    }

    pub async fn close(&mut self) -> Result<(), Error> {
        self.close_session_tx.send(()).await?;

        Ok(())
    }

    pub async fn write(&mut self, buf: &Bytes, is_rtp: bool) -> Result<usize, Error> {
        if self.is_rtp != is_rtp {
            return Err(Error::SessionRtpRtcpTypeMismatch);
        }

        let mut local_context = self.local_context.lock().await;

        let encrypted = if is_rtp {
            local_context.encrypt_rtp(buf)?
        } else {
            local_context.encrypt_rtcp(buf)?
        };

        match self.udp_tx.send(&encrypted).await {
            Ok(n) => Ok(n),
            Err(err) => Err(err.into()),
        }
    }

    pub async fn write_rtp(&mut self, packet: &rtp::packet::Packet) -> Result<usize, Error> {
        let raw = packet.marshal()?;
        self.write(&raw, true).await
    }

    pub async fn write_rtcp(&mut self, packet: &dyn rtcp::packet::Packet) -> Result<usize, Error> {
        let raw = packet.marshal()?;
        self.write(&raw, false).await
    }
}
