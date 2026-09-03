use axum::{
    extract::Json,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    Reader,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Reader => "listener",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub password_hash: String,
    pub role: UserRole,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionUser {
    pub username: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Session {
    username: String,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct AuthService {
    root_dir: PathBuf,
    users_path: PathBuf,
    sessions_path: PathBuf,
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    users: Arc<RwLock<HashMap<String, User>>>,
    login_failures: Arc<Mutex<HashMap<String, (chrono::DateTime<Utc>, u32)>>>,
}

impl AuthService {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        let root_dir = root_dir.into();
        let users_path = root_dir.join("users.json");
        let sessions_path = root_dir.join("sessions.json");
        let users = match fs::read_to_string(&users_path) {
            Ok(contents) => serde_json::from_str::<HashMap<String, User>>(&contents).unwrap_or_default(),
            Err(_) => HashMap::new(),
        };
        let sessions = match fs::read_to_string(&sessions_path) {
            Ok(contents) => serde_json::from_str::<HashMap<String, Session>>(&contents).unwrap_or_default(),
            Err(_) => HashMap::new(),
        };

        if let Some(parent) = users_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        Self {
            root_dir,
            users_path,
            sessions_path,
            sessions: Arc::new(RwLock::new(sessions)),
            users: Arc::new(RwLock::new(users)),
            login_failures: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn ensure_default_admin(&self, username: &str, password: &str) {
        if self.users.read().unwrap().is_empty() {
            let _ = self.create_user_internal(username, password, UserRole::Admin, true);
        }
    }

    fn write_users(&self, users: &HashMap<String, User>) {
        if let Some(parent) = self.users_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let payload = serde_json::to_string_pretty(users).expect("users payload is serializable");
        let _ = fs::write(&self.users_path, payload);
    }

    fn write_sessions(&self, sessions: &HashMap<String, Session>) {
        if let Some(parent) = self.sessions_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let payload = serde_json::to_string_pretty(sessions).expect("sessions payload is serializable");
        let _ = fs::write(&self.sessions_path, payload);
    }

    fn normalize_username(username: &str) -> String {
        username.trim().to_lowercase()
    }

    fn user_dir(&self, username: &str) -> PathBuf {
        let safe = Self::normalize_username(username);
        self.root_dir.join(format!("user-{safe}"))
    }

    pub fn favorites_path_for_user(&self, username: &str) -> PathBuf {
        let directory = self.user_dir(username);
        let _ = fs::create_dir_all(&directory);
        directory.join("favorites.json")
    }

    fn create_user_internal(&self, username: &str, password: &str, role: UserRole, skip_password_check: bool) -> Result<User, String> {
        let username = username.trim();
        if username.is_empty() {
            return Err("Username is required".to_string());
        }
        if !skip_password_check && password.trim().is_empty() {
            return Err("Password is required".to_string());
        }

        let normalized = Self::normalize_username(username);
        let mut users = self.users.write().unwrap();
        if users.contains_key(&normalized) {
            return Err(format!("User '{}' already exists", username));
        }

        let password_hash = hash(password, DEFAULT_COST).map_err(|_| "Failed to hash password".to_string())?;
        let user = User {
            username: normalized.clone(),
            password_hash,
            role,
        };
        users.insert(normalized.clone(), user.clone());
        self.write_users(&users);
        let _ = fs::create_dir_all(self.user_dir(&normalized));
        Ok(user)
    }

    pub fn login(&self, username: &str, password: &str) -> Result<(User, String), String> {
        let normalized = Self::normalize_username(username);
        let now = Utc::now();
        {
            let failures = self.login_failures.lock().unwrap();
            if let Some((window_started, count)) = failures.get(&normalized) {
                if *count >= 5 && *window_started + chrono::Duration::minutes(15) > now {
                    return Err("Too many login attempts; try again later".to_string());
                }
            }
        }

        let users = self.users.read().unwrap();
        let user = users
            .get(&normalized)
            .cloned()
            .ok_or_else(|| {
                self.record_login_failure(&normalized, now);
                "Invalid credentials".to_string()
            })?;

        if !verify(password, &user.password_hash).map_err(|_| "Password verification failed".to_string())? {
            self.record_login_failure(&normalized, now);
            return Err("Invalid credentials".to_string());
        }

        self.login_failures.lock().unwrap().remove(&normalized);
        let session_id = uuid::Uuid::new_v4().to_string();
        let mut sessions = self.sessions.write().unwrap();
        sessions.insert(session_id.clone(), Session {
            username: normalized.clone(),
            expires_at: now + chrono::Duration::days(365),
        });
        self.write_sessions(&sessions);
        Ok((user, session_id))
    }

    fn record_login_failure(&self, username: &str, now: chrono::DateTime<Utc>) {
        let mut failures = self.login_failures.lock().unwrap();
        let entry = failures.entry(username.to_string()).or_insert((now, 0));
        if entry.0 + chrono::Duration::minutes(15) <= now {
            *entry = (now, 0);
        }
        entry.1 = entry.1.saturating_add(1);
    }

    pub fn current_user_from_session(&self, session_id: &str) -> Option<User> {
        let now = Utc::now();
        let mut sessions = self.sessions.write().unwrap();
        let session = sessions.get(session_id)?.clone();
        if session.expires_at <= now {
            sessions.remove(session_id);
            self.write_sessions(&sessions);
            return None;
        }
        if session.expires_at < now + chrono::Duration::days(365) {
            if let Some(active_session) = sessions.get_mut(session_id) {
                active_session.expires_at = now + chrono::Duration::days(365);
            }
            self.write_sessions(&sessions);
        }
        let username = session.username;
        let users = self.users.read().unwrap();
        users.get(&username).cloned()
    }

    pub fn create_user(&self, username: &str, password: &str, role: &str, actor: &User) -> Result<User, String> {
        if !actor.role.eq(&UserRole::Admin) {
            return Err("Admin access required".to_string());
        }

        let user_role = match role.to_ascii_lowercase().as_str() {
            "admin" => UserRole::Admin,
            "listener" | "reader" | "readonly" => UserRole::Reader,
            _ => UserRole::Reader,
        };

        self.create_user_internal(username, password, user_role, false)
    }

    pub fn change_password_for_user(&self, username: &str, old_password: &str, new_password: &str) -> Result<(), String> {
        let normalized = Self::normalize_username(username);
        let mut users = self.users.write().unwrap();
        let user = users
            .get_mut(&normalized)
            .ok_or_else(|| "User not found".to_string())?;

        if !verify(old_password, &user.password_hash).map_err(|_| "Password verification failed".to_string())? {
            return Err("Current password is incorrect".to_string());
        }

        let trimmed_new = new_password.trim();
        if trimmed_new.is_empty() {
            return Err("New password is required".to_string());
        }

        let new_hash = hash(trimmed_new, DEFAULT_COST).map_err(|_| "Failed to hash password".to_string())?;
        user.password_hash = new_hash;
        self.write_users(&users);
        Ok(())
    }

    pub fn logout(&self, session_id: &str) {
        let mut sessions = self.sessions.write().unwrap();
        sessions.remove(session_id);
        self.write_sessions(&sessions);
    }

    pub fn extract_session_id(headers: &HeaderMap) -> Option<String> {
        let cookie_value = headers.get("cookie")?.to_str().ok()?;
        for part in cookie_value.split(';') {
            let part = part.trim();
            if let Some(value) = part.strip_prefix("session_id=") {
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
        None
    }

    const SESSION_COOKIE_MAX_AGE: i64 = 365 * 24 * 60 * 60;

    fn cookie_header(session_id: &str) -> HeaderValue {
        let expires = (Utc::now() + chrono::Duration::seconds(Self::SESSION_COOKIE_MAX_AGE))
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();

        HeaderValue::from_str(&format!(
            "session_id={session_id}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={}; Expires={}",
            Self::SESSION_COOKIE_MAX_AGE,
            expires
        ))
        .expect("valid session cookie")
    }

    pub fn login_response(&self, _headers: HeaderMap, Json(payload): Json<LoginRequest>) -> Response {
        match self.login(&payload.username, &payload.password) {
            Ok((user, session_id)) => {
                let response = (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "username": user.username,
                        "role": user.role.as_str()
                    })),
                )
                    .into_response();
                let mut response = response;
                response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
                response.headers_mut().append(header::SET_COOKIE, Self::cookie_header(&session_id));
                response
            }
            Err(message) => (
                if message.starts_with("Too many") { StatusCode::TOO_MANY_REQUESTS } else { StatusCode::UNAUTHORIZED },
                Json(serde_json::json!({ "error": message })),
            )
                .into_response(),
        }
    }

    pub fn me_response(&self, headers: HeaderMap) -> Response {
        let Some(session_id) = Self::extract_session_id(&headers) else {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Not authenticated" }))).into_response();
        };

        let Some(user) = self.current_user_from_session(&session_id) else {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Session expired" }))).into_response();
        };

        let response = (
            StatusCode::OK,
            Json(serde_json::json!({
                "username": user.username,
                "role": user.role.as_str()
            })),
        )
            .into_response();
        let mut response = response;
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response.headers_mut().append(header::SET_COOKIE, Self::cookie_header(&session_id));
        response
    }

    pub fn logout_response(&self, headers: HeaderMap) -> Response {
        if let Some(session_id) = Self::extract_session_id(&headers) {
            self.logout(&session_id);
        }

        let response = (StatusCode::OK, Json(serde_json::json!({ "success": true }))).into_response();
        let mut response = response;
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_str("session_id=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; HttpOnly; Secure; SameSite=Lax").expect("valid logout cookie"),
        );
        response
    }

    pub fn create_user_response(&self, headers: HeaderMap, Json(payload): Json<CreateUserRequest>) -> Response {
        let Some(session_id) = Self::extract_session_id(&headers) else {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Not authenticated" }))).into_response();
        };

        let Some(actor) = self.current_user_from_session(&session_id) else {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Session expired" }))).into_response();
        };

        match self.create_user(&payload.username, &payload.password, &payload.role, &actor) {
            Ok(user) => (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "username": user.username,
                    "role": user.role.as_str()
                })),
            )
                .into_response(),
            Err(message) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": message })),
            )
                .into_response(),
        }
    }

    pub fn change_password_response(&self, headers: HeaderMap, Json(payload): Json<ChangePasswordRequest>) -> Response {
        let Some(session_id) = Self::extract_session_id(&headers) else {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Not authenticated" }))).into_response();
        };

        let Some(actor) = self.current_user_from_session(&session_id) else {
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Session expired" }))).into_response();
        };

        match self.change_password_for_user(&actor.username, &payload.old_password, &payload.new_password) {
            Ok(_) => (
                StatusCode::OK,
                Json(serde_json::json!({ "success": true })),
            )
                .into_response(),
            Err(message) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": message })),
            )
                .into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_user_for_reader_is_allowed_for_admin() {
        let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
        let auth = AuthService::new(&temp_dir);
        auth.ensure_default_admin("admin", "secret");

        let admin = auth.current_user_from_session(
            &auth.login("admin", "secret").unwrap().1,
        )
        .unwrap();

        let created = auth.create_user("reader01", "hello", "reader", &admin).unwrap();
        assert_eq!(created.role, UserRole::Reader);
        assert!(auth.favorites_path_for_user("reader01").exists());

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn admin_required_for_user_creation() {
        let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
        let auth = AuthService::new(&temp_dir);
        auth.ensure_default_admin("admin", "secret");

        let reader_user = auth.create_user_internal("reader", "pass", UserRole::Reader, false).unwrap();
        let session = auth.login("reader", "pass").unwrap().1;
        let actor = auth.current_user_from_session(&session).unwrap();

        assert_ne!(actor.role, UserRole::Admin);
        assert!(auth.create_user("reader2", "pass", "reader", &reader_user).is_err());

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn user_can_change_their_own_password() {
        let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
        let auth = AuthService::new(&temp_dir);
        auth.ensure_default_admin("admin", "secret");

        auth.change_password_for_user("admin", "secret", "new-secret").unwrap();
        assert!(auth.login("admin", "new-secret").is_ok());
        assert!(auth.login("admin", "secret").is_err());

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn session_remains_valid_after_service_restart() {
        let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
        let auth = AuthService::new(&temp_dir);
        auth.ensure_default_admin("admin", "secret");

        let (_, session_id) = auth.login("admin", "secret").unwrap();

        let restarted = AuthService::new(&temp_dir);
        assert_eq!(restarted.current_user_from_session(&session_id).unwrap().username, "admin");

        let _ = fs::remove_dir_all(temp_dir);
    }
}
