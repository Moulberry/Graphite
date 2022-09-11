mod service;
pub use service::*;

mod fake_player;
pub use fake_player::*;

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        eprintln!("[{}:{}] {}", file!(), line!(), format!($($arg)*));
    }};
}
