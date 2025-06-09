use axum::response::IntoResponse;
use axum::{
	extract::{Form, State},
	http::{HeaderMap, StatusCode},
	response::{Html, Redirect, Response},
	routing::{get, post},
	Router,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
	time::{SystemTime, UNIX_EPOCH},
};
use tower::util::ServiceExt;
use uuid::Uuid;

use crate::{
	html::generate_challenge_html,
	jwt::{generate_secret, issue_jwt, validate_jwt},
	values::{
		CHALLENGE_EXPIRY_SECS, COOKIE_NAME, MAX_ATTEMPTS, POW_DIFFICULTY_PREFIX, TOKEN_EXPIRY_SECS,
	},
};

#[derive(Clone)]
pub struct AppState {
	pub jwt_secret: Vec<u8>,
	pub challenges: Arc<Mutex<HashMap<String, Challenge>>>,
}

#[derive(Debug, Clone)]
pub struct Challenge {
	pub token: String,
	pub challenge: String,
	pub created_at: u64,
	pub attempts: u32,
}

#[derive(Deserialize)]
pub struct NonceSubmission {
	nonce: String,
	token: String,
}

impl AppState {
	pub fn new() -> Self {
		Self {
			jwt_secret: generate_secret(),
			challenges: Arc::new(Mutex::new(HashMap::new())),
		}
	}
}

pub fn create_router() -> Router {
	let state = AppState::new();

	Router::new()
		.route("/get_challenge", get(handle_get_challenge))
		.route("/post_nonce", post(handle_post_nonce))
		.route("/validate", get(handle_validate))
		.with_state(state)
}

async fn handle_get_challenge(State(state): State<AppState>) -> Result<Response, StatusCode> {
	let token = Uuid::new_v4().to_string();
	let challenge = Uuid::new_v4().to_string();
	let now = current_timestamp();

	let challenge_data = Challenge {
		token: token.clone(),
		challenge: challenge.clone(),
		created_at: now,
		attempts: 0,
	};

	if let Ok(mut map) = state.challenges.lock() {
		map.insert(token.clone(), challenge_data);
	}

	let html = generate_challenge_html(&token, &challenge, POW_DIFFICULTY_PREFIX.len());
	Ok(Html(html).into_response())
}

async fn handle_post_nonce(
	State(state): State<AppState>,
	Form(submission): Form<NonceSubmission>,
) -> Result<Response, StatusCode> {
	let now = current_timestamp();

	let mut challenges = state
		.challenges
		.lock()
		.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
	let challenge = match challenges.get_mut(&submission.token) {
		Some(c) => c,
		None => return Ok((StatusCode::FORBIDDEN, "No active challenge").into_response()),
	};

	if now - challenge.created_at > CHALLENGE_EXPIRY_SECS {
		challenges.remove(&submission.token);
		return Ok((StatusCode::FORBIDDEN, "Challenge expired").into_response());
	}

	if challenge.attempts >= MAX_ATTEMPTS {
		return Ok((StatusCode::TOO_MANY_REQUESTS, "Too many attempts").into_response());
	}

	challenge.attempts += 1;

	let hash_input = format!("{}{}", challenge.challenge, submission.nonce);
	let hash = Sha256::digest(hash_input.as_bytes());
	let hash_hex = format!("{:x}", hash);

	if !hash_hex.starts_with(POW_DIFFICULTY_PREFIX) {
		return Ok((StatusCode::FORBIDDEN, "Invalid nonce").into_response());
	}

	let jwt_token = issue_jwt("verified_user", &state.jwt_secret)
		.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

	challenges.remove(&submission.token);
	drop(challenges);

	let cookie = format!(
		"{name}={value}; HttpOnly; Secure; SameSite=Strict; Max-Age={max_age}",
		name = COOKIE_NAME,
		value = jwt_token,
		max_age = TOKEN_EXPIRY_SECS
	);

	let mut response = (
		StatusCode::OK,
		"PoW verified, access granted! Redirecting...",
	)
		.into_response();

	response
		.headers_mut()
		.insert("set-cookie", cookie.parse().unwrap());

	response
		.headers_mut()
		.insert("refresh", "2; url=/".parse().unwrap());

	Ok(response)
}

async fn handle_validate(
	headers: HeaderMap,
	State(state): State<AppState>,
) -> Result<Response, StatusCode> {
	if let Some(cookie_str) = headers.get("cookie").and_then(|c| c.to_str().ok()) {
		if let Some(token) = extract_token_from_cookie(cookie_str) {
			if let Ok(_) = validate_jwt(&token, &state.jwt_secret) {
				return Ok((StatusCode::OK, "Access Granted - You are authenticated!").into_response());
			}
		}
	}

	Ok((
		StatusCode::UNAUTHORIZED,
		[("refresh", "0; url=/get_challenge")],
		"Unauthorized. Redirecting to challenge..."
	).into_response())
}

fn extract_token_from_cookie(cookie_str: &str) -> Option<String> {
	cookie_str.split(';').map(str::trim).find_map(|cookie| {
		cookie
			.strip_prefix(&format!("{COOKIE_NAME}="))
			.map(String::from)
	})
}

fn current_timestamp() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default()
		.as_secs()
}

pub async fn start_server() {
	let app = create_router();
	let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
		.await
		.expect("bind failed");
	println!("🚀 Listening on http://0.0.0.0:3000");
	println!("📋 Endpoints:");
	println!("   GET  /get_challenge - Get a new PoW challenge");
	println!("   POST /post_nonce    - Submit nonce solution");
	println!("   GET  /validate      - Check authentication status");
	axum::serve(listener, app).await.expect("server error");
}

