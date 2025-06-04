use actix_web::{
    cookie::Cookie,
    dev::{forward_ready, ServiceRequest, ServiceResponse, Transform},
    guard, web, App, Error, HttpMessage, HttpRequest, HttpResponse, HttpServer, Responder,
};
use futures_util::future::{LocalBoxFuture, ready, Ready};
use rand::{RngCore};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    rc::Rc,
};
use tokio::sync::Mutex;
use time::{Duration, OffsetDateTime};
use tracing::{error, info, warn};
use std::sync::Arc;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

mod html; // html template rendering

const COOKIE_NAME: &str = "mpow_token";
const TOKEN_BYTE_LENGTH: usize = 24;           // 24 raw bytes → base64‐URL string
const TOKEN_EXPIRY_SECS: i64 = 36 * 3600;      // 36h token validity after PoW success
const CHALLENGE_EXPIRY_SECS: i64 = 300;        // 5 minutes to solve each challenge
const POW_DIFFICULTY_PREFIX: &str = "0000";    // e.g. hash must start with “0000”
const MAX_NONCE_LENGTH: usize = 128;           // Reject nonces that are too long
const MAX_CHALLENGE_ATTEMPTS: usize = 15;      // Rate limit: attempts per challenge

#[derive(Debug)]
struct Challenge {
    challenge: String,
    created_at: OffsetDateTime,
    attempts: usize,
}

#[derive(Debug)]
struct TokenInfo {
    issued_at: OffsetDateTime,
    valid: bool,
}

pub struct AppState {
    challenges: Arc<Mutex<HashMap<String, Challenge>>>,
    tokens: Arc<Mutex<HashMap<String, TokenInfo>>>,
}

impl AppState {
    fn new() -> Self {
        AppState {
            challenges: Arc::new(Mutex::new(HashMap::new())),
            tokens: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[derive(Clone)]
pub struct SanitizedCookie {
    pub token: String,
    pub is_new_token: bool,
    pub set_cookie: Option<Cookie<'static>>,
}

fn generate_secure_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTE_LENGTH];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(&bytes)
}

async fn cleanup_expired(state: &web::Data<AppState>) {
    let now = OffsetDateTime::now_utc();

    {
        let mut challenges = state.challenges.lock().await;
        challenges.retain(|_, ch| {
            ch.created_at + Duration::seconds(CHALLENGE_EXPIRY_SECS) > now
        });
    }

    {
        let mut tokens = state.tokens.lock().await;
        tokens.retain(|_, ti| {
            ti.issued_at + Duration::seconds(TOKEN_EXPIRY_SECS) > now
        });
    }
}

async fn issue_new_token(state: &web::Data<AppState>) -> (String, Cookie<'static>) {
    let token = generate_secure_token();
    let now = OffsetDateTime::now_utc();

    {
        let mut tokens_map = state.tokens.lock().await;
        tokens_map.insert(
            token.clone(),
            TokenInfo {
                issued_at: now,
                valid: false,
            },
        );
    }

    let cookie = Cookie::build(COOKIE_NAME, token.clone())
        .path("/")
        .max_age(Duration::seconds(TOKEN_EXPIRY_SECS))
        .http_only(true)
        .secure(true)
        .same_site(actix_web::cookie::SameSite::Lax)
        .finish();

    (token, cookie)
}


pub struct CookieSanitizer;

impl<S, B> Transform<S, ServiceRequest> for CookieSanitizer
where
    S: actix_web::dev::Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>
        + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = CookieSanitizerMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(CookieSanitizerMiddleware {
            service: Rc::new(service),
        }))
    }
}

pub struct CookieSanitizerMiddleware<S> {
    service: Rc<S>,
}

