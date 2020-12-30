pub mod body;
pub mod header;
pub mod name;
pub mod question;
pub mod resource;

use std::fmt;

// Message formats

// A Type is a type of DNS request and response.
#[derive(Copy, Clone)]
pub enum DNSType {
    // ResourceHeader.Type and Question.Type
    A = 1,
    NS = 2,
    CNAME = 5,
    SOA = 6,
    PTR = 12,
    MX = 15,
    TXT = 16,
    AAAA = 28,
    SRV = 33,
    OPT = 41,

    // Question.Type
    WKS = 11,
    HINFO = 13,
    MINFO = 14,
    AXFR = 252,
    ALL = 255,

    Unsupported,
}

impl fmt::Display for DNSType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            DNSType::A => "TypeA",
            DNSType::NS => "TypeNS",
            DNSType::CNAME => "TypeCNAME",
            DNSType::SOA => "TypeSOA",
            DNSType::PTR => "TypePTR",
            DNSType::MX => "TypeMX",
            DNSType::TXT => "TypeTXT",
            DNSType::AAAA => "TypeAAAA",
            DNSType::SRV => "TypeSRV",
            DNSType::OPT => "TypeOPT",
            DNSType::WKS => "TypeWKS",
            DNSType::HINFO => "TypeHINFO",
            DNSType::MINFO => "TypeMINFO",
            DNSType::AXFR => "TypeAXFR",
            DNSType::ALL => "TypeALL",
            _ => "Unsupported",
        };
        write!(f, "{}", s)
    }
}

// A Class is a type of network.
#[derive(Copy, Clone)]
pub enum DNSClass {
    // ResourceHeader.Class and Question.Class
    INET = 1,
    CSNET = 2,
    CHAOS = 3,
    HESIOD = 4,

    // Question.Class
    ANY = 255,
}

impl fmt::Display for DNSClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            DNSClass::INET => "ClassINET",
            DNSClass::CSNET => "ClassCSNET",
            DNSClass::CHAOS => "ClassCHAOS",
            DNSClass::HESIOD => "ClassHESIOD",
            DNSClass::ANY => "ClassANY",
        };
        write!(f, "{}", s)
    }
}

// An OpCode is a DNS operation code.
type OpCode = u16;

// An RCode is a DNS response status code.
#[derive(Copy, Clone)]
pub enum RCode {
    // Message.Rcode
    Success = 0,
    FormatError = 1,
    ServerFailure = 2,
    NameError = 3,
    NotImplemented = 4,
    Refused = 5,
}

impl fmt::Display for RCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RCode::Success => "RCodeSuccess",
            RCode::FormatError => "RCodeFormatError",
            RCode::ServerFailure => "RCodeServerFailure",
            RCode::NameError => "RCodeNameError",
            RCode::NotImplemented => "RCodeNotImplemented",
            RCode::Refused => "RCodeRefused",
        };
        write!(f, "{}", s)
    }
}

// Internal constants.

// PACK_STARTING_CAP is the default initial buffer size allocated during
// packing.
//
// The starting capacity doesn't matter too much, but most DNS responses
// Will be <= 512 bytes as it is the limit for DNS over UDP.
const PACK_STARTING_CAP: usize = 512;

// UINT16LEN is the length (in bytes) of a uint16.
const UINT16LEN: usize = 2;

// UINT32LEN is the length (in bytes) of a uint32.
const UINT32LEN: usize = 4;

// HEADER_LEN is the length (in bytes) of a DNS header.
//
// A header is comprised of 6 uint16s and no padding.
const HEADER_LEN: usize = 6 * UINT16LEN;

const HEADER_BIT_QR: u16 = 1 << 15; // query/response (response=1)
const HEADER_BIT_AA: u16 = 1 << 10; // authoritative
const HEADER_BIT_TC: u16 = 1 << 9; // truncated
const HEADER_BIT_RD: u16 = 1 << 8; // recursion desired
const HEADER_BIT_RA: u16 = 1 << 7; // recursion available

