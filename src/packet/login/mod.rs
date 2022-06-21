mod login_start;

pub use login_start::LoginStart;

use super::identify_packets;

identify_packets!(
    LoginStart = 0
);