impl<S, B> actix_web::dev::Service<ServiceRequest> for CookieSanitizerMiddleware<S>
where
    S: actix_web::dev::Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>
        + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let app_data_opt: Option<web::Data<AppState>> = req.app_data::<web::Data<AppState>>().cloned();
        let service_clone = self.service.clone();

        Box::pin(async move {
            // Default fallback if state is missing 
            let (token, is_new_token, set_cookie) = if let Some(state) = app_data_opt.clone() {
                // Check for existing cookie:
                if let Some(cookie) = req.cookie(COOKIE_NAME) {
                    let val = cookie.value().to_string();
                    let mut tokens_map = state.tokens.lock().await;
                    if let Some(info) = tokens_map.get_mut(&val) {
                        // If token exists, check expiry:
                        let now = OffsetDateTime::now_utc();
                        if info.issued_at + Duration::seconds(TOKEN_EXPIRY_SECS) > now {
                            // Valid (but maybe not yet marked valid by PoW)
                            (val.clone(), false, None)
                        } else {
                            // Expired → remove and issue new
                            tokens_map.remove(&val);
                            drop(tokens_map);
                            let (new_t, cookie) = issue_new_token(&state).await;
                            (new_t, true, Some(cookie))
                        }
                    } else {
                        // Unrecognized token → issue new
                        drop(tokens_map);
                        let (new_t, cookie) = issue_new_token(&state).await;
                        (new_t, true, Some(cookie))
                    }
                } else {
                    // No cookie → issue new
                    let (new_t, cookie) = issue_new_token(&state).await;
                    (new_t, true, Some(cookie))
                }
            } else {
                // No AppState provided, generate a dummy token (should not occur)
                let dummy = generate_secure_token();
                (dummy, true, None)
            };

            // Store in request extensions:
            req.extensions_mut().insert(SanitizedCookie {
                token: token.clone(),
                is_new_token,
                set_cookie: set_cookie.clone(),
            });

            // Call the next service in the chain
            let mut res = service_clone.call(req).await?;

            // If we issued a new cookie, attach it to response
            if let Some(cookie) = set_cookie {
                res.response_mut().add_cookie(&cookie)?;
            }

            Ok(res)
        })
    }
}

/// GET handler: show PoW challenge if needed, otherwise redirect.
pub async fn get_handler(
    data: web::Data<AppState>,
    req: HttpRequest,
) -> impl Responder {
    // First, run cleanup for expired challenges/tokens
    cleanup_expired(&data).await;

    // Extract sanitized cookie info
    let extensions = req.extensions();
    let sc = match extensions.get::<SanitizedCookie>() {
        Some(sc) => sc,
        None => {
            error!("CookieSanitizer middleware missing");
            return HttpResponse::InternalServerError().body("Internal error");
        }
    };
    let token = &sc.token;

    // Check if token is marked valid (PoW done)
    {
        let tokens_map = data.tokens.lock().await;
        if let Some(info) = tokens_map.get(token) {
            if info.valid {
                info!("Token {} already valid – redirecting", token);
                return HttpResponse::Found()
                    .append_header(("Location", "https://google.com"))
                    .finish();
            }
        }
    }

    // Otherwise, find or create a challenge for this token
    let challenge_str = {
        let mut challenges_map = data.challenges.lock().await;
        if let Some(ch) = challenges_map.get(token) {
            // If still within expiry window, reuse
            if ch.created_at + Duration::seconds(CHALLENGE_EXPIRY_SECS) > OffsetDateTime::now_utc() {
                ch.challenge.clone()
            } else {
                // Expired challenge → remove and fall through to make a new one
                challenges_map.remove(token);
                let new_ch = Challenge {
                    challenge: generate_secure_token(),
                    created_at: OffsetDateTime::now_utc(),
                    attempts: 0,
                };
                let chal_str = new_ch.challenge.clone();
                challenges_map.insert(token.clone(), new_ch);
                chal_str
            }
        } else {
            // No existing challenge → create new
            let new_ch = Challenge {
                challenge: generate_secure_token(),
                created_at: OffsetDateTime::now_utc(),
                attempts: 0,
            };
            let chal_str = new_ch.challenge.clone();
            challenges_map.insert(token.clone(), new_ch);
            chal_str
        }
    };

    // Render HTML form with `challenge_str`
    let html_body = html::get_html_template(&challenge_str);
    HttpResponse::Ok().content_type("text/html").body(html_body)
}