enum Section {
    NotStarted,
    Header,
    Questions,
    Answers,
    Authorities,
    Additionals,
    Done,
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Section::NotStarted => "NotStarted",
            Section::Header => "Header",
            Section::Questions => "Question",
            Section::Answers => "Answer",
            Section::Authorities => "Authority",
            Section::Additionals => "Additional",
            Section::Done => "Done",
        };
        write!(f, "{}", s)
    }
}
/*



// header is the wire format for a DNS message header.
type header struct {
    id          uint16
    bits        uint16
    questions   uint16
    answers     uint16
    authorities uint16
    additionals uint16
}

func (h *header) count(sec section) uint16 {
    switch sec {
    case sectionQuestions:
        return h.questions
    case sectionAnswers:
        return h.answers
    case sectionAuthorities:
        return h.authorities
    case sectionAdditionals:
        return h.additionals
    }
    return 0
}

// pack appends the wire format of the header to msg.
func (h *header) pack(msg []byte) []byte {
    msg = packUint16(msg, h.id)
    msg = packUint16(msg, h.bits)
    msg = packUint16(msg, h.questions)
    msg = packUint16(msg, h.answers)
    msg = packUint16(msg, h.authorities)
    return packUint16(msg, h.additionals)
}

func (h *header) unpack(msg []byte, off int) (int, error) {
    newOff := off
    var err error
    if h.id, newOff, err = unpackUint16(msg, newOff); err != nil {
        return off, &nestedError{"id", err}
    }
    if h.bits, newOff, err = unpackUint16(msg, newOff); err != nil {
        return off, &nestedError{"bits", err}
    }
    if h.questions, newOff, err = unpackUint16(msg, newOff); err != nil {
        return off, &nestedError{"questions", err}
    }
    if h.answers, newOff, err = unpackUint16(msg, newOff); err != nil {
        return off, &nestedError{"answers", err}
    }
    if h.authorities, newOff, err = unpackUint16(msg, newOff); err != nil {
        return off, &nestedError{"authorities", err}
    }
    if h.additionals, newOff, err = unpackUint16(msg, newOff); err != nil {
        return off, &nestedError{"additionals", err}
    }
    return newOff, nil
}

func (h *header) header() Header {
    return Header{
        ID:                 h.id,
        Response:           (h.bits & HEADER_BIT_QR) != 0,
        OpCode:             OpCode(h.bits>>11) & 0xF,
        Authoritative:      (h.bits & HEADER_BIT_AA) != 0,
        Truncated:          (h.bits & HEADER_BIT_TC) != 0,
        RecursionDesired:   (h.bits & HEADER_BIT_RD) != 0,
        RecursionAvailable: (h.bits & HEADER_BIT_RA) != 0,
        RCode:              RCode(h.bits & 0xF),
    }
}


// A Parser allows incrementally parsing a DNS message.
//
// When parsing is started, the Header is parsed. Next, each Question can be
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

func (p *Parser) skipResource(sec section) error {
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
    p.off, err = skipResource(p.msg, p.off)
    if err != nil {
        return &nestedError{"skipping: " + sectionNames[sec], err}
    }
    p.index++
    return nil
}

// Question parses a single Question.
func (p *Parser) Question() (Question, error) {
    if err := p.checkAdvance(sectionQuestions); err != nil {
        return Question{}, err
    }
    var name Name
    off, err := name.unpack(p.msg, p.off)
    if err != nil {
        return Question{}, &nestedError{"unpacking Question.Name", err}
    }
    typ, off, err := unpackType(p.msg, off)
    if err != nil {
        return Question{}, &nestedError{"unpacking Question.Type", err}
    }
    class, off, err := unpackClass(p.msg, off)
    if err != nil {
        return Question{}, &nestedError{"unpacking Question.Class", err}
    }
    p.off = off
    p.index++
    return Question{name, typ, class}, nil
}

// AllQuestions parses all Questions.
func (p *Parser) AllQuestions() ([]Question, error) {
    // Multiple questions are valid according to the spec,
    // but servers don't actually support them. There will
    // be at most one question here.
    //
    // Do not pre-allocate based on info in p.header, since
    // the data is untrusted.
    qs := []Question{}
    for {
        q, err := p.Question()
        if err == ErrSectionDone {
            return qs, nil
        }
        if err != nil {
            return nil, err
        }
        qs = append(qs, q)
    }
}

// SkipQuestion skips a single Question.
func (p *Parser) SkipQuestion() error {
    if err := p.checkAdvance(sectionQuestions); err != nil {
        return err
    }
    off, err := skip_name(p.msg, p.off)
    if err != nil {
        return &nestedError{"skipping Question Name", err}
    }
    if off, err = skipType(p.msg, off); err != nil {
        return &nestedError{"skipping Question Type", err}
    }
    if off, err = skipClass(p.msg, off); err != nil {
        return &nestedError{"skipping Question Class", err}
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
    return p.skipResource(sectionAnswers)
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
    return p.skipResource(sectionAuthorities)
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
    return p.skipResource(sectionAdditionals)
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

// CNAMEResource parses a single CNAMEResource.
//
// One of the XXXHeader methods must have been called before calling this
// method.
func (p *Parser) CNAMEResource() (CNAMEResource, error) {
    if !p.resHeaderValid || p.resHeader.Type != TypeCNAME {
        return CNAMEResource{}, ErrNotStarted
    }
    r, err := unpackCNAMEResource(p.msg, p.off)
    if err != nil {
        return CNAMEResource{}, err
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
    if err := h.fixLen(msg, lenOff, preLen); err != nil {
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
    if err := h.fixLen(msg, lenOff, preLen); err != nil {
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
    if err := h.fixLen(msg, lenOff, preLen); err != nil {
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
    if err := h.fixLen(msg, lenOff, preLen); err != nil {
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
    if err := h.fixLen(msg, lenOff, preLen); err != nil {
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
    if err := h.fixLen(msg, lenOff, preLen); err != nil {
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
    if err := h.fixLen(msg, lenOff, preLen); err != nil {
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
    if err := h.fixLen(msg, lenOff, preLen); err != nil {
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
    if err := h.fixLen(msg, lenOff, preLen); err != nil {
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
    if err := h.fixLen(msg, lenOff, preLen); err != nil {
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

// packUint16 appends the wire format of field to msg.
func packUint16(msg []byte, field uint16) []byte {
    return append(msg, byte(field>>8), byte(field))
}

func unpackUint16(msg []byte, off int) (uint16, int, error) {
    if off+UINT16LEN > len(msg) {
        return 0, off, errBaseLen
    }
    return uint16(msg[off])<<8 | uint16(msg[off+1]), off + UINT16LEN, nil
}

func skipUint16(msg []byte, off int) (int, error) {
    if off+UINT16LEN > len(msg) {
        return off, errBaseLen
    }
    return off + UINT16LEN, nil
}

// packType appends the wire format of field to msg.
func packType(msg []byte, field Type) []byte {
    return packUint16(msg, uint16(field))
}

func unpackType(msg []byte, off int) (Type, int, error) {
    t, o, err := unpackUint16(msg, off)
    return Type(t), o, err
}

func skipType(msg []byte, off int) (int, error) {
    return skipUint16(msg, off)
}

// packClass appends the wire format of field to msg.
func packClass(msg []byte, field Class) []byte {
    return packUint16(msg, uint16(field))
}

func unpackClass(msg []byte, off int) (Class, int, error) {
    c, o, err := unpackUint16(msg, off)
    return Class(c), o, err
}

func skipClass(msg []byte, off int) (int, error) {
    return skipUint16(msg, off)
}

// packUint32 appends the wire format of field to msg.
func packUint32(msg []byte, field uint32) []byte {
    return append(
        msg,
        byte(field>>24),
        byte(field>>16),
        byte(field>>8),
        byte(field),
    )
}

func unpackUint32(msg []byte, off int) (uint32, int, error) {
    if off+UINT32LEN > len(msg) {
        return 0, off, errBaseLen
    }
    v := uint32(msg[off])<<24 | uint32(msg[off+1])<<16 | uint32(msg[off+2])<<8 | uint32(msg[off+3])
    return v, off + UINT32LEN, nil
}

func skipUint32(msg []byte, off int) (int, error) {
    if off+UINT32LEN > len(msg) {
        return off, errBaseLen
    }
    return off + UINT32LEN, nil
}

// packText appends the wire format of field to msg.
func packText(msg []byte, field string) ([]byte, error) {
    l := len(field)
    if l > 255 {
        return nil, errStringTooLong
    }
    msg = append(msg, byte(l))
    msg = append(msg, field...)

    return msg, nil
}

func unpackText(msg []byte, off int) (string, int, error) {
    if off >= len(msg) {
        return "", off, errBaseLen
    }
    beginOff := off + 1
    endOff := beginOff + int(msg[off])
    if endOff > len(msg) {
        return "", off, errCalcLen
    }
    return string(msg[beginOff:endOff]), endOff, nil
}

// packBytes appends the wire format of field to msg.
func packBytes(msg []byte, field []byte) []byte {
    return append(msg, field...)
}

func unpackBytes(msg []byte, off int, field []byte) (int, error) {
    newOff := off + len(field)
    if newOff > len(msg) {
        return off, errBaseLen
    }
    copy(field, msg[off:newOff])
    return newOff, nil
}


// An MXResource is an MX Resource record.
type MXResource struct {
    Pref uint16
    MX   Name
}

func (r *MXResource) real_type() Type {
    return TypeMX
}

// pack appends the wire format of the MXResource to msg.
func (r *MXResource) pack(msg []byte, compression map[string]int, compressionOff int) ([]byte, error) {
    oldMsg := msg
    msg = packUint16(msg, r.Pref)
    msg, err := r.MX.pack(msg, compression, compressionOff)
    if err != nil {
        return oldMsg, &nestedError{"MXResource.MX", err}
    }
    return msg, nil
}

// GoString implements fmt.GoStringer.GoString.
func (r *MXResource) GoString() string {
    return "dnsmessage.MXResource{" +
        "Pref: " + printUint16(r.Pref) + ", " +
        "MX: " + r.MX.GoString() + "}"
}

func unpackMXResource(msg []byte, off int) (MXResource, error) {
    pref, off, err := unpackUint16(msg, off)
    if err != nil {
        return MXResource{}, &nestedError{"Pref", err}
    }
    var mx Name
    if _, err := mx.unpack(msg, off); err != nil {
        return MXResource{}, &nestedError{"MX", err}
    }
    return MXResource{pref, mx}, nil
}

// An NSResource is an NS Resource record.
type NSResource struct {
    NS Name
}

func (r *NSResource) real_type() Type {
    return TypeNS
}

// pack appends the wire format of the NSResource to msg.
func (r *NSResource) pack(msg []byte, compression map[string]int, compressionOff int) ([]byte, error) {
    return r.NS.pack(msg, compression, compressionOff)
}

// GoString implements fmt.GoStringer.GoString.
func (r *NSResource) GoString() string {
    return "dnsmessage.NSResource{NS: " + r.NS.GoString() + "}"
}

func unpackNSResource(msg []byte, off int) (NSResource, error) {
    var ns Name
    if _, err := ns.unpack(msg, off); err != nil {
        return NSResource{}, err
    }
    return NSResource{ns}, nil
}

// A PTRResource is a PTR Resource record.
type PTRResource struct {
    PTR Name
}

func (r *PTRResource) real_type() Type {
    return TypePTR
}

// pack appends the wire format of the PTRResource to msg.
func (r *PTRResource) pack(msg []byte, compression map[string]int, compressionOff int) ([]byte, error) {
    return r.PTR.pack(msg, compression, compressionOff)
}

// GoString implements fmt.GoStringer.GoString.
func (r *PTRResource) GoString() string {
    return "dnsmessage.PTRResource{PTR: " + r.PTR.GoString() + "}"
}

func unpackPTRResource(msg []byte, off int) (PTRResource, error) {
    var ptr Name
    if _, err := ptr.unpack(msg, off); err != nil {
        return PTRResource{}, err
    }
    return PTRResource{ptr}, nil
}

// An SOAResource is an SOA Resource record.
type SOAResource struct {
    NS      Name
    MBox    Name
    Serial  uint32
    Refresh uint32
    Retry   uint32
    Expire  uint32

    // MinTTL the is the default TTL of Resources records which did not
    // contain a TTL value and the TTL of negative responses. (RFC 2308
    // Section 4)
    MinTTL uint32
}

func (r *SOAResource) real_type() Type {
    return TypeSOA
}

// pack appends the wire format of the SOAResource to msg.
func (r *SOAResource) pack(msg []byte, compression map[string]int, compressionOff int) ([]byte, error) {
    oldMsg := msg
    msg, err := r.NS.pack(msg, compression, compressionOff)
    if err != nil {
        return oldMsg, &nestedError{"SOAResource.NS", err}
    }
    msg, err = r.MBox.pack(msg, compression, compressionOff)
    if err != nil {
        return oldMsg, &nestedError{"SOAResource.MBox", err}
    }
    msg = packUint32(msg, r.Serial)
    msg = packUint32(msg, r.Refresh)
    msg = packUint32(msg, r.Retry)
    msg = packUint32(msg, r.Expire)
    return packUint32(msg, r.MinTTL), nil
}

// GoString implements fmt.GoStringer.GoString.
func (r *SOAResource) GoString() string {
    return "dnsmessage.SOAResource{" +
        "NS: " + r.NS.GoString() + ", " +
        "MBox: " + r.MBox.GoString() + ", " +
        "Serial: " + printUint32(r.Serial) + ", " +
        "Refresh: " + printUint32(r.Refresh) + ", " +
        "Retry: " + printUint32(r.Retry) + ", " +
        "Expire: " + printUint32(r.Expire) + ", " +
        "MinTTL: " + printUint32(r.MinTTL) + "}"
}

func unpackSOAResource(msg []byte, off int) (SOAResource, error) {
    var ns Name
    off, err := ns.unpack(msg, off)
    if err != nil {
        return SOAResource{}, &nestedError{"NS", err}
    }
    var mbox Name
    if off, err = mbox.unpack(msg, off); err != nil {
        return SOAResource{}, &nestedError{"MBox", err}
    }
    serial, off, err := unpackUint32(msg, off)
    if err != nil {
        return SOAResource{}, &nestedError{"Serial", err}
    }
    refresh, off, err := unpackUint32(msg, off)
    if err != nil {
        return SOAResource{}, &nestedError{"Refresh", err}
    }
    retry, off, err := unpackUint32(msg, off)
    if err != nil {
        return SOAResource{}, &nestedError{"Retry", err}
    }
    expire, off, err := unpackUint32(msg, off)
    if err != nil {
        return SOAResource{}, &nestedError{"Expire", err}
    }
    minTTL, _, err := unpackUint32(msg, off)
    if err != nil {
        return SOAResource{}, &nestedError{"MinTTL", err}
    }
    return SOAResource{ns, mbox, serial, refresh, retry, expire, minTTL}, nil
}

// A TXTResource is a TXT Resource record.
type TXTResource struct {
    TXT []string
}

func (r *TXTResource) real_type() Type {
    return TypeTXT
}

// pack appends the wire format of the TXTResource to msg.
func (r *TXTResource) pack(msg []byte, compression map[string]int, compressionOff int) ([]byte, error) {
    oldMsg := msg
    for _, s := range r.TXT {
        var err error
        msg, err = packText(msg, s)
        if err != nil {
            return oldMsg, err
        }
    }
    return msg, nil
}

// GoString implements fmt.GoStringer.GoString.
func (r *TXTResource) GoString() string {
    s := "dnsmessage.TXTResource{TXT: []string{"
    if len(r.TXT) == 0 {
        return s + "}}"
    }
    s += `"` + printString([]byte(r.TXT[0]))
    for _, t := range r.TXT[1:] {
        s += `", "` + printString([]byte(t))
    }
    return s + `"}}`
}

func unpackTXTResource(msg []byte, off int, length uint16) (TXTResource, error) {
    txts := make([]string, 0, 1)
    for n := uint16(0); n < length; {
        var t string
        var err error
        if t, off, err = unpackText(msg, off); err != nil {
            return TXTResource{}, &nestedError{"text", err}
        }
        // Check if we got too many bytes.
        if length-n < uint16(len(t))+1 {
            return TXTResource{}, errCalcLen
        }
        n += uint16(len(t)) + 1
        txts = append(txts, t)
    }
    return TXTResource{txts}, nil
}

// An SRVResource is an SRV Resource record.
type SRVResource struct {
    Priority uint16
    Weight   uint16
    Port     uint16
    Target   Name // Not compressed as per RFC 2782.
}

func (r *SRVResource) real_type() Type {
    return TypeSRV
}

// pack appends the wire format of the SRVResource to msg.
func (r *SRVResource) pack(msg []byte, compression map[string]int, compressionOff int) ([]byte, error) {
    oldMsg := msg
    msg = packUint16(msg, r.Priority)
    msg = packUint16(msg, r.Weight)
    msg = packUint16(msg, r.Port)
    msg, err := r.Target.pack(msg, nil, compressionOff)
    if err != nil {
        return oldMsg, &nestedError{"SRVResource.Target", err}
    }
    return msg, nil
}

// GoString implements fmt.GoStringer.GoString.
func (r *SRVResource) GoString() string {
    return "dnsmessage.SRVResource{" +
        "Priority: " + printUint16(r.Priority) + ", " +
        "Weight: " + printUint16(r.Weight) + ", " +
        "Port: " + printUint16(r.Port) + ", " +
        "Target: " + r.Target.GoString() + "}"
}

func unpackSRVResource(msg []byte, off int) (SRVResource, error) {
    priority, off, err := unpackUint16(msg, off)
    if err != nil {
        return SRVResource{}, &nestedError{"Priority", err}
    }
    weight, off, err := unpackUint16(msg, off)
    if err != nil {
        return SRVResource{}, &nestedError{"Weight", err}
    }
    port, off, err := unpackUint16(msg, off)
    if err != nil {
        return SRVResource{}, &nestedError{"Port", err}
    }
    var target Name
    if _, err := target.unpackCompressed(msg, off, false /* allowCompression */); err != nil {
        return SRVResource{}, &nestedError{"Target", err}
    }
    return SRVResource{priority, weight, port, target}, nil
}

