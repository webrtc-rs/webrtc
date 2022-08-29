use crate::packet::Packet;

use std::collections::VecDeque;

/// control queue
pub(crate) type ControlQueue = VecDeque<Packet>;
