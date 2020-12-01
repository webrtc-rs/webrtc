mod session_rtcp_test;
mod session_rtp_test;

use crate::{config::Config, context::Context, option, stream::Stream};

use util::Error;
use util::{buffer::ERR_BUFFER_FULL, Buffer};

use tokio::{
    net::UdpSocket,
    sync::{mpsc, Mutex},
};

use std::{
    collections::HashMap,
    io::{BufWriter, Cursor},
    sync::Arc,
};

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
    pub(crate) udp_tx: Arc<UdpSocket>,
    is_rtp: bool,
}

impl Session {
    pub async fn new(conn: UdpSocket, config: Config, is_rtp: bool) -> Result<Self, Error> {
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
                Some(option::srtp_replay_protection(
                    DEFAULT_SESSION_SRTP_REPLAY_PROTECTION_WINDOW,
                ))
            } else {
                config.remote_rtp_options
            },
            if config.remote_rtcp_options.is_none() {
                Some(option::srtcp_replay_protection(
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
        let udp_tx = Arc::new(conn);
        let udp_rx = Arc::clone(&udp_tx);
        let cloned_streams_map = Arc::clone(&streams_map);
        let cloned_close_stream_tx = close_stream_tx.clone();

        tokio::spawn(async move {
            let mut buf: Vec<u8> = vec![0; 8192];

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
        udp_rx: &Arc<UdpSocket>,
        buf: &mut [u8],
        streams_map: &Arc<Mutex<HashMap<u32, Buffer>>>,
        close_stream_tx: &mpsc::Sender<u32>,
        new_stream_tx: &mut mpsc::Sender<Stream>,
        remote_context: &mut Context,
        is_rtp: bool,
    ) -> Result<(), Error> {
        let n = udp_rx.recv(buf).await?;
        if n == 0 {
            return Err(Error::new("EOF".to_string()));
        }

        let decrypted = if is_rtp {
            remote_context.decrypt_rtp(&buf[0..n])?
        } else {
            remote_context.decrypt_rtcp(&buf[0..n])?
        };

        let ssrcs = if is_rtp {
            let mut reader = Cursor::new(&decrypted);
            vec![rtp::header::Header::unmarshal(&mut reader)?.ssrc]
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
                        return Err(err);
                    }
                }
            }
        }

        Ok(())
    }

    // listen on the given SSRC to create a stream, it can be used
    // if you want a certain SSRC, but don't want to wait for Accept
    pub async fn listen(&mut self, ssrc: u32) -> Result<Stream, Error> {
        let mut streams = self.streams_map.lock().await;

        if streams.contains_key(&ssrc) {
            Err(Error::new(format!("Stream with ssrc {} exists", ssrc)))
        } else {
            let stream = Stream::new(ssrc, self.close_stream_tx.clone(), self.is_rtp);
            streams.insert(ssrc, stream.get_cloned_buffer());

            Ok(stream)
        }
    }

    // accept returns a stream to handle RTCP for a single SSRC
    pub async fn accept(&mut self) -> Result<Stream, Error> {
        let result = self.new_stream_rx.recv().await;
        if let Some(stream) = result {
            Ok(stream)
        } else {
            Err(Error::new("SessionSRTP has been closed".to_string()))
        }
    }

    pub async fn close(&mut self) -> Result<(), Error> {
        self.close_session_tx.send(()).await?;

        Ok(())
    }

    pub async fn write(&mut self, buf: &[u8], is_rtp: bool) -> Result<usize, Error> {
        if self.is_rtp != is_rtp {
            return Err(Error::new(
                "Session RTP/RTCP type must be same as input buffer".to_string(),
            ));
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
        let mut raw: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
            packet.marshal(&mut writer)?;
        }
        self.write(&raw, true).await
    }

    pub async fn write_rtcp(&mut self, packet: &rtcp::packet::Packet) -> Result<usize, Error> {
        let mut raw: Vec<u8> = vec![];
        {
            let mut writer = BufWriter::<&mut Vec<u8>>::new(raw.as_mut());
            packet.marshal(&mut writer)?;
        }
        self.write(&raw, false).await
    }
}
