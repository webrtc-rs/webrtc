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

pub fn read_type<R: io::BufRead>(reader: &mut R) -> Result<(String, usize), Error> {
    let mut buf = vec![];
    let num_bytes = reader.read_until(b'=', &mut buf)?;
    if num_bytes == 0 {
        return Ok(("".to_owned(), num_bytes));
    }

    if buf.len() != 2 {
        return Err(Error::new(format!("SyntaxError: {:?}", buf)));
    }
    let key = String::from_utf8(buf)?;
    //print!("{}", key);
    Ok((key, num_bytes))
}

pub fn read_value<R: io::BufRead>(reader: &mut R) -> Result<(String, usize), Error> {
    let mut line = vec![];
    let num_bytes = reader.read_until(b'\n', &mut line)?;
    if num_bytes == 0 {
        return Ok(("".to_owned(), num_bytes));
    }

    let value = String::from_utf8(line)?;
    //print!("{}", value);
    Ok((value.trim().to_string(), num_bytes))
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
