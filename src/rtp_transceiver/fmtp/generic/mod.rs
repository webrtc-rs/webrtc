#[cfg(test)]
mod generic_test;

use super::*;

/// fmtp_consist checks that two FMTP parameters are not inconsistent.
fn fmtp_consist(a: &HashMap<String, String>, b: &HashMap<String, String>) -> bool {
    //TODO: add unicode case-folding equal support
    for (k, v) in a {
        if let Some(vb) = b.get(k) {
            if vb.to_uppercase() != v.to_uppercase() {
                return false;
            }
        }
    }
    for (k, v) in b {
        if let Some(va) = a.get(k) {
            if va.to_uppercase() != v.to_uppercase() {
                return false;
            }
        }
    }
    true
}

#[derive(Debug, PartialEq)]
pub(crate) struct GenericFmtp {
    pub(crate) mime_type: String,
    pub(crate) parameters: HashMap<String, String>,
}

impl Fmtp for GenericFmtp {
    fn mime_type(&self) -> &str {
        self.mime_type.as_str()
    }

    /// Match returns true if g and b are compatible fmtp descriptions
    /// The generic implementation is used for MimeTypes that are not defined
    fn match_fmtp(&self, f: &(dyn Fmtp)) -> bool {
        if let Some(c) = f.as_any().downcast_ref::<GenericFmtp>() {
            if self.mime_type != c.mime_type() {
                return false;
            }

            fmtp_consist(&self.parameters, &c.parameters)
        } else {
            false
        }
    }

    fn parameter(&self, key: &str) -> Option<&String> {
        self.parameters.get(key)
    }

    fn equal(&self, other: &(dyn Fmtp)) -> bool {
        other
            .as_any()
            .downcast_ref::<GenericFmtp>()
            .map_or(false, |a| self == a)
    }

    fn as_any(&self) -> &(dyn Any) {
        self
    }
}
