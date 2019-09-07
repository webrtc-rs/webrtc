use util::Error;

use crate::context::*;
use crate::stream::*;

use std::collections::HashMap;
use std::sync::Mutex;

pub mod session_srtcp;
pub mod session_srtp;

use session_srtcp::SessionSRTCP;
use session_srtp::SessionSRTP;

pub enum Session {
    SessionSRTP(SessionSRTP),
    SessionSRTCP(SessionSRTCP),
}

struct SessionBase {
    //localContextMutex           sync.Mutex
    local_context: Context,
    remote_context: Context,

    //newStream chan readStream

    //started chan interface{}
    //closed  chan interface{}
    read_streams_closed: bool,

    read_streams: Mutex<HashMap<u32, Box<dyn ReadStream>>>,
    //log logging.LeveledLogger

    //nextConn net.Conn
}

impl SessionBase {
    fn get_or_create_read_stream(
        &mut self,
        ssrc: u32,
        child: Session,
        proto: fn() -> Box<dyn ReadStream>,
    ) -> Option<&Box<dyn ReadStream>> {
        let read_streams = self.read_streams.get_mut().unwrap();

        if self.read_streams_closed {
            return None;
        }

        if !read_streams.contains_key(&ssrc) {
            let r = proto();
            if r.init(child, ssrc).is_err() {
                return None;
            }
            read_streams.insert(ssrc, r);
        }

        read_streams.get(&ssrc)
    }

    fn remove_read_stream(&mut self, ssrc: u32) {
        let read_streams = self.read_streams.get_mut().unwrap();

        if self.read_streams_closed {
            return;
        }

        read_streams.remove(&ssrc);
    }

    fn close(&mut self) -> Result<(), Error> {
        /*if s.nextConn == nil {
            return nil
        } else if err := s.nextConn.Close(); err != nil {
            return err
        }

        <-s.closed
        return nil*/
        Ok(())
    }

    fn start(
        &mut self,
        localMasterKey: Vec<u8>,
        localMasterSalt: Vec<u8>,
        remoteMasterKey: Vec<u8>,
        remoteMasterSalt: Vec<u8>,
        profile: ProtectionProfile,
        child: Session,
    ) -> Result<(), Error> {
        /*var err error
        s.localContext, err = CreateContext(localMasterKey, localMasterSalt, profile)
        if err != nil {
            return err
        }

        s.remoteContext, err = CreateContext(remoteMasterKey, remoteMasterSalt, profile)
        if err != nil {
            return err
        }

        go func() {
            defer func() {
                close(s.newStream)

                s.readStreamsLock.Lock()
                s.readStreamsClosed = true
                s.readStreamsLock.Unlock()
                close(s.closed)
            }()

            b := make([]byte, 8192)
            for {
                var i int
                i, err = s.nextConn.Read(b)
                if err != nil {
                    if err != io.EOF {
                        s.log.Errorf("srtp: %s", err.Error())
                    }
                    return
                }

                if err = child.decrypt(b[:i]); err != nil {
                    s.log.Infof("%v \n", err)
                }
            }
        }()

        close(s.started)

        return nil*/
        Ok(())
    }
}
