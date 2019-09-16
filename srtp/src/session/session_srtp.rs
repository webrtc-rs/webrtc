use crate::config::Config;
use crate::context::Context;
use crate::stream::stream_srtp::StreamSRTP;

use rtp::packet::Header;
use util::{Buffer, Error};

use tokio::net::udp::split::{UdpSocketRecvHalf, UdpSocketSendHalf};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Lock};

use std::collections::HashMap;
use std::io::Cursor;

use futures::{
    future::FutureExt, // for `.fuse()`
    pin_mut,
    select,
};

// SessionSRTP implements io.ReadWriteCloser and provides a bi-directional SRTP session
// SRTP itself does not have a design like this, but it is common in most applications
// for local/remote to each have their own keying material. This provides those patterns
// instead of making everyone re-implement
pub struct SessionSRTP {
    local_context: Context,
    new_stream_rx: mpsc::Receiver<StreamSRTP>,
    close_session_tx: mpsc::Sender<()>,
    udp_tx: UdpSocketSendHalf,
}

impl SessionSRTP {
    pub async fn new(conn: UdpSocket, config: Config) -> Result<Self, Error> {
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

        let streams_map = Lock::new(HashMap::new());
        let (mut new_stream_tx, new_stream_rx) = mpsc::channel(1);
        let (close_stream_tx, mut close_stream_rx) = mpsc::channel(1);
        let (close_session_tx, mut close_session_rx) = mpsc::channel(1);
        let (mut udp_rx, udp_tx) = conn.split();

        tokio::spawn(async move {
            let mut buf: Vec<u8> = vec![0; 8192];

            let listen_udp = SessionSRTP::listening(
                &mut udp_rx,
                &mut buf,
                streams_map.clone(),
                &close_stream_tx,
                &mut new_stream_tx,
                &mut remote_context,
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
                        Some(ssrc) => SessionSRTP::close_stream(streams_map.clone(), ssrc).await,
                        None => {}
                    },
                    opt = close_session => break
                }
            }
        });

        Ok(SessionSRTP {
            local_context,
            new_stream_rx,
            close_session_tx,
            udp_tx,
        })
    }

    async fn close_stream(mut streams_map: Lock<HashMap<u32, Buffer>>, ssrc: u32) {
        let mut streams = streams_map.lock().await;
        streams.remove(&ssrc);
    }

    async fn listening(
        udp_rx: &mut UdpSocketRecvHalf,
        buf: &mut [u8],
        mut streams_map: Lock<HashMap<u32, Buffer>>,
        close_stream_tx: &mpsc::Sender<u32>,
        new_stream_tx: &mut mpsc::Sender<StreamSRTP>,
        remote_context: &mut Context,
    ) -> Result<(), Error> {
        let n = udp_rx.recv(buf).await?;
        if n == 0 {
            return Err(Error::new("EOF".to_string()));
        }

        let mut reader = Cursor::new(buf[0..n].as_ref());
        let ssrc = Header::unmarshal(&mut reader)?.ssrc;

        let decrypted = remote_context.decrypt_rtp(&buf[0..n])?;

        let mut streams = streams_map.lock().await;
        if !streams.contains_key(&ssrc) {
            let stream = StreamSRTP::new(ssrc, close_stream_tx.clone());
            streams.insert(ssrc, stream.get_cloned_buffer());
            new_stream_tx.send(stream).await?;
        }
        streams.get_mut(&ssrc).unwrap().write(&decrypted).await?;

        Ok(())
    }

    // AcceptStream returns a stream to handle RTCP for a single SSRC
    pub async fn accept(&mut self) -> Result<StreamSRTP, Error> {
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
}
