use jsonwebtoken::{decode, encode, errors::Error, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::values::TOKEN_EXPIRY_SECS;

/// JWT Claims structure: subject and expiration
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
	pub sub: String,
	pub exp: usize,
}

/// Generates a 256-bit (32 bytes) secure random secret key
///
/// # Returns
/// A `Vec<u8>` containing the secret key bytes
pub fn generate_secret() -> Vec<u8> {
	const KEY_SIZE_BYTES: usize = 32;
	let mut secret_bytes: Vec<u8> = vec![0u8; KEY_SIZE_BYTES];
	rand::thread_rng().fill_bytes(&mut secret_bytes);
	return secret_bytes;
}

/// Issues a JWT containing a subject and expiration
///
/// # Arguments
/// * `subject` - identifier for the subject
/// * `secret_key` - HMAC secret key bytes
///
/// # Returns
/// `Ok(String)` containing the encoded JWT or `Err(String)` on failure
pub fn issue_jwt(subject: &str, secret_key: &[u8]) -> Result<String, String> {
	let now = SystemTime::now();
	let expiration = match now
		.checked_add(Duration::from_secs(TOKEN_EXPIRY_SECS))
		.and_then(|ts| ts.duration_since(UNIX_EPOCH).ok())
	{
		Some(duration) => duration.as_secs() as usize,
		None => return Err(String::from("JWT time calculation failed")),
	};

	let claims: Claims = Claims {
		sub: subject.to_owned(),
		exp: expiration,
	};

	match encode(
		&Header::default(),
		&claims,
		&EncodingKey::from_secret(secret_key),
	) {
		Ok(token) => Ok(token),
		Err(e) => Err(format!("JWT encoding failed: {}", e)),
	}
}

/// Validates a JWT and extracts its claims
///
/// # Arguments
/// * `token` - JWT string
/// * `secret_key` - HMAC secret key bytes
///
/// # Returns
/// `Ok(Claims)` if valid or `Err(Error)` from the jsonwebtoken crate
pub fn validate_jwt(token: &str, secret_key: &[u8]) -> Result<Claims, Error> {
	let validator: Validation = Validation::default();
	let decoded = decode::<Claims>(token, &DecodingKey::from_secret(secret_key), &validator)?;
	return Ok(decoded.claims);
}

/// Generates an expired token (used for testing)
fn create_expired_token(subject: &str, secret: &[u8]) -> String {
	let claims: Claims = Claims {
		sub: subject.to_owned(),
		exp: 0, // Epoch start time (always expired)
	};
	encode(
		&Header::default(),
		&claims,
		&EncodingKey::from_secret(secret),
	)
	.expect("Failed to encode expired test token")
}

#[cfg(test)]
mod tests {
	use super::*;
	use jsonwebtoken::errors::ErrorKind;

	#[test]
	fn test_issue_and_validate() {
		let secret: Vec<u8> = generate_secret();
		let subject: &str = "test_user";

		let token: String = issue_jwt(subject, &secret).expect("JWT issuance should succeed");

		let claims: Claims = validate_jwt(&token, &secret).expect("JWT validation should succeed");

		assert_eq!(claims.sub, subject);

		let now: usize = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_secs() as usize;
		assert!(claims.exp > now, "Token expiry should be in the future");
	}

	#[test]
	fn test_expired_token() {
		let secret = generate_secret();
		let token = create_expired_token("expired_user", &secret);
		let err = validate_jwt(&token, &secret).expect_err("Should fail on expired token");

		match *err.kind() {
			ErrorKind::ExpiredSignature => (),
			_ => panic!("Expected ExpiredSignature, got {:?}", err),
		}
	}

	#[test]
	fn test_invalid_signature() {
		let secret1 = generate_secret();
		let secret2 = generate_secret();
		let subject = "user_signature";

		let token = issue_jwt(subject, &secret1).unwrap();
		let err = validate_jwt(&token, &secret2).expect_err("Should fail with wrong signature");

		match *err.kind() {
			ErrorKind::InvalidSignature => (),
			_ => panic!("Expected InvalidSignature, got {:?}", err),
		}
	}
	#[test]
	fn demo_function_exists_jwt() {
		demo_jwt();
	}
}

/// Debug helper
pub fn demo_jwt() {
	println!("jwt module demo called");
}
