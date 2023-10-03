use std::collections::HashMap;

use super::header::*;
use super::question::*;
use super::resource::*;
use super::*;
use crate::error::*;

// A Builder allows incrementally packing a DNS message.
//
// Example usage:
//	b := NewBuilder(Header{...})
//	b.enable_compression()
//	// Optionally start a section and add things to that section.
//	// Repeat adding sections as necessary.
//	buf, err := b.Finish()
//	// If err is nil, buf[2:] will contain the built bytes.
#[derive(Default)]
pub struct Builder {
    // msg is the storage for the message being built.
    pub msg: Option<Vec<u8>>,

    // section keeps track of the current section being built.
    pub section: Section,

    // header keeps track of what should go in the header when Finish is
    // called.
    pub header: HeaderInternal,

    // start is the starting index of the bytes allocated in msg for header.
    pub start: usize,

    // compression is a mapping from name suffixes to their starting index
    // in msg.
    pub compression: Option<HashMap<String, usize>>,
}

impl Builder {
    // NewBuilder creates a new builder with compression disabled.
    //
    // Note: Most users will want to immediately enable compression with the
    // enable_compression method. See that method's comment for why you may or may
    // not want to enable compression.
    //
    // The DNS message is appended to the provided initial buffer buf (which may be
    // nil) as it is built. The final message is returned by the (*Builder).Finish
    // method, which may return the same underlying array if there was sufficient
    // capacity in the slice.
    pub fn new(h: &Header) -> Self {
        let (id, bits) = h.pack();

        Builder {
            msg: Some(vec![0; HEADER_LEN]),
            start: 0,
            section: Section::Header,
            header: HeaderInternal {
                id,
                bits,
                ..Default::default()
            },
            compression: None,
        }

        //var hb [HEADER_LEN]byte
        //b.msg = append(b.msg, hb[:]...)
        //return b
    }

    // enable_compression enables compression in the Builder.
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
    pub fn enable_compression(&mut self) {
        self.compression = Some(HashMap::new());
    }

    fn start_check(&self, section: Section) -> Result<()> {
        if self.section <= Section::NotStarted {
            return Err(Error::ErrNotStarted);
        }
        if self.section > section {
            return Err(Error::ErrSectionDone);
        }

        Ok(())
    }

    // start_questions prepares the builder for packing Questions.
    pub fn start_questions(&mut self) -> Result<()> {
        self.start_check(Section::Questions)?;
        self.section = Section::Questions;
        Ok(())
    }

    // start_answers prepares the builder for packing Answers.
    pub fn start_answers(&mut self) -> Result<()> {
        self.start_check(Section::Answers)?;
        self.section = Section::Answers;
        Ok(())
    }

    // start_authorities prepares the builder for packing Authorities.
    pub fn start_authorities(&mut self) -> Result<()> {
        self.start_check(Section::Authorities)?;
        self.section = Section::Authorities;
        Ok(())
    }

    // start_additionals prepares the builder for packing Additionals.
    pub fn start_additionals(&mut self) -> Result<()> {
        self.start_check(Section::Additionals)?;
        self.section = Section::Additionals;
        Ok(())
    }

    fn increment_section_count(&mut self) -> Result<()> {
        let section = self.section;
        let (count, err) = match section {
            Section::Questions => (&mut self.header.questions, Error::ErrTooManyQuestions),
            Section::Answers => (&mut self.header.answers, Error::ErrTooManyAnswers),
            Section::Authorities => (&mut self.header.authorities, Error::ErrTooManyAuthorities),
            Section::Additionals => (&mut self.header.additionals, Error::ErrTooManyAdditionals),
            Section::NotStarted => return Err(Error::ErrNotStarted),
            Section::Done => return Err(Error::ErrSectionDone),
            Section::Header => return Err(Error::ErrSectionHeader),
        };

        if *count == u16::MAX {
            Err(err)
        } else {
            *count += 1;
            Ok(())
        }
    }

    // question adds a single question.
    pub fn add_question(&mut self, q: &Question) -> Result<()> {
        if self.section < Section::Questions {
            return Err(Error::ErrNotStarted);
        }
        if self.section > Section::Questions {
            return Err(Error::ErrSectionDone);
        }
        let msg = self.msg.take();
        if let Some(mut msg) = msg {
            msg = q.pack(msg, &mut self.compression, self.start)?;
            self.increment_section_count()?;
            self.msg = Some(msg);
        }

        Ok(())
    }

    fn check_resource_section(&self) -> Result<()> {
        if self.section < Section::Answers {
            return Err(Error::ErrNotStarted);
        }
        if self.section > Section::Additionals {
            return Err(Error::ErrSectionDone);
        }
        Ok(())
    }

    // Resource adds a single resource.
    pub fn add_resource(&mut self, r: &mut Resource) -> Result<()> {
        self.check_resource_section()?;

        if let Some(body) = &r.body {
            r.header.typ = body.real_type();
        } else {
            return Err(Error::ErrNilResourceBody);
        }

        if let Some(msg) = self.msg.take() {
            let (mut msg, len_off) = r.header.pack(msg, &mut self.compression, self.start)?;
            let pre_len = msg.len();
            if let Some(body) = &r.body {
                msg = body.pack(msg, &mut self.compression, self.start)?;
                r.header.fix_len(&mut msg, len_off, pre_len)?;
                self.increment_section_count()?;
            }
            self.msg = Some(msg);
        }

        Ok(())
    }

    // Finish ends message building and generates a binary message.
    pub fn finish(&mut self) -> Result<Vec<u8>> {
        if self.section < Section::Header {
            return Err(Error::ErrNotStarted);
        }
        self.section = Section::Done;

        // Space for the header was allocated in NewBuilder.
        let buf = self.header.pack(vec![]);
        assert_eq!(buf.len(), HEADER_LEN);
        if let Some(mut msg) = self.msg.take() {
            msg[..HEADER_LEN].copy_from_slice(&buf[..HEADER_LEN]);
            Ok(msg)
        } else {
            Err(Error::ErrEmptyBuilderMsg)
        }
    }
}
