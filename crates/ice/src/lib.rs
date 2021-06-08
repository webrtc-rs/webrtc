#![warn(rust_2018_idioms)]
#![cfg_attr(not(test), warn(clippy::pedantic, clippy::nursery))]
#![cfg_attr(
    not(test),
    allow(
        clippy::case_sensitive_file_extension_comparisons,
        clippy::let_underscore_drop,
        clippy::missing_errors_doc,
        clippy::missing_panics_doc,
        clippy::module_name_repetitions,
        clippy::similar_names,
        clippy::too_many_lines,
        clippy::wildcard_imports,
    )
)]
#![allow(dead_code)]

#[macro_use]
extern crate lazy_static;

pub mod agent;
pub mod candidate;
pub mod control;
pub mod errors;
pub mod external_ip_mapper;
pub mod mdns;
pub mod network_type;
pub mod priority;
mod rand;
pub mod state;
pub mod stats;
pub mod tcp_type;
pub mod url;
pub mod use_candidate;
mod util;
