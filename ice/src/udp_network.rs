use std::sync::Arc;

use super::udp_mux::UDPMux;
use super::Error;

#[derive(Default, Clone)]
pub struct EphemeralUDP {
    port_min: u16,
    port_max: u16,
}

impl EphemeralUDP {
    pub fn new(port_min: u16, port_max: u16) -> Result<Self, Error> {
        let mut s = Self::default();
        s.set_ports(port_min, port_max)?;

        Ok(s)
    }

    pub fn port_min(&self) -> u16 {
        self.port_min
    }

    pub fn port_max(&self) -> u16 {
        self.port_max
    }

    pub fn set_ports(&mut self, port_min: u16, port_max: u16) -> Result<(), Error> {
        if port_max < port_min {
            return Err(Error::ErrPort);
        }

        self.port_min = port_min;
        self.port_max = port_max;

        Ok(())
    }
}

/// Configuration for the underlying UDP network stack.
/// There are two ways to configure this Ephemeral and Muxed.
///
/// **Ephemeral mode**
///
/// In Ephemeral mode sockets are created and bound to random ports during ICE
/// gathering. The ports to use can be restricted by setting [`EphemeralUDP::port_min`] and
/// [`EphemeralUDP::port_max`] in which case only ports in this range will be used.
///
/// **Muxed**
///
/// In muxed mode a single UDP socket is used and all connections are muxed over this single socket.
///
#[derive(Clone)]
pub enum UDPNetwork {
    Ephemeral(EphemeralUDP),
    Muxed(Arc<dyn UDPMux + Send + Sync>),
}

impl Default for UDPNetwork {
    fn default() -> Self {
        Self::Ephemeral(Default::default())
    }
}

impl UDPNetwork {
    fn is_ephemeral(&self) -> bool {
        matches!(self, Self::Ephemeral(_))
    }

    fn is_muxed(&self) -> bool {
        matches!(self, Self::Muxed(_))
    }
}

#[cfg(test)]
mod test {
    use super::EphemeralUDP;

    #[test]
    fn test_ephemeral_udp_constructor() {
        assert!(
            EphemeralUDP::new(3000, 2999).is_err(),
            "EphemeralUDP should not allow invalid port range"
        );

        let e = EphemeralUDP::default();
        assert_eq!(e.port_min(), 0, "EphemeralUDP should default port_min to 0");
        assert_eq!(e.port_max(), 0, "EphemeralUDP should default port_max to 0");
    }

    #[test]
    fn test_ephemeral_udp_set_ports() {
        let mut e = EphemeralUDP::default();

        assert!(
            e.set_ports(3000, 2999).is_err(),
            "EphemeralUDP should not allow invalid port range"
        );

        assert!(
            e.set_ports(6000, 6001).is_ok(),
            "EphemeralUDP::set_ports should allow valid port range"
        );

        assert_eq!(
            e.port_min(),
            6000,
            "Ports set with `EphemeralUDP::set_ports` should be reflected"
        );
        assert_eq!(
            e.port_max(),
            6001,
            "Ports set with `EphemeralUDP::set_ports` should be reflected"
        );
    }
}
