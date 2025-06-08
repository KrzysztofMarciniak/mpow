use time::Duration;

pub const COOKIE_NAME: &str = "mpow_token";
pub const TOKEN_BYTE_LENGTH: usize = 24;
pub const TOKEN_EXPIRY_SECS: i64 = 36 * 3600;
pub const CHALLENGE_EXPIRY_SECS: i64 = 300;
pub const POW_DIFFICULTY_PREFIX: &str = "0000";
pub const MAX_NONCE_LENGTH: usize = 128;
pub const MAX_CHALLENGE_ATTEMPTS: usize = 15;
pub const USE_LOKI: bool = false;

pub use time::Duration;
