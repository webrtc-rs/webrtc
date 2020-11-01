use std::sync::{Arc, Mutex};

// Sequencer generates sequential sequence numbers for building RTP packets
pub trait Sequencer {
    fn next_sequence_number(&mut self) -> u16;
    fn roll_over_count(&self) -> u64;
}

// NewRandomSequencer returns a new sequencer starting from a random sequence
// number
pub fn new_random_sequencer() -> impl Sequencer {
    SequencerImpl {
        mutex: Arc::new(Mutex::new(SequencerInternal {
            sequence_number: rand::random::<u16>(),
            roll_over_count: 0,
        })),
    }
}

// NewFixedSequencer returns a new sequencer starting from a specific
// sequence number
pub fn new_fixed_sequencer(s: u16) -> impl Sequencer {
    SequencerImpl {
        mutex: Arc::new(Mutex::new(SequencerInternal {
            sequence_number: s - 1,
            roll_over_count: 0,
        })),
    }
}

struct SequencerInternal {
    sequence_number: u16,
    roll_over_count: u64,
}

struct SequencerImpl {
    mutex: Arc<Mutex<SequencerInternal>>,
}

// NextSequenceNumber increment and returns a new sequence number for
// building RTP packets
impl Sequencer for SequencerImpl {
    fn next_sequence_number(&mut self) -> u16 {
        let mut s = self.mutex.lock().unwrap();
        s.sequence_number += 1;
        if s.sequence_number == 0 {
            s.roll_over_count += 1;
        }
        s.sequence_number
    }

    // RollOverCount returns the amount of times the 16bit sequence number
    // has wrapped
    fn roll_over_count(&self) -> u64 {
        let s = self.mutex.lock().unwrap();
        s.roll_over_count
    }
}
