#[cfg(test)]
mod sdp_test;

pub mod common_description;
pub mod direction;
pub mod extmap;
pub mod ice_candidate;
pub mod media_description;
pub mod session_description;
pub mod util;

/*pub(crate) struct AttributeStatus {
    pub(crate) seen: bool,
    pub(crate) value: String,
    pub(crate) allow_multiple: bool,
}

impl AttributeStatus {
    pub(crate) fn attribute_valid(
        statuses: &mut [AttributeStatus],
        attribute: &str,
    ) -> Result<(), Error> {
        let mut attr_found = false;
        for v in statuses {
            if attr_found && v.seen {
                return Err(Error::new(format!(
                    "Attribute {} was found, but later attribute {} has already been set",
                    attribute, v.value
                )));
            }

            if &v.value == attribute {
                if v.seen && !v.allow_multiple {
                    return Err(Error::new(format!(
                        "Attribute {} was attempted to be set twice: {}",
                        attribute, v.value
                    )));
                }
                attr_found = true;
                v.seen = true;
            }
        }

        Ok(())
    }
}*/
