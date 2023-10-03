use std::collections::VecDeque;

use crate::packet::Packet;

/// control queue
pub(crate) type ControlQueue = VecDeque<Packet>;
