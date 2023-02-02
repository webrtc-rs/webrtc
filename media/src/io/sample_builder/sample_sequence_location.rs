use super::seqnum_distance;

#[derive(Debug, PartialEq)]
pub(crate) enum Comparison {
    Void,
    Before,
    Inside,
    After,
}

pub(crate) struct Iterator<'a, T> {
    data: &'a [Option<T>],
    sample: SampleSequenceLocation,
    i: u16,
}

impl<'a, T> std::iter::Iterator for Iterator<'a, T> {
    type Item = Option<&'a T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.sample.compare(self.i) == Comparison::Inside {
            let old_i = self.i as usize;
            self.i = self.i.wrapping_add(1);
            return Some(self.data[old_i].as_ref());
        }

        None
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SampleSequenceLocation {
    /// head is the first packet in a sequence
    pub(crate) head: u16,
    /// tail is always set to one after the final sequence number,
    /// so if `head == tail` then the sequence is empty
    pub(crate) tail: u16,
}

impl SampleSequenceLocation {
    pub(crate) fn new() -> Self {
        Self { head: 0, tail: 0 }
    }

    pub(crate) fn empty(&self) -> bool {
        self.head == self.tail
    }

    pub(crate) fn has_data(&self) -> bool {
        self.head != self.tail
    }

    pub(crate) fn count(&self) -> u16 {
        seqnum_distance(self.head, self.tail)
    }

    pub(crate) fn compare(&self, pos: u16) -> Comparison {
        if self.head == self.tail {
            return Comparison::Void;
        }
        if self.head < self.tail {
            if self.head <= pos && pos < self.tail {
                return Comparison::Inside;
            }
        } else if self.head <= pos || pos < self.tail {
            return Comparison::Inside;
        }
        if self.head.wrapping_sub(pos) <= pos.wrapping_sub(self.tail) {
            return Comparison::Before;
        }
        Comparison::After
    }

    pub(crate) fn range<'a, T>(&self, data: &'a [Option<T>]) -> Iterator<'a, T> {
        Iterator {
            data,
            sample: *self,
            i: self.head,
        }
    }
}
