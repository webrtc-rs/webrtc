use std::fmt;
use std::sync::{Arc, Mutex};

/// Sequencer generates sequential sequence numbers for building RTP packets
pub trait Sequencer: fmt::Debug {
    fn next_sequence_number(&self) -> u16;
    fn roll_over_count(&self) -> u64;
    fn clone_to(&self) -> Box<dyn Sequencer + Send + Sync>;
}

impl Clone for Box<dyn Sequencer + Send + Sync> {
    fn clone(&self) -> Box<dyn Sequencer + Send + Sync> {
        self.clone_to()
    }
}

/// NewRandomSequencer returns a new sequencer starting from a random sequence
/// number
pub fn new_random_sequencer() -> impl Sequencer {
    let c = Counters {
        sequence_number: rand::random::<u16>(),
        roll_over_count: 0,
    };
    SequencerImpl(Arc::new(Mutex::new(c)))
}

/// NewFixedSequencer returns a new sequencer starting from a specific
/// sequence number
pub fn new_fixed_sequencer(s: u16) -> impl Sequencer {
    let sequence_number = if s == 0 { u16::MAX } else { s - 1 };

    let c = Counters {
        sequence_number,
        roll_over_count: 0,
    };

    SequencerImpl(Arc::new(Mutex::new(c)))
}

#[derive(Debug, Clone)]
struct SequencerImpl(Arc<Mutex<Counters>>);

#[derive(Debug)]
struct Counters {
    sequence_number: u16,
    roll_over_count: u64,
}

impl Sequencer for SequencerImpl {
    /// NextSequenceNumber increment and returns a new sequence number for
    /// building RTP packets
    fn next_sequence_number(&self) -> u16 {
        let mut lock = self.0.lock().unwrap();

        if lock.sequence_number == u16::MAX {
            lock.roll_over_count += 1;
            lock.sequence_number = 0;
        } else {
            lock.sequence_number += 1;
        }

        lock.sequence_number
    }

    /// RollOverCount returns the amount of times the 16bit sequence number
    /// has wrapped
    fn roll_over_count(&self) -> u64 {
        self.0.lock().unwrap().roll_over_count
    }

    fn clone_to(&self) -> Box<dyn Sequencer + Send + Sync> {
        Box::new(self.clone())
    }
}
