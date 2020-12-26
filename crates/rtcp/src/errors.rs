use util::Error;

lazy_static! {
    pub static ref ERR_INVALID_TOTAL_LOST: Error =
        Error::new("rtcp: invalid total lost count".to_owned());
    pub static ref ERR_INVALID_HEADER: Error = Error::new("rtcp: invalid header".to_owned());
    pub static ref ERR_EMPTY_COMPOUND: Error = Error::new("rtcp: empty compound packet".to_owned());
    pub static ref ERR_BAD_FIRST_PACKET: Error =
        Error::new("rtcp: first packet in compound must be SR or RR".to_owned());
    pub static ref ERR_MISSING_CNAME: Error =
        Error::new("rtcp: compound missing SourceDescription with CNAME".to_owned());
    pub static ref ERR_PACKET_BEFORE_CNAME: Error =
        Error::new("rtcp: feedback packet seen before CNAME".to_owned());
    pub static ref ERR_TOO_MANY_REPORTS: Error = Error::new("rtcp: too many reports".to_owned());
    pub static ref ERR_TOO_MANY_CHUNKS: Error = Error::new("rtcp: too many chunks".to_owned());
    pub static ref ERR_TOO_MANY_SOURCES: Error = Error::new("rtcp: too many sources".to_owned());
    pub static ref ERR_PACKET_TOO_SHORT: Error = Error::new("rtcp: packet too short".to_owned());
    pub static ref ERR_WRONG_TYPE: Error = Error::new("rtcp: wrong packet type".to_owned());
    pub static ref ERR_SDESTEXT_TOO_LONG: Error =
        Error::new("rtcp: sdes must be < 255 octets long".to_owned());
    pub static ref ERR_SDESMISSING_TYPE: Error =
        Error::new("rtcp: sdes item missing type".to_owned());
    pub static ref ERR_REASON_TOO_LONG: Error =
        Error::new("rtcp: reason must be < 255 octets long".to_owned());
    pub static ref ERR_BAD_VERSION: Error = Error::new("rtcp: invalid packet version".to_owned());
    pub static ref ERR_FAILED_TO_FILL_WHOLE_BUFFER: Error =
        Error::new("failed to fill whole buffer".to_owned());
    pub static ref ERR_BAD_UNIQUE_IDENTIFIER: Error =
        Error::new("rtcp: invalid unique identifier".to_owned());
    pub static ref ERR_BAD_MEDIA_SSRC: Error = Error::new("rtcp: invalid media SSRC".to_owned());
    pub static ref ERR_DELTA_EXCEED_LIMIT: Error = Error::new("delta exceed limit".to_owned());
    pub static ref ERR_PACKET_STATUS_CHUNK_LENGTH: Error =
        Error::new("packet status chunk must be 2 bytes".to_owned());
}
