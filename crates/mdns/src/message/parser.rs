/*
// A Parser allows incrementally parsing a DNS message.
//
// When parsing is started, the Header is parsed. Next, each question can be
// either parsed or skipped. Alternatively, all Questions can be skipped at
// once. When all Questions have been parsed, attempting to parse Questions
// will return (nil, nil) and attempting to skip Questions will return
// (true, nil). After all Questions have been either parsed or skipped, all
// Answers, Authorities and Additionals can be either parsed or skipped in the
// same way, and each type of Resource must be fully parsed or skipped before
// proceeding to the next type of Resource.
//
// Note that there is no requirement to fully skip or parse the message.
type Parser struct {
    msg    []byte
    header header

    section        section
    off            int
    index          int
    resHeaderValid bool
    resHeader      ResourceHeader
}

// Start parses the header and enables the parsing of Questions.
func (p *Parser) Start(msg []byte) (Header, error) {
    if p.msg != nil {
        *p = Parser{}
    }
    p.msg = msg
    var err error
    if p.off, err = p.header.unpack(msg, 0); err != nil {
        return Header{}, &nestedError{"unpacking header", err}
    }
    p.section = sectionQuestions
    return p.header.header(), nil
}

func (p *Parser) checkAdvance(sec section) error {
    if p.section < sec {
        return ErrNotStarted
    }
    if p.section > sec {
        return ErrSectionDone
    }
    p.resHeaderValid = false
    if p.index == int(p.header.count(sec)) {
        p.index = 0
        p.section++
        return ErrSectionDone
    }
    return nil
}

func (p *Parser) resource(sec section) (Resource, error) {
    var r Resource
    var err error
    r.Header, err = p.resourceHeader(sec)
    if err != nil {
        return r, err
    }
    p.resHeaderValid = false
    r.Body, p.off, err = unpackResourceBody(p.msg, p.off, r.Header)
    if err != nil {
        return Resource{}, &nestedError{"unpacking " + sectionNames[sec], err}
    }
    p.index++
    return r, nil
}

func (p *Parser) resourceHeader(sec section) (ResourceHeader, error) {
    if p.resHeaderValid {
        return p.resHeader, nil
    }
    if err := p.checkAdvance(sec); err != nil {
        return ResourceHeader{}, err
    }
    var hdr ResourceHeader
    off, err := hdr.unpack(p.msg, p.off)
    if err != nil {
        return ResourceHeader{}, err
    }
    p.resHeaderValid = true
    p.resHeader = hdr
    p.off = off
    return hdr, nil
}

func (p *Parser) skip_resource(sec section) error {
    if p.resHeaderValid {
        newOff := p.off + int(p.resHeader.Length)
        if newOff > len(p.msg) {
            return errResourceLen
        }
        p.off = newOff
        p.resHeaderValid = false
        p.index++
        return nil
    }
    if err := p.checkAdvance(sec); err != nil {
        return err
    }
    var err error
    p.off, err = skip_resource(p.msg, p.off)
    if err != nil {
        return &nestedError{"skipping: " + sectionNames[sec], err}
    }
    p.index++
    return nil
}

// question parses a single question.
func (p *Parser) question() (question, error) {
    if err := p.checkAdvance(sectionQuestions); err != nil {
        return question{}, err
    }
    var name Name
    off, err := name.unpack(p.msg, p.off)
    if err != nil {
        return question{}, &nestedError{"unpacking question.Name", err}
    }
    typ, off, err := unpack_type(p.msg, off)
    if err != nil {
        return question{}, &nestedError{"unpacking question.Type", err}
    }
    class, off, err := unpack_class(p.msg, off)
    if err != nil {
        return question{}, &nestedError{"unpacking question.Class", err}
    }
    p.off = off
    p.index++
    return question{name, typ, class}, nil
}

// AllQuestions parses all Questions.
func (p *Parser) AllQuestions() ([]question, error) {
    // Multiple questions are valid according to the spec,
    // but servers don't actually support them. There will
    // be at most one question here.
    //
    // Do not pre-allocate based on info in p.header, since
    // the data is untrusted.
    qs := []question{}
    for {
        q, err := p.question()
        if err == ErrSectionDone {
            return qs, nil
        }
        if err != nil {
            return nil, err
        }
        qs = append(qs, q)
    }
}

// SkipQuestion skips a single question.
func (p *Parser) SkipQuestion() error {
    if err := p.checkAdvance(sectionQuestions); err != nil {
        return err
    }
    off, err := skip_name(p.msg, p.off)
    if err != nil {
        return &nestedError{"skipping question Name", err}
    }
    if off, err = skip_type(p.msg, off); err != nil {
        return &nestedError{"skipping question Type", err}
    }
    if off, err = skip_class(p.msg, off); err != nil {
        return &nestedError{"skipping question Class", err}
    }
    p.off = off
    p.index++
    return nil
}

// SkipAllQuestions skips all Questions.
func (p *Parser) SkipAllQuestions() error {
    for {
        if err := p.SkipQuestion(); err == ErrSectionDone {
            return nil
        } else if err != nil {
            return err
        }
    }
}

// AnswerHeader parses a single Answer ResourceHeader.
func (p *Parser) AnswerHeader() (ResourceHeader, error) {
    return p.resourceHeader(sectionAnswers)
}

// Answer parses a single Answer Resource.
func (p *Parser) Answer() (Resource, error) {
    return p.resource(sectionAnswers)
}

// AllAnswers parses all Answer Resources.
func (p *Parser) AllAnswers() ([]Resource, error) {
    // The most common query is for A/AAAA, which usually returns
    // a handful of IPs.
    //
    // Pre-allocate up to a certain limit, since p.header is
    // untrusted data.
    n := int(p.header.answers)
    if n > 20 {
        n = 20
    }
    as := make([]Resource, 0, n)
    for {
        a, err := p.Answer()
        if err == ErrSectionDone {
            return as, nil
        }
        if err != nil {
            return nil, err
        }
        as = append(as, a)
    }
}

// SkipAnswer skips a single Answer Resource.
func (p *Parser) SkipAnswer() error {
    return p.skip_resource(sectionAnswers)
}

// SkipAllAnswers skips all Answer Resources.
func (p *Parser) SkipAllAnswers() error {
    for {
        if err := p.SkipAnswer(); err == ErrSectionDone {
            return nil
        } else if err != nil {
            return err
        }
    }
}

// AuthorityHeader parses a single Authority ResourceHeader.
func (p *Parser) AuthorityHeader() (ResourceHeader, error) {
    return p.resourceHeader(sectionAuthorities)
}

// Authority parses a single Authority Resource.
func (p *Parser) Authority() (Resource, error) {
    return p.resource(sectionAuthorities)
}

// AllAuthorities parses all Authority Resources.
func (p *Parser) AllAuthorities() ([]Resource, error) {
    // Authorities contains SOA in case of NXDOMAIN and friends,
    // otherwise it is empty.
    //
    // Pre-allocate up to a certain limit, since p.header is
    // untrusted data.
    n := int(p.header.authorities)
    if n > 10 {
        n = 10
    }
    as := make([]Resource, 0, n)
    for {
        a, err := p.Authority()
        if err == ErrSectionDone {
            return as, nil
        }
        if err != nil {
            return nil, err
        }
        as = append(as, a)
    }
}

// SkipAuthority skips a single Authority Resource.
func (p *Parser) SkipAuthority() error {
    return p.skip_resource(sectionAuthorities)
}

// SkipAllAuthorities skips all Authority Resources.
func (p *Parser) SkipAllAuthorities() error {
    for {
        if err := p.SkipAuthority(); err == ErrSectionDone {
            return nil
        } else if err != nil {
            return err
        }
    }
}

// AdditionalHeader parses a single Additional ResourceHeader.
func (p *Parser) AdditionalHeader() (ResourceHeader, error) {
    return p.resourceHeader(sectionAdditionals)
}

// Additional parses a single Additional Resource.
func (p *Parser) Additional() (Resource, error) {
    return p.resource(sectionAdditionals)
}

// AllAdditionals parses all Additional Resources.
func (p *Parser) AllAdditionals() ([]Resource, error) {
    // Additionals usually contain OPT, and sometimes A/AAAA
    // glue records.
    //
    // Pre-allocate up to a certain limit, since p.header is
    // untrusted data.
    n := int(p.header.additionals)
    if n > 10 {
        n = 10
    }
    as := make([]Resource, 0, n)
    for {
        a, err := p.Additional()
        if err == ErrSectionDone {
            return as, nil
        }
        if err != nil {
            return nil, err
        }
        as = append(as, a)
    }
}

// SkipAdditional skips a single Additional Resource.
func (p *Parser) SkipAdditional() error {
    return p.skip_resource(sectionAdditionals)
}

// SkipAllAdditionals skips all Additional Resources.
func (p *Parser) SkipAllAdditionals() error {
    for {
        if err := p.SkipAdditional(); err == ErrSectionDone {
            return nil
        } else if err != nil {
            return err
        }
    }
}

// cnameresource parses a single cnameresource.
//
// One of the XXXHeader methods must have been called before calling this
// method.
func (p *Parser) cnameresource() (cnameresource, error) {
    if !p.resHeaderValid || p.resHeader.Type != TypeCNAME {
        return cnameresource{}, ErrNotStarted
    }
    r, err := unpackCNAMEResource(p.msg, p.off)
    if err != nil {
        return cnameresource{}, err
    }
    p.off += int(p.resHeader.Length)
    p.resHeaderValid = false
    p.index++
    return r, nil
}

// MXResource parses a single MXResource.
//
// One of the XXXHeader methods must have been called before calling this
// method.
func (p *Parser) MXResource() (MXResource, error) {
    if !p.resHeaderValid || p.resHeader.Type != TypeMX {
        return MXResource{}, ErrNotStarted
    }
    r, err := unpackMXResource(p.msg, p.off)
    if err != nil {
        return MXResource{}, err
    }
    p.off += int(p.resHeader.Length)
    p.resHeaderValid = false
    p.index++
    return r, nil
}

// NSResource parses a single NSResource.
//
// One of the XXXHeader methods must have been called before calling this
// method.
func (p *Parser) NSResource() (NSResource, error) {
    if !p.resHeaderValid || p.resHeader.Type != TypeNS {
        return NSResource{}, ErrNotStarted
    }
    r, err := unpackNSResource(p.msg, p.off)
    if err != nil {
        return NSResource{}, err
    }
    p.off += int(p.resHeader.Length)
    p.resHeaderValid = false
    p.index++
    return r, nil
}

// PTRResource parses a single PTRResource.
//
// One of the XXXHeader methods must have been called before calling this
// method.
func (p *Parser) PTRResource() (PTRResource, error) {
    if !p.resHeaderValid || p.resHeader.Type != TypePTR {
        return PTRResource{}, ErrNotStarted
    }
    r, err := unpackPTRResource(p.msg, p.off)
    if err != nil {
        return PTRResource{}, err
    }
    p.off += int(p.resHeader.Length)
    p.resHeaderValid = false
    p.index++
    return r, nil
}

// SOAResource parses a single SOAResource.
//
// One of the XXXHeader methods must have been called before calling this
// method.
func (p *Parser) SOAResource() (SOAResource, error) {
    if !p.resHeaderValid || p.resHeader.Type != TypeSOA {
        return SOAResource{}, ErrNotStarted
    }
    r, err := unpackSOAResource(p.msg, p.off)
    if err != nil {
        return SOAResource{}, err
    }
    p.off += int(p.resHeader.Length)
    p.resHeaderValid = false
    p.index++
    return r, nil
}

// TXTResource parses a single TXTResource.
//
// One of the XXXHeader methods must have been called before calling this
// method.
func (p *Parser) TXTResource() (TXTResource, error) {
    if !p.resHeaderValid || p.resHeader.Type != TypeTXT {
        return TXTResource{}, ErrNotStarted
    }
    r, err := unpackTXTResource(p.msg, p.off, p.resHeader.Length)
    if err != nil {
        return TXTResource{}, err
    }
    p.off += int(p.resHeader.Length)
    p.resHeaderValid = false
    p.index++
    return r, nil
}

// SRVResource parses a single SRVResource.
//
// One of the XXXHeader methods must have been called before calling this
// method.
func (p *Parser) SRVResource() (SRVResource, error) {
    if !p.resHeaderValid || p.resHeader.Type != TypeSRV {
        return SRVResource{}, ErrNotStarted
    }
    r, err := unpackSRVResource(p.msg, p.off)
    if err != nil {
        return SRVResource{}, err
    }
    p.off += int(p.resHeader.Length)
    p.resHeaderValid = false
    p.index++
    return r, nil
}

// AResource parses a single AResource.
//
// One of the XXXHeader methods must have been called before calling this
// method.
func (p *Parser) AResource() (AResource, error) {
    if !p.resHeaderValid || p.resHeader.Type != TypeA {
        return AResource{}, ErrNotStarted
    }
    r, err := unpackAResource(p.msg, p.off)
    if err != nil {
        return AResource{}, err
    }
    p.off += int(p.resHeader.Length)
    p.resHeaderValid = false
    p.index++
    return r, nil
}

// AAAAResource parses a single AAAAResource.
//
// One of the XXXHeader methods must have been called before calling this
// method.
func (p *Parser) AAAAResource() (AAAAResource, error) {
    if !p.resHeaderValid || p.resHeader.Type != TypeAAAA {
        return AAAAResource{}, ErrNotStarted
    }
    r, err := unpackAAAAResource(p.msg, p.off)
    if err != nil {
        return AAAAResource{}, err
    }
    p.off += int(p.resHeader.Length)
    p.resHeaderValid = false
    p.index++
    return r, nil
}

// OPTResource parses a single OPTResource.
//
// One of the XXXHeader methods must have been called before calling this
// method.
func (p *Parser) OPTResource() (OPTResource, error) {
    if !p.resHeaderValid || p.resHeader.Type != TypeOPT {
        return OPTResource{}, ErrNotStarted
    }
    r, err := unpackOPTResource(p.msg, p.off, p.resHeader.Length)
    if err != nil {
        return OPTResource{}, err
    }
    p.off += int(p.resHeader.Length)
    p.resHeaderValid = false
    p.index++
    return r, nil
}
*/
