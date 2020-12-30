//use super::*;
use crate::message::name::*;
//use crate::message::*;

// A CNAMEResource is a cname Resource record.
pub struct CNAMEResource {
    cname: Name,
}
/*
func (r *CNAMEResource) real_type() Type {
    return TypeCNAME
}

// pack appends the wire format of the CNAMEResource to msg.
func (r *CNAMEResource) pack(msg []byte, compression map[string]int, compressionOff int) ([]byte, error) {
    return r.CNAME.pack(msg, compression, compressionOff)
}

// GoString implements fmt.GoStringer.GoString.
func (r *CNAMEResource) GoString() string {
    return "dnsmessage.CNAMEResource{cname: " + r.CNAME.GoString() + "}"
}

func unpackCNAMEResource(msg []byte, off int) (CNAMEResource, error) {
    var cname Name
    if _, err := cname.unpack(msg, off); err != nil {
        return CNAMEResource{}, err
    }
    return CNAMEResource{cname}, nil
}*/
