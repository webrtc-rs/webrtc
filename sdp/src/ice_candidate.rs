use std::io;

use util::Error;

#[cfg(test)]
mod ice_candidate_test;

// ICECandidate is used to (un)marshal ICE candidates.
#[derive(Debug, Default)]
pub struct ICECandidate {
    foundation: String,
    component: u16,
    priority: u32,
    address: String,
    protocol: String,
    port: u16,
    typ: String,
    related_address: String,
    related_port: u16,
    extension_attributes: Vec<ICECandidateAttribute>,
}

// ICECandidateAttribute represents an ICE candidate extension attribute
#[derive(Debug, Default)]
struct ICECandidateAttribute {
    key: String,
    value: String,
}

// https://tools.ietf.org/html/draft-ietf-mmusic-ice-sip-sdp-24#section-4.1
// candidate-attribute   = "candidate" ":" foundation SP component-id SP
//                            transport SP
//                            priority SP
//                            connection-address SP     ;from RFC 4566
//                            port         ;port from RFC 4566
//                            SP cand-type
//                            [SP rel-addr]
//                            [SP rel-port]
//                            *(SP extension-att-name SP
//                                 extension-att-value)

impl ICECandidate {
    // Marshal returns the string representation of the ICECandidate
    pub fn marshal(&self) -> String {
        let mut val = format!(
            "{} {} {} {} {} {} typ {}",
            self.foundation,
            self.component,
            self.protocol,
            self.priority,
            self.address,
            self.port,
            self.typ,
        );
        if self.related_address.len() > 0 {
            val += format!(
                " raddr {} rport {}",
                self.related_address, self.related_port
            )
            .as_str();
        }
        for attr in &self.extension_attributes {
            val += format!(" {} {}", attr.key, attr.value).as_str();
        }
        val
    }

    // Unmarshal popuulates the ICECandidate from its string representation
    pub fn unmarshal<R: io::BufRead>(reader: &mut R) -> Result<Self, Error> {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let split: Vec<&str> = line.split_whitespace().collect();
        if split.len() < 8 {
            return Err(Error::new(format!(
                "attribute not long enough to be ICE candidate ({})",
                split.len(),
            )));
        }

        // Foundation
        let foundation = split[0];

        // Component
        let component = split[1].parse::<u16>()?;

        // Protocol
        let protocol = split[2];

        // Priority
        let priority = split[3].parse::<u32>()?;

        // Address
        let address = split[4];

        // Port
        let port = split[5].parse::<u16>()?;

        let typ = split[7];

        let mut ice_candidate = ICECandidate {
            foundation: foundation.to_owned(),
            component,
            priority,
            address: address.to_owned(),
            protocol: protocol.to_owned(),
            port,
            typ: typ.to_owned(),
            related_address: "".to_owned(),
            related_port: 0,
            extension_attributes: vec![],
        };

        if split.len() <= 8 {
            return Ok(ice_candidate);
        }
        let mut l = 8;
        if split[l + 0] == "raddr" {
            if split.len() < l + 4 {
                return Err(Error::new(format!(
                    "could not parse related addresses: incorrect length"
                )));
            }

            // RelatedAddress
            ice_candidate.related_address = split[l + 1].to_owned();

            // RelatedPort
            ice_candidate.related_port = split[l + 3].parse::<u16>()?;

            if split.len() <= l + 4 {
                return Ok(ice_candidate);
            }

            l += 4;
        }

        for i in (l..split.len() - 1).step_by(2) {
            ice_candidate
                .extension_attributes
                .push(ICECandidateAttribute {
                    key: split[i].to_owned(),
                    value: split[i + 1].to_owned(),
                });
        }

        Ok(ice_candidate)
    }
}
