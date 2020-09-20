use crate::config::Config;
use crate::context::Context;
use crate::stream::Stream;

use transport::buffer::ERR_BUFFER_FULL;
use transport::Buffer;
use util::Error;

use tokio::net::udp::{RecvHalf, SendHalf};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};

use std::collections::HashMap;
use std::io::{BufWriter, Cursor};
use std::sync::Arc;

use futures::{
    future::FutureExt, // for `.fuse()`
    pin_mut,
    select,
};

// Session implements io.ReadWriteCloser and provides a bi-directional SRTP session
// SRTP itself does not have a design like this, but it is common in most applications
// for local/remote to each have their own keying material. This provides those patterns
// instead of making everyone re-implement
pub struct Session {
    local_context: Arc<Mutex<Context>>,
    new_stream_rx: mpsc::Receiver<Stream>,
    close_session_tx: mpsc::Sender<()>,
    udp_tx: SendHalf,
    is_rtp: bool,
}

impl Session {
    pub async fn new(conn: UdpSocket, config: Config, is_rtp: bool) -> Result<Self, Error> {
        let local_context = Context::new(
            config.keys.local_master_key,
            config.keys.local_master_salt,
            config.profile,
        )?;

        let mut remote_context = Context::new(
            config.keys.remote_master_key,
            config.keys.remote_master_salt,
            config.profile,
        )?;

        let streams_map = Arc::new(Mutex::new(HashMap::new()));
        let (mut new_stream_tx, new_stream_rx) = mpsc::channel(1);
        let (close_stream_tx, mut close_stream_rx) = mpsc::channel(1);
        let (close_session_tx, mut close_session_rx) = mpsc::channel(1);
        let (mut udp_rx, udp_tx) = conn.split();

        tokio::spawn(async move {
            let mut buf: Vec<u8> = vec![0; 8192];

            let listen_udp = Session::listening(
                &mut udp_rx,
                &mut buf,
                Arc::clone(&streams_map),
                &close_stream_tx,
                &mut new_stream_tx,
                &mut remote_context,
                is_rtp,
            )
            .fuse();
            let close_stream = close_stream_rx.recv().fuse();
            let close_session = close_session_rx.recv().fuse();
            pin_mut!(listen_udp, close_stream, close_session);

            loop {
                select! {
                    result = listen_udp => match result{
                        Ok(()) => {},
                        Err(_) => break,
                    },
                    opt = close_stream => match opt {
                        Some(ssrc) => Session::close_stream(Arc::clone(&streams_map), ssrc).await,
                        None => {}
                    },
                    opt = close_session => break
                }
            }
        });

        Ok(Session {
            local_context: Arc::new(Mutex::new(local_context)),
            new_stream_rx,
            close_session_tx,
            udp_tx,
            is_rtp,
        })
    }

    async fn close_stream(streams_map: Arc<Mutex<HashMap<u32, Buffer>>>, ssrc: u32) {
        let mut streams = streams_map.lock().await;
        streams.remove(&ssrc);
    }

    async fn listening(
        udp_rx: &mut RecvHalf,
        buf: &mut [u8],
        streams_map: Arc<Mutex<HashMap<u32, Buffer>>>,
        close_stream_tx: &mpsc::Sender<u32>,
        new_stream_tx: &mut mpsc::Sender<Stream>,
        remote_context: &mut Context,
        is_rtp: bool,
    ) -> Result<(), Error> {
        let n = udp_rx.recv(buf).await?;
        if n == 0 {
            return Err(Error::new("EOF".to_string()));
        }

        let decrypted = remote_context.decrypt_rtp(&buf[0..n])?;
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

    // AcceptStream returns a stream to handle RTCP for a single SSRC
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
