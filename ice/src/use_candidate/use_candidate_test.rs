use stun::message::BINDING_REQUEST;

use super::*;
use crate::error::Result;

#[test]
fn test_use_candidate_attr_add_to() -> Result<()> {
    let mut m = Message::new();
    assert!(!UseCandidateAttr::is_set(&m), "should not be set");

    m.build(&[Box::new(BINDING_REQUEST), Box::new(UseCandidateAttr::new())])?;

    let mut m1 = Message::new();
    m1.write(&m.raw)?;

    assert!(UseCandidateAttr::is_set(&m1), "should be set");

    Ok(())
}
