use crate::ice::ice_candidate_type::ICECandidateType;
use crate::ice::ice_protocol::ICEProtocol;
use serde::{Deserialize, Serialize};

/// ICECandidate represents a ice candidate
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ICECandidate {
    pub stats_id: String,
    pub foundation: String,
    pub priority: u32,
    pub address: String,
    pub protocol: ICEProtocol,
    pub port: u16,
    pub typ: ICECandidateType,
    pub component: u16,
    pub related_address: String,
    pub related_port: u16,
    pub tcp_type: String,
}

/*
// Conversion for package ice
func newICECandidatesFromICE(iceCandidates []ice.Candidate) ([]ICECandidate, error) {
    candidates := []ICECandidate{}

    for _, i := range iceCandidates {
        c, err := newICECandidateFromICE(i)
        if err != nil {
            return nil, err
        }
        candidates = append(candidates, c)
    }

    return candidates, nil
}

func newICECandidateFromICE(i ice.Candidate) (ICECandidate, error) {
    typ, err := convertTypeFromICE(i.Type())
    if err != nil {
        return ICECandidate{}, err
    }
    protocol, err := NewICEProtocol(i.NetworkType().NetworkShort())
    if err != nil {
        return ICECandidate{}, err
    }

    c := ICECandidate{
        stats_id:    i.ID(),
        foundation: i.foundation(),
        priority:   i.priority(),
        address:    i.address(),
        Protocol:   protocol,
        Port:       uint16(i.Port()),
        Component:  i.Component(),
        Typ:        typ,
        TCPType:    i.TCPType().String(),
    }

    if i.RelatedAddress() != nil {
        c.RelatedAddress = i.RelatedAddress().address
        c.RelatedPort = uint16(i.RelatedAddress().Port)
    }

    return c, nil
}

func (c ICECandidate) toICE() (ice.Candidate, error) {
    candidateID := c.stats_id
    switch c.Typ {
    case ICECandidateTypeHost:
        config := ice.CandidateHostConfig{
            CandidateID: candidateID,
            Network:     c.Protocol.String(),
            address:     c.address,
            Port:        int(c.Port),
            Component:   c.Component,
            TCPType:     ice.NewTCPType(c.TCPType),
            foundation:  c.foundation,
            priority:    c.priority,
        }
        return ice.NewCandidateHost(&config)
    case ICECandidateTypeSrflx:
        config := ice.CandidateServerReflexiveConfig{
            CandidateID: candidateID,
            Network:     c.Protocol.String(),
            address:     c.address,
            Port:        int(c.Port),
            Component:   c.Component,
            foundation:  c.foundation,
            priority:    c.priority,
            RelAddr:     c.RelatedAddress,
            RelPort:     int(c.RelatedPort),
        }
        return ice.NewCandidateServerReflexive(&config)
    case ICECandidateTypePrflx:
        config := ice.CandidatePeerReflexiveConfig{
            CandidateID: candidateID,
            Network:     c.Protocol.String(),
            address:     c.address,
            Port:        int(c.Port),
            Component:   c.Component,
            foundation:  c.foundation,
            priority:    c.priority,
            RelAddr:     c.RelatedAddress,
            RelPort:     int(c.RelatedPort),
        }
        return ice.NewCandidatePeerReflexive(&config)
    case ICECandidateTypeRelay:
        config := ice.CandidateRelayConfig{
            CandidateID: candidateID,
            Network:     c.Protocol.String(),
            address:     c.address,
            Port:        int(c.Port),
            Component:   c.Component,
            foundation:  c.foundation,
            priority:    c.priority,
            RelAddr:     c.RelatedAddress,
            RelPort:     int(c.RelatedPort),
        }
        return ice.NewCandidateRelay(&config)
    default:
        return nil, fmt.Errorf("%w: %s", errICECandidateTypeUnknown, c.Typ)
    }
}

func convertTypeFromICE(t ice.CandidateType) (ICECandidateType, error) {
    switch t {
    case ice.CandidateTypeHost:
        return ICECandidateTypeHost, nil
    case ice.CandidateTypeServerReflexive:
        return ICECandidateTypeSrflx, nil
    case ice.CandidateTypePeerReflexive:
        return ICECandidateTypePrflx, nil
    case ice.CandidateTypeRelay:
        return ICECandidateTypeRelay, nil
    default:
        return ICECandidateType(t), fmt.Errorf("%w: %s", errICECandidateTypeUnknown, t)
    }
}

func (c ICECandidate) String() string {
    ic, err := c.toICE()
    if err != nil {
        return fmt.Sprintf("%#v failed to convert to ICE: %s", c, err)
    }
    return ic.String()
}

// ToJSON returns an ICECandidateInit
// as indicated by the spec https://w3c.github.io/webrtc-pc/#dom-rtcicecandidate-tojson
func (c ICECandidate) ToJSON() ICECandidateInit {
    zeroVal := uint16(0)
    emptyStr := ""
    candidateStr := ""

    candidate, err := c.toICE()
    if err == nil {
        candidateStr = candidate.Marshal()
    }

    return ICECandidateInit{
        Candidate:     fmt.Sprintf("candidate:%s", candidateStr),
        SDPMid:        &emptyStr,
        SDPMLineIndex: &zeroVal,
    }
}
*/
