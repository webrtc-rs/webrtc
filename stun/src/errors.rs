use util::Error;

lazy_static! {
    // ErrAttributeNotFound means that attribute with provided attribute
    // type does not exist in message.
     pub static ref ERR_ATTRIBUTE_NOT_FOUND: Error = Error::new("attribute not found".to_owned());
}
