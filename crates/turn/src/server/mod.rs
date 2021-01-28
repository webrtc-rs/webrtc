mod handler;
mod utils;

use crate::allocation::allocation_manager::*;
use crate::proto::chandata::ChannelData;

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
pub fn handle_request(r: Request) -> Result<(), Error> {
    log::debug!(
        "received {} bytes of udp from {} on {}",
        r.buff.len(),
        r.src_addr,
        r.conn.local_addr()?
    );

    if ChannelData::is_channel_data(&r.buff) {
        handle_data_packet(r)
    } else {
        handle_turn_packet(r)
    }
}

fn handle_data_packet(r: Request) -> Result<(), Error> {
    log::debug!("received DataPacket from {}", r.src_addr);
    let mut c = ChannelData {
        raw: r.buff, //TODO.clone(),
        ..Default::default()
    };
    c.decode()?;
    //TODO: handleChannelData(r, &c)?;

    Ok(())
}

fn handle_turn_packet(r: Request) -> Result<(), Error> {
    log::debug!("handle_turnpacket");
    let mut m = Message {
        raw: r.buff, //TODO.clone(),
        ..Default::default()
    };
    m.decode()?;

    /*h, err := getMessageHandler(m.Type.Class, m.Type.Method)
    if err != nil {
        return fmt.Errorf("%w %v-%v from %v: %v", errUnhandledSTUNPacket, m.Type.Method, m.Type.Class, r.src_addr, err)
    }

    err = h(r, m)
    if err != nil {
        return fmt.Errorf("%w %v-%v from %v: %v", errFailedToHandle, m.Type.Method, m.Type.Class, r.src_addr, err)
    }*/

    Ok(())
}

/*
func getMessageHandler(class stun.MessageClass, method stun.Method) (func(r Request, m *stun.Message) error, error) {
    switch class {
    case stun.ClassIndication:
        switch method {
        case stun.MethodSend:
            return handleSendIndication, nil
        default:
            return nil, fmt.Errorf("%w: %s", errUnexpectedMethod, method)
        }

    case stun.ClassRequest:
        switch method {
        case stun.MethodAllocate:
            return handle_allocate_request, nil
        case stun.MethodRefresh:
            return handleRefreshRequest, nil
        case stun.MethodCreatePermission:
            return handleCreatePermissionRequest, nil
        case stun.MethodChannelBind:
            return handleChannelBindRequest, nil
        case stun.MethodBinding:
            return handle_binding_request, nil
        default:
            return nil, fmt.Errorf("%w: %s", errUnexpectedMethod, method)
        }

    default:
        return nil, fmt.Errorf("%w: %s", errUnexpectedClass, class)
    }
}
*/
