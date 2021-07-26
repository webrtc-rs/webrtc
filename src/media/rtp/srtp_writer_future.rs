use anyhow::Result;
use srtp::stream::Stream;
use std::sync::Arc;

/// SrtpWriterFuture blocks Read/Write calls until
/// the SRTP Session is available
#[derive(Default)]
pub(crate) struct SrtpWriterFuture {
    //rtpSender: RTPSender,
    rtcp_read_stream: Option<Arc<Stream>>, // atomic.Value // *
    rtp_write_stream: Option<Arc<Stream>>, // atomic.Value // *
}
impl SrtpWriterFuture {
    /*
    func (s *srtpWriterFuture) init(returnWhenNoSRTP bool) error {
        if returnWhenNoSRTP {
            select {
            case <-s.rtpSender.stopCalled:
                return io.ErrClosedPipe
            case <-s.rtpSender.transport.srtpReady:
            default:
                return nil
            }
        } else {
            select {
            case <-s.rtpSender.stopCalled:
                return io.ErrClosedPipe
            case <-s.rtpSender.transport.srtpReady:
            }
        }

        srtcpSession, err := s.rtpSender.transport.getSRTCPSession()
        if err != nil {
            return err
        }

        rtcp_read_stream, err := srtcpSession.OpenReadStream(uint32(s.rtpSender.ssrc))
        if err != nil {
            return err
        }

        srtpSession, err := s.rtpSender.transport.getSRTPSession()
        if err != nil {
            return err
        }

        rtp_write_stream, err := srtpSession.OpenWriteStream()
        if err != nil {
            return err
        }

        s.rtcp_read_stream.Store(rtcp_read_stream)
        s.rtp_write_stream.Store(rtp_write_stream)
        return nil
    }
    */

    pub fn close(&self) -> Result<()> {
        /*TODO:
           if value := s.rtcp_read_stream.Load(); value != nil {
            return value.(*srtp.ReadStreamSRTCP).Close()
        }

        return nil*/
        Ok(())
    }

    /*
    func (s *srtpWriterFuture) Read(b []byte) (n int, err error) {
        if value := s.rtcp_read_stream.Load(); value != nil {
            return value.(*srtp.ReadStreamSRTCP).Read(b)
        }

        if err := s.init(false); err != nil || s.rtcp_read_stream.Load() == nil {
            return 0, err
        }

        return s.Read(b)
    }

    func (s *srtpWriterFuture) SetReadDeadline(t time.Time) error {
        if value := s.rtcp_read_stream.Load(); value != nil {
            return value.(*srtp.ReadStreamSRTCP).SetReadDeadline(t)
        }

        if err := s.init(false); err != nil || s.rtcp_read_stream.Load() == nil {
            return err
        }

        return s.SetReadDeadline(t)
    }

    func (s *srtpWriterFuture) WriteRTP(header *rtp.Header, payload []byte) (int, error) {
        if value := s.rtp_write_stream.Load(); value != nil {
            return value.(*srtp.WriteStreamSRTP).WriteRTP(header, payload)
        }

        if err := s.init(true); err != nil || s.rtp_write_stream.Load() == nil {
            return 0, err
        }

        return s.WriteRTP(header, payload)
    }

    func (s *srtpWriterFuture) Write(b []byte) (int, error) {
        if value := s.rtp_write_stream.Load(); value != nil {
            return value.(*srtp.WriteStreamSRTP).Write(b)
        }

        if err := s.init(true); err != nil || s.rtp_write_stream.Load() == nil {
            return 0, err
        }

        return s.Write(b)
    }
    */
}
