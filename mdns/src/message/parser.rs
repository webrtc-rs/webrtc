use crate::error::*;
use crate::message::header::{Header, HeaderInternal, Section};
use crate::message::name::Name;
use crate::message::question::Question;
use crate::message::resource::{unpack_resource_body, Resource, ResourceBody, ResourceHeader};
use crate::message::{DnsClass, DnsType};

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
#[derive(Default)]
pub struct Parser<'a> {
    pub msg: &'a [u8],
    pub header: HeaderInternal,

    pub section: Section,
    pub off: usize,
    pub index: usize,
    pub res_header_valid: bool,
    pub res_header: ResourceHeader,
}

impl<'a> Parser<'a> {
    // start parses the header and enables the parsing of Questions.
    pub fn start(&mut self, msg: &'a [u8]) -> Result<Header> {
        *self = Parser {
            msg,
            ..Default::default()
        };
        self.off = self.header.unpack(msg, 0)?;
        self.section = Section::Questions;
        Ok(self.header.header())
    }

    fn check_advance(&mut self, sec: Section) -> Result<()> {
        if self.section < sec {
            return Err(Error::ErrNotStarted);
        }
        if self.section > sec {
            return Err(Error::ErrSectionDone);
        }
        self.res_header_valid = false;
        if self.index == self.header.count(sec) as usize {
            self.index = 0;
            self.section = Section::from(1 + self.section as u8);
            return Err(Error::ErrSectionDone);
        }
        Ok(())
    }

    fn resource(&mut self, sec: Section) -> Result<Resource> {
        let header = self.resource_header(sec)?;
        self.res_header_valid = false;
        let (body, off) =
            unpack_resource_body(header.typ, self.msg, self.off, header.length as usize)?;
        self.off = off;
        self.index += 1;
        Ok(Resource {
            header,
            body: Some(body),
        })
    }

    fn resource_header(&mut self, sec: Section) -> Result<ResourceHeader> {
        if self.res_header_valid {
            return Ok(self.res_header.clone());
        }
        self.check_advance(sec)?;
        let mut hdr = ResourceHeader::default();
        let off = hdr.unpack(self.msg, self.off, 0)?;

        self.res_header_valid = true;
        self.res_header = hdr.clone();
        self.off = off;
        Ok(hdr)
    }

    fn skip_resource(&mut self, sec: Section) -> Result<()> {
        if self.res_header_valid {
            let new_off = self.off + self.res_header.length as usize;
            if new_off > self.msg.len() {
                return Err(Error::ErrResourceLen);
            }
            self.off = new_off;
            self.res_header_valid = false;
            self.index += 1;
            return Ok(());
        }
        self.check_advance(sec)?;

        self.off = Resource::skip(self.msg, self.off)?;
        self.index += 1;
        Ok(())
    }

    // question parses a single question.
    pub fn question(&mut self) -> Result<Question> {
        self.check_advance(Section::Questions)?;
        let mut name = Name::new("")?;
        let mut off = name.unpack(self.msg, self.off)?;
        let mut typ = DnsType::Unsupported;
        off = typ.unpack(self.msg, off)?;
        let mut class = DnsClass::default();
        off = class.unpack(self.msg, off)?;
        self.off = off;
        self.index += 1;
        Ok(Question { name, typ, class })
    }

    // all_questions parses all Questions.
    pub fn all_questions(&mut self) -> Result<Vec<Question>> {
        // Multiple questions are valid according to the spec,
        // but servers don't actually support them. There will
        // be at most one question here.
        //
        // Do not pre-allocate based on info in self.header, since
        // the data is untrusted.
        let mut qs = vec![];
        loop {
            match self.question() {
                Err(err) => {
                    if Error::ErrSectionDone == err {
                        return Ok(qs);
                    } else {
                        return Err(err);
                    }
                }
                Ok(q) => qs.push(q),
            }
        }
    }

    // skip_question skips a single question.
    pub fn skip_question(&mut self) -> Result<()> {
        self.check_advance(Section::Questions)?;
        let mut off = Name::skip(self.msg, self.off)?;
        off = DnsType::skip(self.msg, off)?;
        off = DnsClass::skip(self.msg, off)?;
        self.off = off;
        self.index += 1;
        Ok(())
    }

    // skip_all_questions skips all Questions.
    pub fn skip_all_questions(&mut self) -> Result<()> {
        loop {
            if let Err(err) = self.skip_question() {
                if Error::ErrSectionDone == err {
                    return Ok(());
                } else {
                    return Err(err);
                }
            }
        }
    }

    // answer_header parses a single answer ResourceHeader.
    pub fn answer_header(&mut self) -> Result<ResourceHeader> {
        self.resource_header(Section::Answers)
    }

