pub mod cli;
pub mod error;
pub mod file_path;
pub mod logging;
pub mod result;
pub mod shared_string;
pub mod timestamp;

pub use parking_lot::{Mutex, RwLock};

pub fn unansi(string: &str) -> String {
    anstream::adapter::strip_str(string).to_string()
}
