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
