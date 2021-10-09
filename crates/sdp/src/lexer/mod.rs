use super::description::session::SessionDescription;
use super::error::{Error, Result};

use std::io;
use std::io::SeekFrom;

pub(crate) const END_LINE: &str = "\r\n";

pub struct Lexer<'a, R: io::BufRead + io::Seek> {
    pub desc: SessionDescription,
    pub reader: &'a mut R,
}

pub type StateFnType<'a, R> = fn(&mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>>;

pub struct StateFn<'a, R: io::BufRead + io::Seek> {
    pub f: StateFnType<'a, R>,
}

pub fn read_type<R: io::BufRead + io::Seek>(reader: &mut R) -> Result<(String, usize)> {
    loop {
        let mut b = [0; 1];
        if reader.read_exact(&mut b).is_err() {
            return Ok(("".to_owned(), 0));
        }

        if b[0] == b'\n' || b[0] == b'\r' {
            continue;
        }
        reader.seek(SeekFrom::Current(-1))?;

        let mut buf = vec![];
        let num_bytes = reader.read_until(b'=', &mut buf)?;
        if num_bytes == 0 {
            return Ok(("".to_owned(), num_bytes));
        }

        let key = String::from_utf8(buf)?;
        match key.len() {
            2 => return Ok((key, num_bytes)),
            _ => return Err(Error::SdpInvalidSyntax(format!("{:?}", key))),
        }
    }
}

pub fn read_value<R: io::BufRead + io::Seek>(reader: &mut R) -> Result<(String, usize)> {
    let mut value = String::new();
    let num_bytes = reader.read_line(&mut value)?;
    Ok((value.trim().to_string(), num_bytes))
}

pub fn index_of(element: &str, data: &[&str]) -> i32 {
    for (k, &v) in data.iter().enumerate() {
        if element == v {
            return k as i32;
        }
    }
    -1
}

pub fn key_value_build(key: &str, value: Option<&String>) -> String {
    if let Some(val) = value {
        format!("{}{}{}", key, val, END_LINE)
    } else {
        "".to_string()
    }
}
