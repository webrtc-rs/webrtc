use utils::Error;

lazy_static! {
    pub static ref ErrInvalidTotalLost: Error =
        Error::new("rtcp: invalid total lost count".to_owned());
    pub static ref ErrInvalidHeader: Error = Error::new("rtcp: invalid header".to_owned());
    pub static ref ErrEmptyCompound: Error = Error::new("rtcp: empty compound packet".to_owned());
    pub static ref ErrBadFirstPacket: Error =
        Error::new("rtcp: first packet in compound must be SR or RR".to_owned());
    pub static ref ErrMissingCNAME: Error =
        Error::new("rtcp: compound missing SourceDescription with CNAME".to_owned());
    pub static ref ErrPacketBeforeCNAME: Error =
        Error::new("rtcp: feedback packet seen before CNAME".to_owned());
    pub static ref ErrTooManyReports: Error = Error::new("rtcp: too many reports".to_owned());
    pub static ref ErrTooManyChunks: Error = Error::new("rtcp: too many chunks".to_owned());
    pub static ref ErrTooManySources: Error = Error::new("rtcp: too many sources".to_owned());
    pub static ref ErrPacketTooShort: Error = Error::new("rtcp: packet too short".to_owned());
    pub static ref ErrWrongType: Error = Error::new("rtcp: wrong packet type".to_owned());
    pub static ref ErrSDESTextTooLong: Error =
        Error::new("rtcp: sdes must be < 255 octets long".to_owned());
    pub static ref ErrSDESMissingType: Error =
        Error::new("rtcp: sdes item missing type".to_owned());
    pub static ref ErrReasonTooLong: Error =
        Error::new("rtcp: reason must be < 255 octets long".to_owned());
    pub static ref ErrBadVersion: Error = Error::new("rtcp: invalid packet version".to_owned());
}
