pub const COOKIE_NAME: &str = "mpow_token";
pub const TOKEN_BYTE_LENGTH: usize = 24;
pub const TOKEN_EXPIRY_SECS: u64 = 36 * 3600;
pub const CHALLENGE_EXPIRY_SECS: u64 = 300;
pub const POW_DIFFICULTY: usize = 4;
pub const MAX_ATTEMPTS: u32 = 15;
pub const POW_DIFFICULTY_PREFIX: &str = "0000";
pub const MAX_NONCE_LENGTH: usize = 128;
pub const MAX_CHALLENGE_ATTEMPTS: usize = 15;
pub const USE_LOKI: bool = false;

pub use time::Duration;

/// Debug helper
pub fn demo_values() {
	println!("values module demo called");
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn constants_exist() {
		let _ = COOKIE_NAME;
		let _ = TOKEN_BYTE_LENGTH;
		let _ = TOKEN_EXPIRY_SECS;
		let _ = CHALLENGE_EXPIRY_SECS;
		let _ = POW_DIFFICULTY_PREFIX;
		let _ = MAX_NONCE_LENGTH;
		let _ = MAX_CHALLENGE_ATTEMPTS;
		let _ = USE_LOKI;
		let _ = POW_DIFFICULTY;
		let _ = MAX_ATTEMPTS;
	}

	#[test]
	fn demo_function_exists_values() {
		demo_values();
	}
}
