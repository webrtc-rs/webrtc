use super::*;
use crate::error::Result;

#[test]
fn test_tcp_type() -> Result<()> {
    //assert_eq!(TCPType::Unspecified, tcpType)
    assert_eq!(TcpType::Active, TcpType::from("active"));
    assert_eq!(TcpType::Passive, TcpType::from("passive"));
    assert_eq!(TcpType::SimultaneousOpen, TcpType::from("so"));
    assert_eq!(TcpType::Unspecified, TcpType::from("something else"));

    assert_eq!("unspecified", TcpType::Unspecified.to_string());
    assert_eq!("active", TcpType::Active.to_string());
    assert_eq!("passive", TcpType::Passive.to_string());
    assert_eq!("so", TcpType::SimultaneousOpen.to_string());

    Ok(())
}