    // answer parses a single answer Resource.
    pub fn answer(&mut self) -> Result<Resource> {
        self.resource(Section::Answers)
    }

    // all_answers parses all answer Resources.
    pub fn all_answers(&mut self) -> Result<Vec<Resource>> {
        // The most common query is for A/AAAA, which usually returns
        // a handful of IPs.
        //
        // Pre-allocate up to a certain limit, since self.header is
        // untrusted data.
        let mut n = self.header.answers as usize;
        if n > 20 {
            n = 20
        }
        let mut a = Vec::with_capacity(n);
        loop {
            match self.answer() {
                Err(err) => {
                    if Error::ErrSectionDone == err {
                        return Ok(a);
                    } else {
                        return Err(err);
                    }
                }
                Ok(r) => a.push(r),
            }
        }
    }

    // skip_answer skips a single answer Resource.
    pub fn skip_answer(&mut self) -> Result<()> {
        self.skip_resource(Section::Answers)
    }

    // skip_all_answers skips all answer Resources.
    pub fn skip_all_answers(&mut self) -> Result<()> {
        loop {
            if let Err(err) = self.skip_answer() {
                if Error::ErrSectionDone == err {
                    return Ok(());
                } else {
                    return Err(err);
                }
            }
        }
    }

    // authority_header parses a single authority ResourceHeader.
    pub fn authority_header(&mut self) -> Result<ResourceHeader> {
        self.resource_header(Section::Authorities)
    }

    // authority parses a single authority Resource.
    pub fn authority(&mut self) -> Result<Resource> {
        self.resource(Section::Authorities)
    }

    // all_authorities parses all authority Resources.
    pub fn all_authorities(&mut self) -> Result<Vec<Resource>> {
        // Authorities contains SOA in case of NXDOMAIN and friends,
        // otherwise it is empty.
        //
        // Pre-allocate up to a certain limit, since self.header is
        // untrusted data.
        let mut n = self.header.authorities as usize;
        if n > 10 {
            n = 10;
        }
        let mut a = Vec::with_capacity(n);
        loop {
            match self.authority() {
                Err(err) => {
                    if Error::ErrSectionDone == err {
                        return Ok(a);
                    } else {
                        return Err(err);
                    }
                }
                Ok(r) => a.push(r),
            }
        }
    }

    // skip_authority skips a single authority Resource.
    pub fn skip_authority(&mut self) -> Result<()> {
        self.skip_resource(Section::Authorities)
    }

    // skip_all_authorities skips all authority Resources.
    pub fn skip_all_authorities(&mut self) -> Result<()> {
        loop {
            if let Err(err) = self.skip_authority() {
                if Error::ErrSectionDone == err {
                    return Ok(());
                } else {
                    return Err(err);
                }
            }
        }
    }

    // additional_header parses a single additional ResourceHeader.
    pub fn additional_header(&mut self) -> Result<ResourceHeader> {
        self.resource_header(Section::Additionals)
    }

    // additional parses a single additional Resource.
    pub fn additional(&mut self) -> Result<Resource> {
        self.resource(Section::Additionals)
    }

    // all_additionals parses all additional Resources.
    pub fn all_additionals(&mut self) -> Result<Vec<Resource>> {
        // Additionals usually contain OPT, and sometimes A/AAAA
        // glue records.
        //
        // Pre-allocate up to a certain limit, since self.header is
        // untrusted data.
        let mut n = self.header.additionals as usize;
        if n > 10 {
            n = 10;
        }
        let mut a = Vec::with_capacity(n);
        loop {
            match self.additional() {
                Err(err) => {
                    if Error::ErrSectionDone == err {
                        return Ok(a);
                    } else {
                        return Err(err);
                    }
                }
                Ok(r) => a.push(r),
            }
        }
    }

    // skip_additional skips a single additional Resource.
    pub fn skip_additional(&mut self) -> Result<()> {
        self.skip_resource(Section::Additionals)
    }

    // skip_all_additionals skips all additional Resources.
    pub fn skip_all_additionals(&mut self) -> Result<()> {
        loop {
            if let Err(err) = self.skip_additional() {
                if Error::ErrSectionDone == err {
                    return Ok(());
                } else {
                    return Err(err);
                }
            }
        }
    }

    // resource_body parses a single resource_boy.
    //
    // One of the XXXHeader methods must have been called before calling this
    // method.
    pub fn resource_body(&mut self) -> Result<Box<dyn ResourceBody>> {
        if !self.res_header_valid {
            return Err(Error::ErrNotStarted);
        }
        let (rb, _off) = unpack_resource_body(
            self.res_header.typ,
            self.msg,
            self.off,
            self.res_header.length as usize,
        )?;
        self.off += self.res_header.length as usize;
        self.res_header_valid = false;
        self.index += 1;
        Ok(rb)
    }
}