// An AResource is an A Resource record.
type AResource struct {
    A [4]byte
}

func (r *AResource) real_type() Type {
    return TypeA
}

// pack appends the wire format of the AResource to msg.
func (r *AResource) pack(msg []byte, compression map[string]int, compressionOff int) ([]byte, error) {
    return packBytes(msg, r.A[:]), nil
}

// GoString implements fmt.GoStringer.GoString.
func (r *AResource) GoString() string {
    return "dnsmessage.AResource{" +
        "A: [4]byte{" + printByteSlice(r.A[:]) + "}}"
}

func unpackAResource(msg []byte, off int) (AResource, error) {
    var a [4]byte
    if _, err := unpackBytes(msg, off, a[:]); err != nil {
        return AResource{}, err
    }
    return AResource{a}, nil
}

// An AAAAResource is an AAAA Resource record.
type AAAAResource struct {
    AAAA [16]byte
}

func (r *AAAAResource) real_type() Type {
    return TypeAAAA
}

// GoString implements fmt.GoStringer.GoString.
func (r *AAAAResource) GoString() string {
    return "dnsmessage.AAAAResource{" +
        "AAAA: [16]byte{" + printByteSlice(r.AAAA[:]) + "}}"
}

// pack appends the wire format of the AAAAResource to msg.
func (r *AAAAResource) pack(msg []byte, compression map[string]int, compressionOff int) ([]byte, error) {
    return packBytes(msg, r.AAAA[:]), nil
}

