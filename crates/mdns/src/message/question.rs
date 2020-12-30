use super::name::*;
use super::*;

// A Question is a DNS query.
pub struct Question {
    pub name: Name,
    pub typ: DNSType,
    pub class: DNSClass,
}

/*
// pack appends the wire format of the Question to msg.
func (q *Question) pack(msg []byte, compression map[string]int, compressionOff int) ([]byte, error) {
    msg, err := q.Name.pack(msg, compression, compressionOff)
    if err != nil {
        return msg, &nestedError{"Name", err}
    }
    msg = packType(msg, q.Type)
    return packClass(msg, q.Class), nil
}

// GoString implements fmt.GoStringer.GoString.
func (q *Question) GoString() string {
    return "dnsmessage.Question{" +
        "Name: " + q.Name.GoString() + ", " +
        "Type: " + q.Type.GoString() + ", " +
        "Class: " + q.Class.GoString() + "}"
}
*/
