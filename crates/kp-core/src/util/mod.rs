//! Small helpers shared by several modules and crates.
//!
//! Only what more than one place needs lives here; a helper used once stays
//! next to its caller.

mod json;
mod limits;
mod text;
mod time;
mod tree;

pub use json::unescape_json_string;
pub use limits::{MAX_CIPHERTEXT_BYTES, MAX_CONTENT_BYTES};
pub use text::{clean_line, has_control_characters, is_clean_line};
pub use time::{unix_millis, unix_now};
pub use tree::{Ancestry, ancestry};
