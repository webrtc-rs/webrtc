// Limit the buffer size to 1MB
const SRTP_BUFFER_SIZE: usize = 1000 * 1000;

// ReadStreamSRTP handles decryption for a single RTP SSRC
struct ReadStreamSRTP {
    //mu sync.Mutex //TODO
    is_inited: bool,
    is_closed: bool, // chan bool

    //session *SessionSRTP
    ssrc: u32,
    //buffer *packetio.Buffer
}
