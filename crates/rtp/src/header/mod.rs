mod header_def;

pub use header_def::{Extension, Header};

pub(super) const HEADER_LENGTH: usize = 4;
pub(super) const VERSION_SHIFT: u8 = 6;
pub(super) const VERSION_MASK: u8 = 0x3;
pub(super) const PADDING_SHIFT: u8 = 5;
pub(super) const PADDING_MASK: u8 = 0x1;
pub(super) const EXTENSION_SHIFT: u8 = 4;
pub(super) const EXTENSION_MASK: u8 = 0x1;
pub(super) const EXTENSION_PROFILE_ONE_BYTE: u16 = 0xBEDE;
pub(super) const EXTENSION_PROFILE_TWO_BYTE: u16 = 0x1000;
pub(super) const EXTENSION_ID_RESERVED: u8 = 0xF;
pub(super) const CC_MASK: u8 = 0xF;
pub(super) const MARKER_SHIFT: u8 = 7;
pub(super) const MARKER_MASK: u8 = 0x1;
pub(super) const PT_MASK: u8 = 0x7F;
pub(super) const SEQ_NUM_OFFSET: usize = 2;
pub(super) const SEQ_NUM_LENGTH: usize = 2;
pub(super) const TIMESTAMP_OFFSET: usize = 4;
pub(super) const TIMESTAMP_LENGTH: usize = 4;
pub(super) const SSRC_OFFSET: usize = 8;
pub(super) const SSRC_LENGTH: usize = 4;
pub(super) const CSRC_OFFSET: usize = 12;
pub(super) const CSRC_LENGTH: usize = 4;

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