#[derive(Deserialize)]
pub struct PowSubmission {
    nonce: String,
}

/// POST handler: validate nonce, mark token valid if PoW passes.
pub async fn post_handler(
    data: web::Data<AppState>,
    req: HttpRequest,
    form: web::Form<PowSubmission>,
) -> impl Responder {
    // Extract sanitized cookie info
    let extensions = req.extensions();
    let sc = match extensions.get::<SanitizedCookie>() {
        Some(sc) => sc,
        None => {
            error!("CookieSanitizer middleware missing");
            return HttpResponse::InternalServerError().body("Internal error");
        }
    };
    let token = &sc.token;

    // Length check on nonce
    if form.nonce.len() > MAX_NONCE_LENGTH {
        warn!("Nonce too long for token {}", token);
        return HttpResponse::BadRequest().body("Nonce too long");
    }

    // Acquire locks for challenges and tokens
    let mut challenges_map = data.challenges.lock().await;
    let mut tokens_map = data.tokens.lock().await;

    // Fetch challenge
    let challenge_entry = match challenges_map.get_mut(token) {
        Some(ch) => ch,
        None => {
            warn!("No active challenge for token {}", token);
            return HttpResponse::Forbidden().body("No active challenge, please refresh");
        }
    };

    // Check challenge expiry
    if challenge_entry.created_at + Duration::seconds(CHALLENGE_EXPIRY_SECS) < OffsetDateTime::now_utc() {
        challenges_map.remove(token);
        warn!("Challenge expired for token {}", token);
        return HttpResponse::Forbidden().body("Challenge expired, please refresh");
    }

    // Rate‐limit attempts
    if challenge_entry.attempts >= MAX_CHALLENGE_ATTEMPTS {
        warn!("Rate limit exceeded for token {}", token);
        return HttpResponse::TooManyRequests().body("Too many attempts, try again later");
    }

    challenge_entry.attempts += 1;
    let data_to_hash = format!("{}{}", challenge_entry.challenge, form.nonce);
    let hash_bytes = Sha256::digest(data_to_hash.as_bytes());
    let hash_hex = hex::encode(hash_bytes);

    if hash_hex.starts_with(POW_DIFFICULTY_PREFIX) {
        info!("PoW verified for token {}", token);
        // Mark token valid
        if let Some(info) = tokens_map.get_mut(token) {
            info.valid = true;
            info.issued_at = OffsetDateTime::now_utc();
        } else {
            // (Should not happen: token always inserted by middleware)
            tokens_map.insert(
                token.clone(),
                TokenInfo {
                    issued_at: OffsetDateTime::now_utc(),
                    valid: true,
                },
            );
        }
        // Remove challenge
        challenges_map.remove(token);

        // Refresh cookie expiry
        let cookie = Cookie::build(COOKIE_NAME, token.clone())
            .path("/")
            .max_age(Duration::seconds(TOKEN_EXPIRY_SECS))
            .http_only(true)
            .secure(true)
            .same_site(actix_web::cookie::SameSite::Lax)
            .finish();

        let mut resp = HttpResponse::Ok().body("PoW verified, access granted");
        resp.add_cookie(&cookie).unwrap();
        resp
    } else {
        warn!("Invalid nonce for token {}", token);
        HttpResponse::Forbidden().body("Invalid nonce")
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let app_state = web::Data::new(AppState::new());

    HttpServer::new(move || {
        App::new()
            .wrap(CookieSanitizer)
            .app_data(app_state.clone())
            .service(web::resource("/{_:.*}").guard(guard::Get()).to(get_handler))
            .service(web::resource("/{_:.*}").guard(guard::Post()).to(post_handler))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