func unpackAAAAResource(msg []byte, off int) (AAAAResource, error) {
    var aaaa [16]byte
    if _, err := unpackBytes(msg, off, aaaa[:]); err != nil {
        return AAAAResource{}, err
    }
    return AAAAResource{aaaa}, nil
}

// An OPTResource is an OPT pseudo Resource record.
//
// The pseudo resource record is part of the extension mechanisms for DNS
// as defined in RFC 6891.
type OPTResource struct {
    Options []Option
}

// An Option represents a DNS message option within OPTResource.
//
// The message option is part of the extension mechanisms for DNS as
// defined in RFC 6891.
type Option struct {
    Code uint16 // option code
    Data []byte
}

// GoString implements fmt.GoStringer.GoString.
func (o *Option) GoString() string {
    return "dnsmessage.Option{" +
        "Code: " + printUint16(o.Code) + ", " +
        "Data: []byte{" + printByteSlice(o.Data) + "}}"
}

func (r *OPTResource) real_type() Type {
    return TypeOPT
}

func (r *OPTResource) pack(msg []byte, compression map[string]int, compressionOff int) ([]byte, error) {
    for _, opt := range r.Options {
        msg = packUint16(msg, opt.Code)
        l := uint16(len(opt.Data))
        msg = packUint16(msg, l)
        msg = packBytes(msg, opt.Data)
    }
    return msg, nil
}

// GoString implements fmt.GoStringer.GoString.
func (r *OPTResource) GoString() string {
    s := "dnsmessage.OPTResource{Options: []dnsmessage.Option{"
    if len(r.Options) == 0 {
        return s + "}}"
    }
    s += r.Options[0].GoString()
    for _, o := range r.Options[1:] {
        s += ", " + o.GoString()
    }
    return s + "}}"
}

func unpackOPTResource(msg []byte, off int, length uint16) (OPTResource, error) {
    var opts []Option
    for oldOff := off; off < oldOff+int(length); {
        var err error
        var o Option
        o.Code, off, err = unpackUint16(msg, off)
        if err != nil {
            return OPTResource{}, &nestedError{"Code", err}
        }
        var l uint16
        l, off, err = unpackUint16(msg, off)
        if err != nil {
            return OPTResource{}, &nestedError{"Data", err}
        }
        o.Data = make([]byte, l)
        if copy(o.Data, msg[off:]) != int(l) {
            return OPTResource{}, &nestedError{"Data", errCalcLen}
        }
        off += int(l)
        opts = append(opts, o)
    }
    return OPTResource{opts}, nil
}
*/
