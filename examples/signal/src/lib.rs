#![warn(rust_2018_idioms)]
#![allow(dead_code)]

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use tokio::sync::{mpsc, Mutex};

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref SDP_CHAN_TX_MUTEX: Arc<Mutex<Option<mpsc::Sender<String>>>> =
        Arc::new(Mutex::new(None));
}

// HTTP Listener to get sdp
async fn remote_handler(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        // A HTTP handler that processes a SessionDescription given to us from the other WebRTC-rs or Pion process
        (&Method::POST, "/sdp") => {
            //println!("remote_handler receive from /sdp");
            let sdp_str = match std::str::from_utf8(&hyper::body::to_bytes(req.into_body()).await?)
            {
                Ok(s) => s.to_owned(),
                Err(err) => panic!("{}", err),
            };

            {
                let sdp_chan_tx = SDP_CHAN_TX_MUTEX.lock().await;
                if let Some(tx) = &*sdp_chan_tx {
                    let _ = tx.send(sdp_str).await;
                }
            }

            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::OK;
            Ok(response)
        }
        // Return the 404 Not Found for other routes.
        _ => {
            let mut not_found = Response::default();
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

/// http_sdp_server starts a HTTP Server that consumes SDPs
pub async fn http_sdp_server(port: u16) -> mpsc::Receiver<String> {
    let (sdp_chan_tx, sdp_chan_rx) = mpsc::channel::<String>(1);
    {
        let mut tx = SDP_CHAN_TX_MUTEX.lock().await;
        *tx = Some(sdp_chan_tx);
    }

    tokio::spawn(async move {
        let addr = SocketAddr::from_str(&format!("0.0.0.0:{port}")).unwrap();
        let service =
            make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(remote_handler)) });
        let server = Server::bind(&addr).serve(service);
        // Run this server for... forever!
        if let Err(e) = server.await {
            eprintln!("server error: {e}");
        }
    });

    sdp_chan_rx
}

/// must_read_stdin blocks until input is received from stdin
#[allow(clippy::assigning_clones)]
pub fn must_read_stdin() -> Result<String> {
    let mut line = String::new();

    std::io::stdin().read_line(&mut line)?;
    line = line.trim().to_owned();
    println!();

    Ok(line)
}

// Allows compressing offer/answer to bypass terminal input limits.
// const COMPRESS: bool = false;

/// encode encodes the input in base64
/// It can optionally zip the input before encoding
pub fn encode(b: &str) -> String {
    //if COMPRESS {
    //    b = zip(b)
    //}

    BASE64_STANDARD.encode(b)
}

/// decode decodes the input from base64
/// It can optionally unzip the input after decoding
pub fn decode(s: &str) -> Result<String> {
    let b = BASE64_STANDARD.decode(s)?;

    //if COMPRESS {
    //    b = unzip(b)
    //}

    let s = String::from_utf8(b)?;
    Ok(s)
}
/*
func zip(in []byte) []byte {
    var b bytes.Buffer
    gz := gzip.NewWriter(&b)
    _, err := gz.Write(in)
    if err != nil {
        panic(err)
    }
    err = gz.Flush()
    if err != nil {
        panic(err)
    }
    err = gz.Close()
    if err != nil {
        panic(err)
    }
    return b.Bytes()
}

func unzip(in []byte) []byte {
    var b bytes.Buffer
    _, err := b.Write(in)
    if err != nil {
        panic(err)
    }
    r, err := gzip.NewReader(&b)
    if err != nil {
        panic(err)
    }
    res, err := ioutil.ReadAll(r)
    if err != nil {
        panic(err)
    }
    return res
}
*/
