mod handler;
mod utils;

use crate::allocation::allocation_manager::*;
use crate::errors::*;
use crate::proto::chandata::ChannelData;
use handler::*;

use stun::message::*;
use util::{Conn, Error};

use std::collections::HashMap;
use std::marker::{Send, Sync};
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

type AuthHandlerFn = fn(username: String, realm: String, srcAddr: SocketAddr) -> (Vec<u8>, bool);

// Request contains all the state needed to process a single incoming datagram
pub struct Request {
    // Current Request State
    conn: Arc<dyn Conn + Send + Sync>,
    src_addr: SocketAddr,
    buff: Vec<u8>,

    // Server State
    allocation_manager: Manager,
    nonces: Arc<Mutex<HashMap<String, Instant>>>,

    // User Configuration
    auth_handler: AuthHandlerFn,
    realm: String,
    channel_bind_timeout: Duration,
}

// handle_request processes the give Request
pub async fn handle_request(r: Request) -> Result<(), Error> {
    log::debug!(
        "received {} bytes of udp from {} on {}",
        r.buff.len(),
        r.src_addr,
        r.conn.local_addr()?
    );

    if ChannelData::is_channel_data(&r.buff) {
        handle_data_packet(r).await
    } else {
        handle_turn_packet(r).await
    }
}

async fn handle_data_packet(r: Request) -> Result<(), Error> {
    log::debug!("received DataPacket from {}", r.src_addr);
    let mut c = ChannelData {
        raw: r.buff.clone(),
        ..Default::default()
    };
    c.decode()?;
    handle_channel_data(r, &c).await
}

async fn handle_turn_packet(r: Request) -> Result<(), Error> {
    log::debug!("handle_turn_packet");
    let mut m = Message {
        raw: r.buff.clone(),
        ..Default::default()
    };
    m.decode()?;

    process_message_handler(r, &m).await
}

async fn process_message_handler(r: Request, m: &Message) -> Result<(), Error> {
    if m.typ.class == CLASS_INDICATION {
        match m.typ.method {
            METHOD_SEND => handle_send_indication(r, m).await,
            _ => Err(ERR_UNEXPECTED_CLASS.to_owned()),
        }
    } else if m.typ.class == CLASS_REQUEST {
        match m.typ.method {
            METHOD_ALLOCATE => handle_allocate_request(r, m).await,
            METHOD_REFRESH => handle_refresh_request(r, m).await,
            METHOD_CREATE_PERMISSION => handle_create_permission_request(r, m).await,
            METHOD_CHANNEL_BIND => handle_channel_bind_request(r, m).await,
            METHOD_BINDING => handle_binding_request(r, m).await,
            _ => Err(ERR_UNEXPECTED_CLASS.to_owned()),
        }
    } else {
        Err(ERR_UNEXPECTED_CLASS.to_owned())
    }
}
