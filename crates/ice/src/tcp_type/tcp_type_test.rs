use super::*;

use util::Error;

#[test]
fn test_tcp_type() -> Result<(), Error> {
    //assert_eq!(TCPType::Unspecified, tcpType)
    assert_eq!(TCPType::Active, TCPType::from("active"));
    assert_eq!(TCPType::Passive, TCPType::from("passive"));
    assert_eq!(TCPType::SimultaneousOpen, TCPType::from("so"));
    assert_eq!(TCPType::Unspecified, TCPType::from("something else"));

    assert_eq!("unspecified", TCPType::Unspecified.to_string());
    assert_eq!("active", TCPType::Active.to_string());
    assert_eq!("passive", TCPType::Passive.to_string());
    assert_eq!("so", TCPType::SimultaneousOpen.to_string());

    Ok(())
}
