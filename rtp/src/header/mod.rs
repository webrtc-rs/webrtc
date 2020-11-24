mod header_def;
pub use header_def::Extension;
pub use header_def::Header;

const HEADER_LENGTH: usize = 4;
const VERSION_SHIFT: u8 = 6;
const VERSION_MASK: u8 = 0x3;

const PADDING_SHIFT: u8 = 5;
const PADDING_MASK: u8 = 0x1;

const EXTENSION_SHIFT: u8 = 4;
const EXTENSION_MASK: u8 = 0x1;
const EXTENSION_ID_RESERVED: u8 = 0xF;

const CC_MASK: u8 = 0xF;
const PT_MASK: u8 = 0x7F;

const MARKER_SHIFT: u8 = 7;
const MARKER_MASK: u8 = 0x1;

const SEQ_NUM_OFFSET: usize = 2;
const SEQ_NUM_LENGTH: usize = 2;

const TIMESTAMP_OFFSET: usize = 4;
const TIMESTAMP_LENGTH: usize = 4;

const SSRC_OFFSET: usize = 8;
const SSRC_LENGTH: usize = 4;

const CSRC_OFFSET: usize = 12;
const CSRC_LENGTH: usize = 4;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u16)]
pub enum ExtensionProfile {
    OneByte = 0xBEDE,
    TwoByte = 0x1000,
    Undefined,
}

impl Default for ExtensionProfile {
    fn default() -> Self {
        0.into()
    }
}

impl From<u16> for ExtensionProfile {
    fn from(val: u16) -> Self {
        match val {
            0xBEDE => ExtensionProfile::OneByte,
            0x1000 => ExtensionProfile::TwoByte,
            _ => ExtensionProfile::Undefined,
        }
    }
}

impl Into<u16> for ExtensionProfile {
    fn into(self) -> u16 {
        match self {
            ExtensionProfile::OneByte => 0xBEDE,
            ExtensionProfile::TwoByte => 0x1000,
            _ => 0x00,
        }
    }
}
