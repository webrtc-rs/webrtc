use std::io;

use utils::Error;

use super::session_description::SessionDescription;

pub const END_LINE: &'static str = "\r\n";
pub const ATTRIBUTE_KEY: &'static str = "a=";

pub struct Lexer<'a, R: io::BufRead> {
    pub desc: SessionDescription,
    pub reader: &'a mut R,
}

pub struct StateFn<'a, R: io::BufRead> {
    pub f: fn(&mut Lexer<'a, R>) -> Result<Option<StateFn<'a, R>>, Error>,
}

pub fn read_type<R: io::BufRead>(reader: &mut R) -> Result<String, Error> {
    let mut buf = vec![];
    reader.read_until(b'=', &mut buf)?;
    if buf.len() != 2 {
        return Err(Error::new(format!("SyntaxError: {:?}", buf)));
    }
    let key = String::from_utf8(buf)?;
    Ok(key)
}

pub fn read_value<R: io::BufRead>(reader: &mut R) -> Result<String, Error> {
    let mut line = vec![];
    reader.read_until(b'\n', &mut line)?;
    let value = String::from_utf8(line)?;
    Ok(value.trim().to_string())
}

pub fn index_of(element: &str, data: &[&str]) -> i32 {
    for (k, &v) in data.iter().enumerate() {
        if element == v {
            return k as i32;
        }
    }
    return -1;
}

pub fn key_value_build(key: &str, value: Option<&String>) -> String {
    if let Some(val) = value {
        format!("{}{}{}", key, val, END_LINE)
    } else {
        "".to_string()
    }
}
