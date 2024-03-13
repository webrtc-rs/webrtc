use std::fmt;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use portable_atomic::{AtomicU16, AtomicU64};

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
        sequence_number: Arc::new(AtomicU16::new(rand::random::<u16>())),
        roll_over_count: Arc::new(AtomicU64::new(0)),
    };
    SequencerImpl(c)
}

/// NewFixedSequencer returns a new sequencer starting from a specific
/// sequence number
pub fn new_fixed_sequencer(s: u16) -> impl Sequencer {
    let sequence_number = if s == 0 { u16::MAX } else { s - 1 };

    let c = Counters {
        sequence_number: Arc::new(AtomicU16::new(sequence_number)),
        roll_over_count: Arc::new(AtomicU64::new(0)),
    };

    SequencerImpl(c)
}

#[derive(Debug, Clone)]
struct SequencerImpl(Counters);

#[derive(Debug, Clone)]
struct Counters {
    sequence_number: Arc<AtomicU16>,
    roll_over_count: Arc<AtomicU64>,
}

impl Sequencer for SequencerImpl {
    /// NextSequenceNumber increment and returns a new sequence number for
    /// building RTP packets
    fn next_sequence_number(&self) -> u16 {
        if self.0.sequence_number.load(Ordering::SeqCst) == u16::MAX {
            self.0.roll_over_count.fetch_add(1, Ordering::SeqCst);
            self.0.sequence_number.store(0, Ordering::SeqCst);
            0
        } else {
            self.0.sequence_number.fetch_add(1, Ordering::SeqCst) + 1
        }
    }

    /// RollOverCount returns the amount of times the 16bit sequence number
    /// has wrapped
    fn roll_over_count(&self) -> u64 {
        self.0.roll_over_count.load(Ordering::SeqCst)
    }

    fn clone_to(&self) -> Box<dyn Sequencer + Send + Sync> {
        Box::new(self.clone())
    }
}
