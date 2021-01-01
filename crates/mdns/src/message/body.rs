use super::header::*;
use super::question::*;
use super::resource::*;
//use super::*;

// Message is a representation of a DNS message.
pub struct Message {
    header: Header,
    questions: Vec<Question>,
    answers: Vec<Resource>,
    authorities: Vec<Resource>,
    additionals: Vec<Resource>,
}

/*

// Unpack parses a full Message.
func (m *Message) Unpack(msg []byte) error {
    var p Parser
    var err error
    if m.Header, err = p.Start(msg); err != nil {
        return err
    }
    if m.Questions, err = p.AllQuestions(); err != nil {
        return err
    }
    if m.Answers, err = p.AllAnswers(); err != nil {
        return err
    }
    if m.Authorities, err = p.AllAuthorities(); err != nil {
        return err
    }
    if m.Additionals, err = p.AllAdditionals(); err != nil {
        return err
    }
    return nil
}

// Pack packs a full Message.
func (m *Message) Pack() ([]byte, error) {
    return m.AppendPack(make([]byte, 0, PACK_STARTING_CAP))
}

// AppendPack is like Pack but appends the full Message to b and returns the
// extended buffer.
func (m *Message) AppendPack(b []byte) ([]byte, error) {
    // Validate the lengths. It is very unlikely that anyone will try to
    // pack more than 65535 of any particular type, but it is possible and
    // we should fail gracefully.
    if len(m.Questions) > int(^uint16(0)) {
        return nil, errTooManyQuestions
    }
    if len(m.Answers) > int(^uint16(0)) {
        return nil, errTooManyAnswers
    }
    if len(m.Authorities) > int(^uint16(0)) {
        return nil, errTooManyAuthorities
    }
    if len(m.Additionals) > int(^uint16(0)) {
        return nil, errTooManyAdditionals
    }

    var h header
    h.id, h.bits = m.Header.pack()

    h.questions = uint16(len(m.Questions))
    h.answers = uint16(len(m.Answers))
    h.authorities = uint16(len(m.Authorities))
    h.additionals = uint16(len(m.Additionals))

    compressionOff := len(b)
    msg := h.pack(b)

    // RFC 1035 allows (but does not require) compression for packing. RFC
    // 1035 requires unpacking implementations to support compression, so
    // unconditionally enabling it is fine.
    //
    // DNS lookups are typically done over UDP, and RFC 1035 states that UDP
    // DNS messages can be a maximum of 512 bytes long. Without compression,
    // many DNS response messages are over this limit, so enabling
    // compression will help ensure compliance.
    compression := map[string]int{}

    for i := range m.Questions {
        var err error
        if msg, err = m.Questions[i].pack(msg, compression, compressionOff); err != nil {
            return nil, &nestedError{"packing Question", err}
        }
    }
    for i := range m.Answers {
        var err error
        if msg, err = m.Answers[i].pack(msg, compression, compressionOff); err != nil {
            return nil, &nestedError{"packing Answer", err}
        }
    }
    for i := range m.Authorities {
        var err error
        if msg, err = m.Authorities[i].pack(msg, compression, compressionOff); err != nil {
            return nil, &nestedError{"packing Authority", err}
        }
    }
    for i := range m.Additionals {
        var err error
        if msg, err = m.Additionals[i].pack(msg, compression, compressionOff); err != nil {
            return nil, &nestedError{"packing Additional", err}
        }
    }

    return msg, nil
}

// GoString implements fmt.GoStringer.GoString.
func (m *Message) GoString() string {
    s := "dnsmessage.Message{Header: " + m.Header.GoString() + ", " +
        "Questions: []dnsmessage.Question{"
    if len(m.Questions) > 0 {
        s += m.Questions[0].GoString()
        for _, q := range m.Questions[1:] {
            s += ", " + q.GoString()
        }
    }
    s += "}, Answers: []dnsmessage.Resource{"
    if len(m.Answers) > 0 {
        s += m.Answers[0].GoString()
        for _, a := range m.Answers[1:] {
            s += ", " + a.GoString()
        }
    }
    s += "}, Authorities: []dnsmessage.Resource{"
    if len(m.Authorities) > 0 {
        s += m.Authorities[0].GoString()
        for _, a := range m.Authorities[1:] {
            s += ", " + a.GoString()
        }
    }
    s += "}, Additionals: []dnsmessage.Resource{"
    if len(m.Additionals) > 0 {
        s += m.Additionals[0].GoString()
        for _, a := range m.Additionals[1:] {
            s += ", " + a.GoString()
        }
    }
    return s + "}}"
}
 */
