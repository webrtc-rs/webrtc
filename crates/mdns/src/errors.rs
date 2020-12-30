use util::Error;

lazy_static! {
    pub static ref ERR_JOINING_MULTICAST_GROUP: Error =
        Error::new("mDNS: failed to join multicast group".to_owned());
    pub static ref ERR_CONNECTION_CLOSED: Error =
        Error::new("mDNS: connection is closed".to_owned());
    pub static ref ERR_CONTEXT_ELAPSED: Error = Error::new("mDNS: context has elapsed".to_owned());
    pub static ref ERR_NIL_CONFIG: Error = Error::new("mDNS: config must not be nil".to_owned());

    // ErrNotStarted indicates that the prerequisite information isn't
    // available yet because the previous records haven't been appropriately
    // parsed, skipped or finished.
    pub static ref ERR_NOT_STARTED :Error = Error::new("parsing/packing of this type isn't available yet".to_owned());

    // ErrSectionDone indicated that all records in the section have been
    // parsed or finished.
    pub static ref ERR_SECTION_DONE :Error = Error::new("parsing/packing of this section has completed".to_owned());

    pub static ref ERR_BASE_LEN            :Error = Error::new("insufficient data for base length type".to_owned());
    pub static ref ERR_CALC_LEN            :Error = Error::new("insufficient data for calculated length type".to_owned());
    pub static ref ERR_RESERVED           :Error = Error::new("segment prefix is reserved".to_owned());
    pub static ref ERR_TOO_MANY_PTR         :Error = Error::new("too many pointers (>10)".to_owned());
    pub static ref ERR_INVALID_PTR         :Error = Error::new("invalid pointer".to_owned());
    pub static ref ERR_NIL_RESOUCE_BODY     :Error = Error::new("nil resource body".to_owned());
    pub static ref ERR_RESOURCE_LEN        :Error = Error::new("insufficient data for resource body length".to_owned());
    pub static ref ERR_SEG_TOO_LONG         :Error = Error::new("segment length too long".to_owned());
    pub static ref ERR_ZERO_SEG_LEN         :Error = Error::new("zero length segment".to_owned());
    pub static ref ERR_RES_TOO_LONG         :Error = Error::new("resource length too long".to_owned());
    pub static ref ERR_TOO_MANY_QUESTIONS   :Error = Error::new("too many Questions to pack (>65535)".to_owned());
    pub static ref ERR_TOO_MANY_ANSWERS     :Error = Error::new("too many Answers to pack (>65535)".to_owned());
    pub static ref ERR_TOO_MANY_AUTHORITIES :Error = Error::new("too many Authorities to pack (>65535)".to_owned());
    pub static ref ERR_TOO_MANY_ADDITIONALS :Error = Error::new("too many Additionals to pack (>65535)".to_owned());
    pub static ref ERR_NON_CANONICAL_NAME   :Error = Error::new("name is not in canonical format (it must end with a .)".to_owned());
    pub static ref ERR_STRING_TOO_LONG      :Error = Error::new("character string exceeds maximum length (255)".to_owned());
    pub static ref ERR_COMPRESSED_SRV      :Error = Error::new("compressed name in SRV resource data".to_owned());
}