#[cfg(test)]
mod tests {
	use super::*;
	use axum::{
		body::Body,
		http::{header, Method, Request},
	};
	use std::collections::HashMap;
	use tower::ServiceExt;

	#[tokio::test]
	async fn test_get_challenge() {
		let app = create_router();

		let request = Request::builder()
			.method(Method::GET)
			.uri("/get_challenge")
			.body(Body::empty())
			.unwrap();

		let response = app.oneshot(request).await.unwrap();

		assert_eq!(response.status(), StatusCode::OK);

		let body = axum::body::to_bytes(response.into_body(), usize::MAX)
			.await
			.unwrap();
		let body_str = String::from_utf8(body.to_vec()).unwrap();

		assert!(body_str.contains("<!DOCTYPE html>"));
		assert!(body_str.contains("Challenge string:"));
	}

	#[tokio::test]
	async fn test_post_nonce_with_invalid_token() {
		let app = create_router();

		let form_data = "nonce=123456&token=invalid_token";

		let request = Request::builder()
			.method(Method::POST)
			.uri("/post_nonce")
			.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
			.body(Body::from(form_data))
			.unwrap();

		let response = app.oneshot(request).await.unwrap();

		assert_eq!(response.status(), StatusCode::FORBIDDEN);

		let body = axum::body::to_bytes(response.into_body(), usize::MAX)
			.await
			.unwrap();
		let body_str = String::from_utf8(body.to_vec()).unwrap();

		assert_eq!(body_str, "No active challenge");
	}

	#[tokio::test]
	async fn test_validate_without_cookie() {
		let app = create_router();

		let request = Request::builder()
			.method(Method::GET)
			.uri("/validate")
			.body(Body::empty())
			.unwrap();

		let response = app.oneshot(request).await.unwrap();

		assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
		assert_eq!(response.headers().get("refresh").unwrap(), "0; url=/get_challenge");
	}

	#[tokio::test]
	async fn test_validate_with_valid_jwt() {
		let state = AppState::new();
		let jwt_token = crate::jwt::issue_jwt("test_user", &state.jwt_secret).unwrap();
		let cookie_value = format!("{}={}", COOKIE_NAME, jwt_token);

		let app = Router::new()
			.route("/validate", get(handle_validate))
			.with_state(state);

		let request = Request::builder()
			.method(Method::GET)
			.uri("/validate")
			.header(header::COOKIE, cookie_value)
			.body(Body::empty())
			.unwrap();

		let response = app.oneshot(request).await.unwrap();

		assert_eq!(response.status(), StatusCode::OK);

		let body = axum::body::to_bytes(response.into_body(), usize::MAX)
			.await
			.unwrap();
		let body_str = String::from_utf8(body.to_vec()).unwrap();

		assert_eq!(body_str, "Access Granted - You are authenticated!");
	}

	// Helper function to find a valid nonce for testing
	fn find_valid_nonce(challenge: &str, difficulty_prefix: &str) -> String {
		for nonce in 0..1000000 {
			let hash_input = format!("{}{}", challenge, nonce);
			let hash = Sha256::digest(hash_input.as_bytes());
			let hash_hex = format!("{:x}", hash);

			if hash_hex.starts_with(difficulty_prefix) {
				return nonce.to_string();
			}
		}
		panic!("Could not find valid nonce within reasonable attempts");
	}

	#[tokio::test]
	async fn test_full_flow() {
		let app = create_router();

		// Step 1: Get challenge
		let request = Request::builder()
			.method(Method::GET)
			.uri("/get_challenge")
			.body(Body::empty())
			.unwrap();

		let response = app.clone().oneshot(request).await.unwrap();
		assert_eq!(response.status(), StatusCode::OK);

		// Step 2: Try to validate without token (should fail)
		let request = Request::builder()
			.method(Method::GET)
			.uri("/validate")
			.body(Body::empty())
			.unwrap();

		let response = app.oneshot(request).await.unwrap();
		assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
	}

	#[tokio::test]
	async fn test_post_nonce_with_valid_solution() {
		let state = AppState::new();
		let token = "test_token";
		let challenge = "test_challenge";

		let valid_challenge = Challenge {
			token: token.to_string(),
			challenge: challenge.to_string(),
			created_at: current_timestamp(),
			attempts: 0,
		};

		{
			let mut challenges = state.challenges.lock().unwrap();
			challenges.insert(token.to_string(), valid_challenge);
		}

		let valid_nonce = find_valid_nonce(challenge, POW_DIFFICULTY_PREFIX);

		let app = Router::new()
			.route("/post_nonce", post(handle_post_nonce))
			.with_state(state);

		let form_data = format!("nonce={}&token={}", valid_nonce, token);

		let request = Request::builder()
			.method(Method::POST)
			.uri("/post_nonce")
			.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
			.body(Body::from(form_data))
			.unwrap();

		let response = app.oneshot(request).await.unwrap();

		assert_eq!(response.status(), StatusCode::OK);

		let set_cookie_header = response.headers().get("set-cookie");
		assert!(set_cookie_header.is_some());
		let cookie_str = set_cookie_header.unwrap().to_str().unwrap();
		assert!(cookie_str.contains(COOKIE_NAME));

		let refresh_header = response.headers().get("refresh");
		assert!(refresh_header.is_some());
		assert_eq!(refresh_header.unwrap().to_str().unwrap(), "2; url=/validate");
	}
}
