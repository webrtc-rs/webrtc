/*

// A Builder allows incrementally packing a DNS message.
//
// Example usage:
//	buf := make([]byte, 2, 514)
//	b := NewBuilder(buf, Header{...})
//	b.EnableCompression()
//	// Optionally start a section and add things to that section.
//	// Repeat adding sections as necessary.
//	buf, err := b.Finish()
//	// If err is nil, buf[2:] will contain the built bytes.
type Builder struct {
    // msg is the storage for the message being built.
    msg []byte

    // section keeps track of the current section being built.
    section section

    // header keeps track of what should go in the header when Finish is
    // called.
    header header

    // start is the starting index of the bytes allocated in msg for header.
    start int

    // compression is a mapping from name suffixes to their starting index
    // in msg.
    compression map[string]int
}

// NewBuilder creates a new builder with compression disabled.
//
// Note: Most users will want to immediately enable compression with the
// EnableCompression method. See that method's comment for why you may or may
// not want to enable compression.
//
// The DNS message is appended to the provided initial buffer buf (which may be
// nil) as it is built. The final message is returned by the (*Builder).Finish
// method, which may return the same underlying array if there was sufficient
// capacity in the slice.
func NewBuilder(buf []byte, h Header) Builder {
    if buf == nil {
        buf = make([]byte, 0, PACK_STARTING_CAP)
    }
    b := Builder{msg: buf, start: len(buf)}
    b.header.id, b.header.bits = h.pack()
    var hb [HEADER_LEN]byte
    b.msg = append(b.msg, hb[:]...)
    b.section = sectionHeader
    return b
}

// EnableCompression enables compression in the Builder.
//
// Leaving compression disabled avoids compression related allocations, but can
// result in larger message sizes. Be careful with this mode as it can cause
// messages to exceed the UDP size limit.
//
// According to RFC 1035, section 4.1.4, the use of compression is optional, but
// all implementations must accept both compressed and uncompressed DNS
// messages.
//
// Compression should be enabled before any sections are added for best results.
func (b *Builder) EnableCompression() {
    b.compression = map[string]int{}
}

func (b *Builder) startCheck(s section) error {
    if b.section <= sectionNotStarted {
        return ErrNotStarted
    }
    if b.section > s {
        return ErrSectionDone
    }
    return nil
}

// StartQuestions prepares the builder for packing Questions.
func (b *Builder) StartQuestions() error {
    if err := b.startCheck(sectionQuestions); err != nil {
        return err
    }
    b.section = sectionQuestions
    return nil
}

// StartAnswers prepares the builder for packing Answers.
func (b *Builder) StartAnswers() error {
    if err := b.startCheck(sectionAnswers); err != nil {
        return err
    }
    b.section = sectionAnswers
    return nil
}

// StartAuthorities prepares the builder for packing Authorities.
func (b *Builder) StartAuthorities() error {
    if err := b.startCheck(sectionAuthorities); err != nil {
        return err
    }
    b.section = sectionAuthorities
    return nil
}

// StartAdditionals prepares the builder for packing Additionals.
func (b *Builder) StartAdditionals() error {
    if err := b.startCheck(sectionAdditionals); err != nil {
        return err
    }
    b.section = sectionAdditionals
    return nil
}

func (b *Builder) incrementSectionCount() error {
    var count *uint16
    var err error
    switch b.section {
    case sectionQuestions:
        count = &b.header.questions
        err = errTooManyQuestions
    case sectionAnswers:
        count = &b.header.answers
        err = errTooManyAnswers
    case sectionAuthorities:
        count = &b.header.authorities
        err = errTooManyAuthorities
    case sectionAdditionals:
        count = &b.header.additionals
        err = errTooManyAdditionals
    }
    if *count == ^uint16(0) {
        return err
    }
    *count++
    return nil
}

// Question adds a single Question.
func (b *Builder) Question(q Question) error {
    if b.section < sectionQuestions {
        return ErrNotStarted
    }
    if b.section > sectionQuestions {
        return ErrSectionDone
    }
    msg, err := q.pack(b.msg, b.compression, b.start)
    if err != nil {
        return err
    }
    if err := b.incrementSectionCount(); err != nil {
        return err
    }
    b.msg = msg
    return nil
}

func (b *Builder) checkResourceSection() error {
    if b.section < sectionAnswers {
        return ErrNotStarted
    }
    if b.section > sectionAdditionals {
        return ErrSectionDone
    }
    return nil
}

// CNAMEResource adds a single CNAMEResource.
func (b *Builder) CNAMEResource(h ResourceHeader, r CNAMEResource) error {
    if err := b.checkResourceSection(); err != nil {
        return err
    }
    h.Type = r.real_type()
    msg, lenOff, err := h.pack(b.msg, b.compression, b.start)
    if err != nil {
        return &nestedError{"ResourceHeader", err}
    }
    preLen := len(msg)
    if msg, err = r.pack(msg, b.compression, b.start); err != nil {
        return &nestedError{"CNAMEResource body", err}
    }
    if err := h.fix_len(msg, lenOff, preLen); err != nil {
        return err
    }
    if err := b.incrementSectionCount(); err != nil {
        return err
    }
    b.msg = msg
    return nil
}

// MXResource adds a single MXResource.
func (b *Builder) MXResource(h ResourceHeader, r MXResource) error {
    if err := b.checkResourceSection(); err != nil {
        return err
    }
    h.Type = r.real_type()
    msg, lenOff, err := h.pack(b.msg, b.compression, b.start)
    if err != nil {
        return &nestedError{"ResourceHeader", err}
    }
    preLen := len(msg)
    if msg, err = r.pack(msg, b.compression, b.start); err != nil {
        return &nestedError{"MXResource body", err}
    }
    if err := h.fix_len(msg, lenOff, preLen); err != nil {
        return err
    }
    if err := b.incrementSectionCount(); err != nil {
        return err
    }
    b.msg = msg
    return nil
}

// NSResource adds a single NSResource.
func (b *Builder) NSResource(h ResourceHeader, r NSResource) error {
    if err := b.checkResourceSection(); err != nil {
        return err
    }
    h.Type = r.real_type()
    msg, lenOff, err := h.pack(b.msg, b.compression, b.start)
    if err != nil {
        return &nestedError{"ResourceHeader", err}
    }
    preLen := len(msg)
    if msg, err = r.pack(msg, b.compression, b.start); err != nil {
        return &nestedError{"NSResource body", err}
    }
    if err := h.fix_len(msg, lenOff, preLen); err != nil {
        return err
    }
    if err := b.incrementSectionCount(); err != nil {
        return err
    }
    b.msg = msg
    return nil
}

// PTRResource adds a single PTRResource.
func (b *Builder) PTRResource(h ResourceHeader, r PTRResource) error {
    if err := b.checkResourceSection(); err != nil {
        return err
    }
    h.Type = r.real_type()
    msg, lenOff, err := h.pack(b.msg, b.compression, b.start)
    if err != nil {
        return &nestedError{"ResourceHeader", err}
    }
    preLen := len(msg)
    if msg, err = r.pack(msg, b.compression, b.start); err != nil {
        return &nestedError{"PTRResource body", err}
    }
    if err := h.fix_len(msg, lenOff, preLen); err != nil {
        return err
    }
    if err := b.incrementSectionCount(); err != nil {
        return err
    }
    b.msg = msg
    return nil
}

// SOAResource adds a single SOAResource.
func (b *Builder) SOAResource(h ResourceHeader, r SOAResource) error {
    if err := b.checkResourceSection(); err != nil {
        return err
    }
    h.Type = r.real_type()
    msg, lenOff, err := h.pack(b.msg, b.compression, b.start)
    if err != nil {
        return &nestedError{"ResourceHeader", err}
    }
    preLen := len(msg)
    if msg, err = r.pack(msg, b.compression, b.start); err != nil {
        return &nestedError{"SOAResource body", err}
    }
    if err := h.fix_len(msg, lenOff, preLen); err != nil {
        return err
    }
    if err := b.incrementSectionCount(); err != nil {
        return err
    }
    b.msg = msg
    return nil
}

// TXTResource adds a single TXTResource.
func (b *Builder) TXTResource(h ResourceHeader, r TXTResource) error {
    if err := b.checkResourceSection(); err != nil {
        return err
    }
    h.Type = r.real_type()
    msg, lenOff, err := h.pack(b.msg, b.compression, b.start)
    if err != nil {
        return &nestedError{"ResourceHeader", err}
    }
    preLen := len(msg)
    if msg, err = r.pack(msg, b.compression, b.start); err != nil {
        return &nestedError{"TXTResource body", err}
    }
    if err := h.fix_len(msg, lenOff, preLen); err != nil {
        return err
    }
    if err := b.incrementSectionCount(); err != nil {
        return err
    }
    b.msg = msg
    return nil
}

// SRVResource adds a single SRVResource.
func (b *Builder) SRVResource(h ResourceHeader, r SRVResource) error {
    if err := b.checkResourceSection(); err != nil {
        return err
    }
    h.Type = r.real_type()
    msg, lenOff, err := h.pack(b.msg, b.compression, b.start)
    if err != nil {
        return &nestedError{"ResourceHeader", err}
    }
    preLen := len(msg)
    if msg, err = r.pack(msg, b.compression, b.start); err != nil {
        return &nestedError{"SRVResource body", err}
    }
    if err := h.fix_len(msg, lenOff, preLen); err != nil {
        return err
    }
    if err := b.incrementSectionCount(); err != nil {
        return err
    }
    b.msg = msg
    return nil
}

// AResource adds a single AResource.
func (b *Builder) AResource(h ResourceHeader, r AResource) error {
    if err := b.checkResourceSection(); err != nil {
        return err
    }
    h.Type = r.real_type()
    msg, lenOff, err := h.pack(b.msg, b.compression, b.start)
    if err != nil {
        return &nestedError{"ResourceHeader", err}
    }
    preLen := len(msg)
    if msg, err = r.pack(msg, b.compression, b.start); err != nil {
        return &nestedError{"AResource body", err}
    }
    if err := h.fix_len(msg, lenOff, preLen); err != nil {
        return err
    }
    if err := b.incrementSectionCount(); err != nil {
        return err
    }
    b.msg = msg
    return nil
}

// AAAAResource adds a single AAAAResource.
func (b *Builder) AAAAResource(h ResourceHeader, r AAAAResource) error {
    if err := b.checkResourceSection(); err != nil {
        return err
    }
    h.Type = r.real_type()
    msg, lenOff, err := h.pack(b.msg, b.compression, b.start)
    if err != nil {
        return &nestedError{"ResourceHeader", err}
    }
    preLen := len(msg)
    if msg, err = r.pack(msg, b.compression, b.start); err != nil {
        return &nestedError{"AAAAResource body", err}
    }
    if err := h.fix_len(msg, lenOff, preLen); err != nil {
        return err
    }
    if err := b.incrementSectionCount(); err != nil {
        return err
    }
    b.msg = msg
    return nil
}

// OPTResource adds a single OPTResource.
func (b *Builder) OPTResource(h ResourceHeader, r OPTResource) error {
    if err := b.checkResourceSection(); err != nil {
        return err
    }
    h.Type = r.real_type()
    msg, lenOff, err := h.pack(b.msg, b.compression, b.start)
    if err != nil {
        return &nestedError{"ResourceHeader", err}
    }
    preLen := len(msg)
    if msg, err = r.pack(msg, b.compression, b.start); err != nil {
        return &nestedError{"OPTResource body", err}
    }
    if err := h.fix_len(msg, lenOff, preLen); err != nil {
        return err
    }
    if err := b.incrementSectionCount(); err != nil {
        return err
    }
    b.msg = msg
    return nil
}

// Finish ends message building and generates a binary message.
func (b *Builder) Finish() ([]byte, error) {
    if b.section < sectionHeader {
        return nil, ErrNotStarted
    }
    b.section = sectionDone
    // Space for the header was allocated in NewBuilder.
    b.header.pack(b.msg[b.start:b.start])
    return b.msg, nil
}

 */
