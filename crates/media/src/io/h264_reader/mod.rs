use std::fmt;

/// NalUnitType is the type of a NAL
/// Enums for NalUnitTypes
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum NalUnitType {
    /// Unspecified
    Unspecified = 0,
    /// Coded slice of a non-IDR picture
    CodedSliceNonIdr = 1,
    /// Coded slice data partition A
    CodedSliceDataPartitionA = 2,
    /// Coded slice data partition B
    CodedSliceDataPartitionB = 3,
    /// Coded slice data partition C
    CodedSliceDataPartitionC = 4,
    /// Coded slice of an IDR picture
    CodedSliceIdr = 5,
    /// Supplemental enhancement information (SEI)
    SEI = 6,
    /// Sequence parameter set
    SPS = 7,
    /// Picture parameter set
    PPS = 8,
    /// Access unit delimiter
    AUD = 9,
    /// End of sequence
    EndOfSequence = 10,
    /// End of stream
    EndOfStream = 11,
    /// Filler data
    Filler = 12,
    /// Sequence parameter set extension
    SpsExt = 13,
    /// Coded slice of an auxiliary coded picture without partitioning
    CodedSliceAux = 19,
    ///Reserved
    Reserved,
    // 14..18                                            // Reserved
    // 20..23                                            // Reserved
    // 24..31                                            // Unspecified
}

impl Default for NalUnitType {
    fn default() -> Self {
        NalUnitType::Unspecified
    }
}

impl fmt::Display for NalUnitType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            NalUnitType::Unspecified => "Unspecified",
            NalUnitType::CodedSliceNonIdr => "CodedSliceNonIdr",
            NalUnitType::CodedSliceDataPartitionA => "CodedSliceDataPartitionA",
            NalUnitType::CodedSliceDataPartitionB => "CodedSliceDataPartitionB",
            NalUnitType::CodedSliceDataPartitionC => "CodedSliceDataPartitionC",
            NalUnitType::CodedSliceIdr => "CodedSliceIdr",
            NalUnitType::SEI => "SEI",
            NalUnitType::SPS => "SPS",
            NalUnitType::PPS => "PPS",
            NalUnitType::AUD => "AUD",
            NalUnitType::EndOfSequence => "EndOfSequence",
            NalUnitType::EndOfStream => "EndOfStream",
            NalUnitType::Filler => "Filler",
            NalUnitType::SpsExt => "SpsExt",
            NalUnitType::CodedSliceAux => "NalUnitTypeCodedSliceAux",
            _ => "Reserved",
        };
        write!(f, "{}({})", s, *self as u8)
    }
}

impl From<u8> for NalUnitType {
    fn from(v: u8) -> Self {
        match v {
            0 => NalUnitType::Unspecified,
            1 => NalUnitType::CodedSliceNonIdr,
            2 => NalUnitType::CodedSliceDataPartitionA,
            3 => NalUnitType::CodedSliceDataPartitionB,
            4 => NalUnitType::CodedSliceDataPartitionC,
            5 => NalUnitType::CodedSliceIdr,
            6 => NalUnitType::SEI,
            7 => NalUnitType::SPS,
            8 => NalUnitType::PPS,
            9 => NalUnitType::AUD,
            10 => NalUnitType::EndOfSequence,
            11 => NalUnitType::EndOfStream,
            12 => NalUnitType::Filler,
            13 => NalUnitType::SpsExt,
            19 => NalUnitType::CodedSliceAux,
            _ => NalUnitType::Reserved,
        }
    }
}
