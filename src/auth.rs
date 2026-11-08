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
    io::Write,
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
pub struct HeartbeatRequest {
    pub station: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ActivePresence {
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub guest: bool,
    #[serde(default)]
    pub station: Option<String>,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub active_seconds: u64,
    #[serde(default)]
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AdminStatsResponse {
    #[serde(default)]
    pub stats_enabled: bool,
    #[serde(default)]
    pub guest_tracking_enabled: bool,
    #[serde(default)]
    pub active_users: Vec<ActivePresence>,
    #[serde(default)]
    pub guests: Vec<ActivePresence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Session {
    username: String,
    expires_at: chrono::DateTime<Utc>,
    started_at: chrono::DateTime<Utc>,
    #[serde(default)]
    last_seen: Option<chrono::DateTime<Utc>>,
    ip: Option<String>,
    user_agent: Option<String>,
    current_station: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GuestSession {
    id: String,
    started_at: chrono::DateTime<Utc>,
    last_seen: chrono::DateTime<Utc>,
    ip: String,
    user_agent: Option<String>,
    current_station: Option<String>,
}

/// One live client: a browser that loaded the app page and keeps refreshing
/// `/api/heartbeat`. Never persisted, so a server restart starts empty.
#[derive(Debug, Clone)]
struct PresenceRecord {
    username: Option<String>,
    guest: bool,
    ip: String,
    station: Option<String>,
    started_at: chrono::DateTime<Utc>,
    last_seen: chrono::DateTime<Utc>,
}

/// In-memory tracker of who has actually loaded the app page, one timer per
/// browser identity (keyed by username + IP for signed-in users, plain IP for
/// guests). Only `/api/heartbeat` records into it, so admin stats never count
/// background API traffic, and timers expire after a silence window so closed
/// browser tabs do not accumulate "active" time.
#[derive(Debug, Default)]
struct PresenceTracker {
    entries: RwLock<HashMap<String, PresenceRecord>>,
}

impl PresenceTracker {
    /// How long presence stays valid without a fresh heartbeat.
    fn timeout() -> chrono::Duration {
        chrono::Duration::minutes(ACTIVE_PRESENCE_TIMEOUT_MINUTES)
    }

    /// One record per identity + IP keeps parallel tabs/requests from
    /// multiplying the session timer.
    fn entry_key(username: Option<&str>, guest: bool, ip: &str) -> String {
        match username.filter(|_| !guest) {
            Some(name) => format!("user:{name}@{ip}"),
            None => format!("guest:{ip}"),
        }
    }

    /// Create or refresh one client's presence. The `started_at` timer only
    /// survives while heartbeats keep arriving: after [`Self::timeout`]
    /// of silence the record is dropped, so the next heartbeat brings a
    /// completely fresh arrival with a zeroed timer.
    fn record(
        &self,
        username: Option<&str>,
        guest: bool,
        ip: &str,
        station: Option<&str>,
        now: chrono::DateTime<Utc>,
    ) {
        let mut entries = self.entries.write().unwrap();
        entries.retain(|_, entry| entry.last_seen + Self::timeout() > now);
        // Guests are keyed by IP only, so their username (if any) is ignored.
        let identity = username.filter(|_| !guest);
        entries
            .entry(Self::entry_key(username, guest, ip))
            .and_modify(|entry| {
                entry.last_seen = now;
                entry.station = station.map(str::to_owned);
            })
            .or_insert_with(|| PresenceRecord {
                username: identity.map(str::to_owned),
                guest,
                ip: ip.to_string(),
                station: station.map(str::to_owned),
                started_at: now,
                last_seen: now,
            });
    }

    /// Prune silently-gone clients and split the remaining ones into the
    /// admin stats' user and guest rows.
    fn active_rows(
        &self,
        now: chrono::DateTime<Utc>,
    ) -> (Vec<ActivePresence>, Vec<ActivePresence>) {
        let mut entries = self.entries.write().unwrap();
        entries.retain(|_, entry| entry.last_seen + Self::timeout() > now);
        let mut users = Vec::new();
        let mut guests = Vec::new();
        for entry in entries.values() {
            let row = ActivePresence {
                username: entry.username.clone(),
                guest: entry.guest,
                station: entry.station.clone(),
                ip: entry.ip.clone(),
                active_seconds: (now - entry.started_at).num_seconds().max(0) as u64,
                started_at: entry.started_at.to_rfc3339(),
            };
            if row.guest {
                guests.push(row);
            } else {
                users.push(row);
            }
        }
        (users, guests)
    }

    /// Drop one signed-in browser's record: its live session ended explicitly
    /// (logout), so the admin list stops reporting it right away.
    fn drop_user(&self, username: &str, ip: Option<&str>) {
        if let Some(ip) = ip {
            self.entries
                .write()
                .unwrap()
                .remove(&Self::entry_key(Some(username), false, ip));
        }
    }

    /// Drop a user's records when their sessions are revoked server-side
    /// (password change). Guests have no username and stay untouched.
    fn drop_user_everywhere(&self, username: &str) {
        self.entries
            .write()
            .unwrap()
            .retain(|_, entry| entry.username.as_deref() != Some(username));
    }
}

/// Append-only login/session trail with retention-based pruning. Stays
/// disabled until `LOG_RETENTION_DAYS` is set; files are created/updated with
/// owner-only permissions.
#[derive(Debug, Default)]
struct ActivityLog {
    path: PathBuf,
    retention_days: u64,
}

impl ActivityLog {
    fn new(path: PathBuf, retention_days: u64) -> Self {
        Self {
            path,
            retention_days,
        }
    }

    fn enabled(&self) -> bool {
        self.retention_days > 0
    }

    fn open(&self) -> std::io::Result<fs::File> {
        let mut options = fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let file = options.open(&self.path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(file)
    }

    fn initialize(&self) {
        if !self.enabled() {
            return;
        }

        match self.open() {
            Ok(_) => self.prune(),
            Err(error) => tracing::error!(
                path = %self.path.display(),
                error = %error,
                "Failed to initialize activity log"
            ),
        }
    }

    fn prune(&self) {
        if !self.enabled() {
            return;
        }

        let cutoff = Utc::now() - chrono::Duration::days(self.retention_days as i64);
        let Ok(contents) = fs::read_to_string(&self.path) else {
            return;
        };

        let filtered: Vec<String> = contents
            .lines()
            .filter_map(|line| {
                if Self::timestamp(line).is_some_and(|timestamp| timestamp >= cutoff) {
                    Some(line.to_string())
                } else {
                    None
                }
            })
            .collect();

        let payload = if filtered.is_empty() {
            String::new()
        } else {
            format!("{}\n", filtered.join("\n"))
        };
        if payload != contents {
            if let Err(error) = fs::write(&self.path, payload) {
                tracing::warn!(
                    path = %self.path.display(),
                    error = %error,
                    "Failed to prune activity log"
                );
            }
        }
    }

    /// Parse an entry timestamp, accepting the text format this log writes
    /// (`[rfc3339]`) and the legacy JSON format possibly left behind by an
    /// older build.
    fn timestamp(line: &str) -> Option<chrono::DateTime<Utc>> {
        if let Some(timestamp) = line
            .strip_prefix('[')
            .and_then(|line| line.split_once(']'))
            .map(|(timestamp, _)| timestamp)
        {
            return chrono::DateTime::parse_from_rfc3339(timestamp)
                .ok()
                .map(|timestamp| timestamp.with_timezone(&Utc));
        }

        let entry: serde_json::Value = serde_json::from_str(line).ok()?;
        let timestamp = entry.get("ts")?.as_str()?;
        chrono::DateTime::parse_from_rfc3339(timestamp)
            .ok()
            .map(|timestamp| timestamp.with_timezone(&Utc))
    }

    /// Append one event line. The retention pruning runs after every write,
    /// which keeps a leaked/growing log within its configured window.
    fn record(
        &self,
        event: &str,
        username: Option<&str>,
        ip: Option<&str>,
        station: Option<&str>,
        details: Option<serde_json::Value>,
    ) {
        if !self.enabled() {
            return;
        }

        let event_label = match event {
            "session_start" => "Session started",
            "session_end" => "Session ended",
            "station_started" => "Station started",
            "guest_session_start" => "Guest session started",
            "user_created" => "User created",
            "password_changed" => "Password changed",
            _ => event,
        };
        let mut fields = vec![
            format!(
                "[{}]",
                Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            ),
            event_label.to_string(),
        ];
        if let Some(username) = username {
            fields.push(format!("User: {}", sanitize_for_log(username)));
        }
        if let Some(ip) = ip {
            fields.push(format!("IP: {}", sanitize_for_log(ip)));
        }
        if let Some(station) = station {
            fields.push(format!("Station: {}", sanitize_for_log(station)));
        }
        if let Some(details) = details.and_then(|details| details.as_object().cloned()) {
            for (key, value) in details {
                if key == "session_id" || value.is_null() {
                    continue;
                }
                let value = value
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| value.to_string());
                fields.push(format!(
                    "{}: {}",
                    key.replace('_', " "),
                    sanitize_for_log(&value)
                ));
            }
        }
        let entry = fields.join(" | ");

        let mut file = match self.open() {
            Ok(file) => file,
            Err(error) => {
                tracing::error!(
                    path = %self.path.display(),
                    error = %error,
                    "Failed to open activity log"
                );
                return;
            }
        };
        if let Err(error) = writeln!(file, "{entry}") {
            tracing::error!(
                path = %self.path.display(),
                error = %error,
                "Failed to write activity log entry"
            );
            return;
        }
        self.prune();
    }
}

/// Username allowlist: normalized usernames must match this. It keeps
/// per-user file paths (`user-<name>/...`) free of traversal sequences.
pub const MAX_USERNAME_LEN: usize = 64;

fn is_valid_username(normalized: &str) -> bool {
    !normalized.is_empty()
        && normalized.len() <= MAX_USERNAME_LEN
        && normalized
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

/// Strip control characters and cap length so untrusted input (login attempts
/// may carry arbitrary usernames) cannot forge lines in the logs.
fn sanitize_for_log(value: &str) -> String {
    value.chars().filter(|c| !c.is_control()).take(64).collect()
}

fn parse_admin_stats_flag(raw: Option<&str>) -> bool {
    raw.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn parse_log_retention_days(raw: Option<&str>) -> u64 {
    raw.and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

/// Recent login failures per username: (window start, attempts in window).
type LoginFailures = HashMap<String, (chrono::DateTime<Utc>, u32)>;
/// Bound on tracked usernames so failure tracking itself cannot grow without
/// limit (e.g. via username rotation).
const MAX_TRACKED_FAILURES: usize = 10_000;
const ACTIVE_PRESENCE_TIMEOUT_MINUTES: i64 = 3;

#[derive(Debug, Default)]
pub struct AuthService {
    root_dir: PathBuf,
    users_path: PathBuf,
    sessions_path: PathBuf,
    stats_enabled: bool,
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    users: Arc<RwLock<HashMap<String, User>>>,
    guest_sessions: Arc<RwLock<HashMap<String, GuestSession>>>,
    presence: PresenceTracker,
    activity_log: ActivityLog,
    login_failures: Arc<Mutex<LoginFailures>>,
}

impl AuthService {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        let root_dir = root_dir.into();
        let users_path = root_dir.join("users.json");
        let sessions_path = root_dir.join("sessions.json");
        let users = match fs::read_to_string(&users_path) {
            Ok(contents) => {
                serde_json::from_str::<HashMap<String, User>>(&contents).unwrap_or_default()
            }
            Err(_) => HashMap::new(),
        };
        let sessions = match fs::read_to_string(&sessions_path) {
            Ok(contents) => {
                serde_json::from_str::<HashMap<String, Session>>(&contents).unwrap_or_default()
            }
            Err(_) => HashMap::new(),
        };
        let stats_enabled =
            parse_admin_stats_flag(std::env::var("ADMIN_STATS_ENABLED").ok().as_deref());
        let log_retention_days =
            parse_log_retention_days(std::env::var("LOG_RETENTION_DAYS").ok().as_deref());
        let activity_log = ActivityLog::new(root_dir.join("activity.log"), log_retention_days);

        if let Some(parent) = users_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let auth = Self {
            root_dir,
            users_path,
            sessions_path,
            stats_enabled,
            sessions: Arc::new(RwLock::new(sessions)),
            users: Arc::new(RwLock::new(users)),
            guest_sessions: Arc::new(RwLock::new(HashMap::new())),
            presence: PresenceTracker::default(),
            activity_log,
            login_failures: Arc::new(Mutex::new(HashMap::new())),
        };
        if !auth.users_path.exists() {
            auth.write_users(&auth.users.read().unwrap());
        }
        if !auth.sessions_path.exists() {
            auth.write_sessions(&auth.sessions.read().unwrap());
        }
        auth.activity_log.initialize();
        auth
    }

    pub fn ensure_default_admin(&self, username: &str, password: &str) {
        if self.users.read().unwrap().is_empty()
            && self
                .create_user_internal(username, password, UserRole::Admin, true)
                .is_ok()
        {
            tracing::info!("Created default admin account");
        }
    }

    /// Atomically persist JSON: write to a temp file with owner-only
    /// permissions, then rename over the target. A crash mid-write can never
    /// leave a truncated file behind (which would otherwise be read back as
    /// empty and wipe all users/sessions).
    fn write_json_file(path: &std::path::Path, payload: &str) {
        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                tracing::warn!(path = %path.display(), error = %err, "Failed to create data directory");
                return;
            }
        }
        let tmp_path = path.with_extension("json.tmp");
        if let Err(err) = fs::write(&tmp_path, payload) {
            tracing::warn!(path = %tmp_path.display(), error = %err, "Failed to write data file");
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(err) = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600)) {
                tracing::warn!(path = %tmp_path.display(), error = %err, "Failed to restrict data file permissions");
                return;
            }
        }
        if let Err(err) = fs::rename(&tmp_path, path) {
            tracing::warn!(path = %path.display(), error = %err, "Failed to persist data file");
            let _ = fs::remove_file(&tmp_path);
        }
    }

    fn write_users(&self, users: &HashMap<String, User>) {
        let payload = serde_json::to_string_pretty(users).expect("users payload is serializable");
        Self::write_json_file(&self.users_path, &payload);
    }

    fn write_sessions(&self, sessions: &HashMap<String, Session>) {
        let payload =
            serde_json::to_string_pretty(sessions).expect("sessions payload is serializable");
        Self::write_json_file(&self.sessions_path, &payload);
    }

    fn normalize_username(username: &str) -> String {
        username.trim().to_lowercase()
    }

    fn user_dir(&self, username: &str) -> PathBuf {
        let safe = Self::normalize_username(username);
        self.root_dir.join(format!("user-{safe}"))
    }

    fn real_ip(headers: &HeaderMap) -> String {
        for name in [
            "CF-Connecting-IP",
            "True-Client-IP",
            "X-Real-IP",
            "X-Forwarded-For",
        ] {
            let Some(value) = headers.get(name) else {
                continue;
            };
            let Ok(value) = value.to_str() else {
                continue;
            };
            let candidate = value
                .split(',')
                .map(str::trim)
                .find(|piece| !piece.is_empty())
                .unwrap_or(value.trim());
            if !candidate.is_empty() {
                return candidate.to_string();
            }
        }
        "unknown".to_string()
    }

    fn request_is_secure(headers: &HeaderMap) -> bool {
        let forwarded_proto = headers
            .get_all("x-forwarded-proto")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .map(str::trim)
            .any(|value| value.eq_ignore_ascii_case("https"));
        if forwarded_proto {
            return true;
        }

        headers
            .get("x-forwarded-ssl")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.eq_ignore_ascii_case("on"))
            || headers
                .get("forwarded")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| {
                    value.split(';').any(|pair| {
                        pair.trim()
                            .split('=')
                            .map(str::trim)
                            .collect::<Vec<_>>()
                            .get(0)
                            .is_some_and(|key| key.eq_ignore_ascii_case("proto"))
                            && pair
                                .split('=')
                                .map(str::trim)
                                .collect::<Vec<_>>()
                                .get(1)
                                .is_some_and(|value| value.eq_ignore_ascii_case("https"))
                    })
                })
    }

    fn session_cookie_name(headers: &HeaderMap) -> &'static str {
        if Self::request_is_secure(headers) {
            "__Host-session_id"
        } else {
            "session_id"
        }
    }

    fn alternate_session_cookie_name(headers: &HeaderMap) -> &'static str {
        if Self::request_is_secure(headers) {
            "session_id"
        } else {
            "__Host-session_id"
        }
    }

    fn guest_cookie_name(headers: &HeaderMap) -> &'static str {
        if Self::request_is_secure(headers) {
            "__Host-guest_session_id"
        } else {
            "guest_session_id"
        }
    }

    fn extract_guest_session_id(headers: &HeaderMap) -> Option<String> {
        let prefixes = [
            format!("{}=", Self::guest_cookie_name(headers)),
            "guest_session_id=".to_string(),
            "__Host-guest_session_id=".to_string(),
        ];
        for value in headers.get_all("cookie") {
            let Ok(cookie_value) = value.to_str() else {
                continue;
            };
            for part in cookie_value.split(';') {
                let part = part.trim();
                for prefix in &prefixes {
                    if let Some(session_id) = part.strip_prefix(prefix.as_str()) {
                        if !session_id.is_empty() && session_id.len() <= 128 {
                            return Some(session_id.to_string());
                        }
                    }
                }
            }
        }
        None
    }

    fn guest_cookie_header(session_id: &str, headers: &HeaderMap) -> HeaderValue {
        let cookie_name = Self::guest_cookie_name(headers);
        let secure = Self::request_is_secure(headers);
        HeaderValue::from_str(&format!(
            "{cookie_name}={session_id}; Path=/; HttpOnly;{} SameSite=Lax; Max-Age=86400; Expires={}",
            if secure { " Secure;" } else { "" },
            (Utc::now() + chrono::Duration::hours(24))
                .format("%a, %d %b %Y %H:%M:%S GMT")
                .to_string()
        ))
        .expect("valid guest cookie")
    }

    fn clear_guest_cookie_header(headers: &HeaderMap) -> HeaderValue {
        let cookie_name = Self::guest_cookie_name(headers);
        let secure = Self::request_is_secure(headers);
        HeaderValue::from_str(&format!(
            "{cookie_name}=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; HttpOnly;{} SameSite=Lax",
            if secure { " Secure;" } else { "" }
        ))
        .expect("valid guest logout cookie")
    }

    /// Create a directory with owner-only access when possible, so per-user
    /// data is not left world-readable on shared volumes.
    fn ensure_private_dir(path: &std::path::Path) {
        if let Err(err) = fs::create_dir_all(path) {
            tracing::warn!(path = %path.display(), error = %err, "Failed to create directory");
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(err) = fs::set_permissions(path, fs::Permissions::from_mode(0o700)) {
                tracing::warn!(path = %path.display(), error = %err, "Failed to restrict directory permissions");
            }
        }
    }

    pub fn favorites_path_for_user(&self, username: &str) -> PathBuf {
        let directory = self.user_dir(username);
        Self::ensure_private_dir(&directory);
        directory.join("favorites.json")
    }

    fn create_user_internal(
        &self,
        username: &str,
        password: &str,
        role: UserRole,
        skip_password_check: bool,
    ) -> Result<User, String> {
        let normalized = Self::normalize_username(username);
        if !is_valid_username(&normalized) {
            return Err(
                "Username must be 1-64 characters: lowercase letters, digits, '.', '_' or '-'"
                    .to_string(),
            );
        }
        if password.len() > 128 {
            return Err("Password is too long".to_string());
        }
        if !skip_password_check && password.trim().is_empty() {
            return Err("Password is required".to_string());
        }

        let mut users = self.users.write().unwrap();
        if users.contains_key(&normalized) {
            return Err(format!("User '{normalized}' already exists"));
        }

        let password_hash =
            hash(password, DEFAULT_COST).map_err(|_| "Failed to hash password".to_string())?;
        let user = User {
            username: normalized.clone(),
            password_hash,
            role,
        };
        users.insert(normalized.clone(), user.clone());
        self.write_users(&users);
        Self::ensure_private_dir(&self.user_dir(&normalized));
        Ok(user)
    }

    pub fn login(&self, username: &str, password: &str) -> Result<(User, String), String> {
        self.login_with_context(username, password, &HeaderMap::new())
    }

    pub fn login_with_context(
        &self,
        username: &str,
        password: &str,
        headers: &HeaderMap,
    ) -> Result<(User, String), String> {
        let normalized = Self::normalize_username(username);
        let now = Utc::now();
        {
            let failures = self.login_failures.lock().unwrap();
            if let Some((window_started, count)) = failures.get(&normalized) {
                if *count >= 5 && *window_started + chrono::Duration::minutes(15) > now {
                    tracing::warn!(
                        username = %sanitize_for_log(&normalized),
                        "Login blocked: too many attempts"
                    );
                    return Err("Too many login attempts; try again later".to_string());
                }
            }
        }

        let users = self.users.read().unwrap();
        let user = users.get(&normalized).cloned().ok_or_else(|| {
            self.record_login_failure(&normalized, now);
            tracing::warn!(
                username = %sanitize_for_log(&normalized),
                "Failed login attempt (unknown user)"
            );
            "Invalid credentials".to_string()
        })?;

        if !verify(password, &user.password_hash)
            .map_err(|_| "Password verification failed".to_string())?
        {
            self.record_login_failure(&normalized, now);
            tracing::warn!(
                username = %sanitize_for_log(&normalized),
                "Failed login attempt (wrong password)"
            );
            return Err("Invalid credentials".to_string());
        }

        self.login_failures.lock().unwrap().remove(&normalized);
        let session_id = uuid::Uuid::new_v4().to_string();
        let mut sessions = self.sessions.write().unwrap();
        let ip = Self::real_ip(headers);
        sessions.insert(
            session_id.clone(),
            Session {
                username: normalized.clone(),
                expires_at: now + chrono::Duration::days(365),
                started_at: now,
                last_seen: Some(now),
                ip: Some(ip.clone()),
                user_agent: None,
                current_station: None,
            },
        );
        self.write_sessions(&sessions);
        self.activity_log
            .record("session_start", Some(&normalized), Some(&ip), None, None);
        tracing::info!(username = %sanitize_for_log(&normalized), "User logged in");
        Ok((user, session_id))
    }

    fn record_login_failure(&self, username: &str, now: chrono::DateTime<Utc>) {
        let mut failures = self.login_failures.lock().unwrap();
        let entry = failures.entry(username.to_string()).or_insert((now, 0));
        if entry.0 + chrono::Duration::minutes(15) <= now {
            *entry = (now, 0);
        }
        entry.1 = entry.1.saturating_add(1);
        // Keep tracking bounded: drop stale windows once too many distinct
        // usernames are tracked (username rotation must not grow memory).
        if failures.len() > MAX_TRACKED_FAILURES {
            failures.retain(|_, (window_started, _)| {
                *window_started + chrono::Duration::minutes(15) > now
            });
        }
    }

    pub fn current_user_from_session(&self, session_id: &str) -> Option<User> {
        let now = Utc::now();
        let mut sessions = self.sessions.write().unwrap();
        let Some(session) = sessions.get(session_id).cloned() else {
            return None;
        };
        if session.expires_at <= now {
            sessions.remove(session_id);
            self.write_sessions(&sessions);
            self.activity_log.record(
                "session_end",
                Some(&session.username),
                session.ip.as_deref(),
                session.current_station.as_deref(),
                Some(serde_json::json!({ "duration_seconds": (now - session.started_at).num_seconds().max(0) })),
            );
            return None;
        }
        let mut changed = false;
        if let Some(active_session) = sessions.get_mut(session_id) {
            if active_session.expires_at < now + chrono::Duration::days(365) {
                active_session.expires_at = now + chrono::Duration::days(365);
                changed = true;
            }
            if active_session.last_seen.map_or(true, |last_seen| {
                last_seen + chrono::Duration::seconds(30) <= now
            }) {
                active_session.last_seen = Some(now);
                changed = true;
            }
        }
        if changed {
            self.write_sessions(&sessions);
        }
        let username = session.username;
        let users = self.users.read().unwrap();
        users.get(&username).cloned()
    }

    pub fn create_user(
        &self,
        username: &str,
        password: &str,
        role: &str,
        actor: &User,
    ) -> Result<User, String> {
        if !actor.role.eq(&UserRole::Admin) {
            return Err("Admin access required".to_string());
        }

        let user_role = match role.to_ascii_lowercase().as_str() {
            "admin" => UserRole::Admin,
            "listener" | "reader" | "readonly" => UserRole::Reader,
            _ => UserRole::Reader,
        };

        let user = self.create_user_internal(username, password, user_role, false)?;
        self.activity_log.record(
            "user_created",
            Some(&user.username),
            None,
            None,
            Some(serde_json::json!({
                "actor": actor.username,
                "role": user.role.as_str(),
            })),
        );
        tracing::info!(
            username = %sanitize_for_log(&user.username),
            role = user.role.as_str(),
            actor = %sanitize_for_log(&actor.username),
            "User account created"
        );
        Ok(user)
    }

    pub fn change_password_for_user(
        &self,
        username: &str,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), String> {
        let normalized = Self::normalize_username(username);
        if !is_valid_username(&normalized) {
            return Err("User not found".to_string());
        }
        if new_password.len() > 128 {
            return Err("New password is too long".to_string());
        }
        let mut users = self.users.write().unwrap();
        let user = users
            .get_mut(&normalized)
            .ok_or_else(|| "User not found".to_string())?;

        if !verify(old_password, &user.password_hash)
            .map_err(|_| "Password verification failed".to_string())?
        {
            return Err("Current password is incorrect".to_string());
        }

        let trimmed_new = new_password.trim();
        if trimmed_new.is_empty() {
            return Err("New password is required".to_string());
        }

        let new_hash =
            hash(trimmed_new, DEFAULT_COST).map_err(|_| "Failed to hash password".to_string())?;
        user.password_hash = new_hash;
        self.write_users(&users);
        self.activity_log.record(
            "password_changed",
            Some(&normalized),
            None,
            None,
            Some(serde_json::json!({ "sessions_revoked": true })),
        );
        tracing::info!(
            username = %sanitize_for_log(&normalized),
            "Password changed; sessions revoked"
        );
        let revoked_name = normalized.clone();
        drop(users);

        // Revoke every session for this user: a password change must also
        // lock out sessions established with the old password.
        let mut sessions = self.sessions.write().unwrap();
        sessions.retain(|_, session| session.username != revoked_name);
        self.write_sessions(&sessions);
        drop(sessions);
        // Revoked sessions must not keep counting as active either.
        self.presence.drop_user_everywhere(&revoked_name);
        Ok(())
    }

    /// True when the `admin` account still uses the well-known default
    /// password. Called once at startup to emit a loud warning.
    pub fn admin_uses_default_password(&self, username: &str, password: &str) -> bool {
        let normalized = Self::normalize_username(username);
        let users = self.users.read().unwrap();
        users
            .get(&normalized)
            .is_some_and(|user| verify(password, &user.password_hash).unwrap_or(false))
    }

    pub fn logout(&self, session_id: &str) {
        let mut sessions = self.sessions.write().unwrap();
        let Some(session) = sessions.remove(session_id) else {
            tracing::debug!("Logout with unknown session");
            return;
        };
        self.write_sessions(&sessions);
        let duration = (Utc::now() - session.started_at).num_seconds().max(0) as u64;
        self.activity_log.record(
            "session_end",
            Some(&session.username),
            session.ip.as_deref(),
            session.current_station.as_deref(),
            Some(serde_json::json!({ "duration_seconds": duration })),
        );
        tracing::info!(username = %sanitize_for_log(&session.username), "User logged out");

        // "Active" ends with the live session: drop the tracker entry so the
        // admin list stops reporting this browser right away instead of
        // lingering until the presence timeout.
        self.presence
            .drop_user(&session.username, session.ip.as_deref());
    }

    pub fn admin_stats(&self) -> AdminStatsResponse {
        let now = Utc::now();
        let (mut active_users, mut guests) = self.presence.active_rows(now);
        // Presentation order: users by name (then address), guests by address.
        active_users.sort_by(|left, right| {
            left.username
                .cmp(&right.username)
                .then_with(|| left.ip.cmp(&right.ip))
        });
        guests.sort_by(|left, right| left.ip.cmp(&right.ip));

        AdminStatsResponse {
            stats_enabled: self.stats_enabled,
            guest_tracking_enabled: true,
            active_users,
            guests,
        }
    }

    pub fn ensure_guest_session(
        &self,
        headers: &HeaderMap,
        station: Option<&str>,
    ) -> Option<(String, Option<String>)> {
        if !self.stats_enabled {
            return None;
        }

        let guest_id = Self::extract_guest_session_id(headers)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let ip = Self::real_ip(headers);
        let now = Utc::now();
        let entry = GuestSession {
            id: guest_id.clone(),
            started_at: now,
            last_seen: now,
            ip: ip.clone(),
            user_agent: None,
            current_station: station.map(str::to_owned),
        };

        let mut guest_sessions = self.guest_sessions.write().unwrap();
        // Admin stats are fed by the presence tracker now; prune here so this
        // bookkeeping (cookie issuance, guest log lines) stays bounded. The
        // window is the same silence timeout as presence.
        guest_sessions.retain(|_, entry| entry.last_seen + PresenceTracker::timeout() > now);
        match guest_sessions.entry(guest_id.clone()) {
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(entry);
                self.activity_log
                    .record("guest_session_start", None, Some(&ip), station, None);
            }
            std::collections::hash_map::Entry::Occupied(mut existing) => {
                let existing = existing.get_mut();
                existing.last_seen = now;
                if existing.ip.is_empty() {
                    existing.ip = ip.clone();
                }
                if existing.current_station.as_deref() != station && station.is_some() {
                    existing.current_station = station.map(str::to_owned);
                    self.activity_log.record(
                        "station_started",
                        None,
                        Some(&ip),
                        station,
                        Some(serde_json::json!({ "guest": true, "session_id": guest_id })),
                    );
                }
            }
        }

        Some((guest_id, Some(ip)))
    }

    /// Session cookie name. The `__Host-` prefix tells browsers to accept the
    /// cookie only when set securely from the site's own host (no `Domain`
    /// attribute allowed), which defeats cookie tossing from sibling
    /// subdomains served through the same reverse proxy. If the app is served
    /// over plain HTTP during local development, browsers reject `Secure`
    /// cookies, so we fall back to the plain `session_id` name instead.
    /// Session ids are server-generated UUIDs; anything longer cannot be ours.
    const MAX_SESSION_ID_LEN: usize = 128;

    fn extract_session_ids(headers: &HeaderMap) -> Vec<String> {
        let prefixes = [
            format!("{}=", Self::session_cookie_name(headers)),
            "session_id=".to_string(),
            "__Host-session_id=".to_string(),
        ];
        let mut session_ids = Vec::new();
        for value in headers.get_all("cookie") {
            let Ok(cookie_value) = value.to_str() else {
                continue;
            };
            for part in cookie_value.split(';') {
                let part = part.trim();
                for prefix in &prefixes {
                    if let Some(session_id) = part.strip_prefix(prefix.as_str()) {
                        if !session_id.is_empty()
                            && session_id.len() <= Self::MAX_SESSION_ID_LEN
                            && !session_ids.iter().any(|existing| existing == session_id)
                        {
                            session_ids.push(session_id.to_string());
                        }
                    }
                }
            }
        }
        session_ids
    }

    pub fn extract_session_id(headers: &HeaderMap) -> Option<String> {
        Self::extract_session_ids(headers).into_iter().next()
    }

    const SESSION_COOKIE_MAX_AGE: i64 = 365 * 24 * 60 * 60;

    fn cookie_header(session_id: &str, headers: &HeaderMap) -> HeaderValue {
        let cookie_name = Self::session_cookie_name(headers);
        let secure = Self::request_is_secure(headers);
        let expires = (Utc::now() + chrono::Duration::seconds(Self::SESSION_COOKIE_MAX_AGE))
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();

        HeaderValue::from_str(&format!(
            "{cookie_name}={session_id}; Path=/; HttpOnly;{} SameSite=Lax; Max-Age={}; Expires={}",
            if secure { " Secure;" } else { "" },
            Self::SESSION_COOKIE_MAX_AGE,
            expires
        ))
        .expect("valid session cookie")
    }

    fn clear_cookie_header(cookie_name: &str, secure: bool) -> HeaderValue {
        HeaderValue::from_str(&format!(
            "{cookie_name}=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; HttpOnly;{} SameSite=Lax",
            if secure { " Secure;" } else { "" }
        ))
        .expect("valid logout cookie")
    }

    pub fn admin_stats_response(&self, headers: HeaderMap) -> Response {
        let actor = match self.session_user(&headers) {
            Ok(actor) => actor,
            Err(message) => return Self::unauthorized(message),
        };
        if actor.role != UserRole::Admin {
            return Self::unauthorized("Admin access required");
        }

        let response = (StatusCode::OK, Json(self.admin_stats())).into_response();
        Self::no_store(response)
    }

    /// A browser page heartbeat: renew the signed-in session (if any) and
    /// create/refresh entry in the presence tracker. Presence is what admin
    /// stats report, so only heartbeats — page loads and open tabs — ever
    /// show up there.
    pub fn heartbeat_response(
        &self,
        headers: HeaderMap,
        Json(payload): Json<HeartbeatRequest>,
    ) -> Response {
        let now = Utc::now();
        let station_name = payload.station.as_deref().filter(|s| !s.trim().is_empty());

        if let Ok((session_id, user)) = self.session_with_user(&headers) {
            self.renew_session_on_heartbeat(&session_id, &user.username, station_name, now);
            self.track_heartbeat(&headers, Some(&user.username), station_name, now);
            let response =
                (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response();
            return Self::no_store(response);
        }

        if !self.stats_enabled {
            return Self::unauthorized("Admin stats disabled");
        }

        let Some((guest_id, _)) = self.ensure_guest_session(&headers, station_name) else {
            return (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response();
        };
        self.track_heartbeat(&headers, None, station_name, now);

        let response = (
            StatusCode::OK,
            Json(serde_json::json!({ "ok": true, "guest_id": guest_id })),
        )
            .into_response();
        let mut response = Self::no_store(response);
        response.headers_mut().append(
            header::SET_COOKIE,
            Self::guest_cookie_header(&guest_id, &headers),
        );
        response
    }

    /// Extend the signed-in session's rolling lifetime and keep the activity
    /// trail aware of the currently playing station ("station_started" is
    /// logged once per station change).
    fn renew_session_on_heartbeat(
        &self,
        session_id: &str,
        username: &str,
        station_name: Option<&str>,
        now: chrono::DateTime<Utc>,
    ) {
        let mut sessions = self.sessions.write().unwrap();
        if let Some(session) = sessions.get_mut(session_id) {
            session.expires_at = now + chrono::Duration::days(365);
            if station_name.is_some() && session.current_station.as_deref() != station_name {
                self.activity_log.record(
                    "station_started",
                    Some(username),
                    session.ip.as_deref(),
                    station_name,
                    Some(serde_json::json!({ "session_id": session_id })),
                );
            }
            session.current_station = station_name.map(str::to_owned);
        }
        self.write_sessions(&sessions);
    }

    /// Create or refresh this browser's presence from a page heartbeat, if
    /// stats tracking is on at all. A username-less heartbeat comes from a
    /// guest and is keyed by IP alone.
    fn track_heartbeat(
        &self,
        headers: &HeaderMap,
        username: Option<&str>,
        station: Option<&str>,
        now: chrono::DateTime<Utc>,
    ) {
        if self.stats_enabled {
            self.presence.record(
                username,
                username.is_none(),
                &Self::real_ip(headers),
                station,
                now,
            );
        }
    }

    /// Resolve the acting user from request headers, or the failure message
    /// for a 401 response.
    fn session_user(&self, headers: &HeaderMap) -> Result<User, &'static str> {
        self.session_with_user(headers).map(|(_, user)| user)
    }

    fn session_with_user(&self, headers: &HeaderMap) -> Result<(String, User), &'static str> {
        let session_ids = Self::extract_session_ids(headers);
        if session_ids.is_empty() {
            return Err("Not authenticated");
        }

        session_ids
            .into_iter()
            .find_map(|session_id| {
                self.current_user_from_session(&session_id)
                    .map(|user| (session_id, user))
            })
            .ok_or("Session expired")
    }

    fn unauthorized(message: &str) -> Response {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": message })),
        )
            .into_response()
    }

    /// `{"username","role"}` body shared by login/me/create-user responses.
    fn user_json(user: &User) -> serde_json::Value {
        serde_json::json!({
            "username": user.username,
            "role": user.role.as_str()
        })
    }

    /// Mark an auth JSON response as uncacheable.
    fn no_store(response: Response) -> Response {
        let mut response = response;
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
    }

    /// Attach a refreshed session cookie to an auth JSON response.
    fn with_session_cookie(response: Response, session_id: &str, headers: &HeaderMap) -> Response {
        let mut response = response;
        response
            .headers_mut()
            .append(header::SET_COOKIE, Self::cookie_header(session_id, headers));
        response.headers_mut().append(
            header::SET_COOKIE,
            Self::clear_cookie_header(Self::alternate_session_cookie_name(headers), true),
        );
        response
    }

    pub fn login_response(
        &self,
        headers: HeaderMap,
        Json(payload): Json<LoginRequest>,
    ) -> Response {
        match self.login_with_context(&payload.username, &payload.password, &headers) {
            Ok((user, session_id)) => {
                let response = (StatusCode::OK, Json(Self::user_json(&user))).into_response();
                let response = Self::no_store(response);
                Self::with_session_cookie(response, &session_id, &headers)
            }
            Err(message) => (
                if message.starts_with("Too many") {
                    StatusCode::TOO_MANY_REQUESTS
                } else {
                    StatusCode::UNAUTHORIZED
                },
                Json(serde_json::json!({ "error": message })),
            )
                .into_response(),
        }
    }

    pub fn me_response(&self, headers: HeaderMap) -> Response {
        let (session_id, user) = match self.session_with_user(&headers) {
            Ok(session) => session,
            Err(message) => return Self::unauthorized(message),
        };

        let response = (StatusCode::OK, Json(Self::user_json(&user))).into_response();
        let response = Self::no_store(response);
        Self::with_session_cookie(response, &session_id, &headers)
    }

    pub fn logout_response(&self, headers: HeaderMap) -> Response {
        for session_id in Self::extract_session_ids(&headers) {
            self.logout(&session_id);
        }
        if Self::extract_guest_session_id(&headers).is_some() {
            let mut response =
                (StatusCode::OK, Json(serde_json::json!({ "success": true }))).into_response();
            response.headers_mut().append(
                header::SET_COOKIE,
                Self::clear_guest_cookie_header(&headers),
            );
            return Self::no_store(response);
        }

        let response =
            (StatusCode::OK, Json(serde_json::json!({ "success": true }))).into_response();
        let response = Self::no_store(response);
        let mut response = response;
        let secure = Self::request_is_secure(&headers);
        let current_cookie_name = Self::session_cookie_name(&headers);
        response.headers_mut().append(
            header::SET_COOKIE,
            Self::clear_cookie_header(current_cookie_name, secure),
        );
        response.headers_mut().append(
            header::SET_COOKIE,
            Self::clear_cookie_header(Self::alternate_session_cookie_name(&headers), true),
        );
        response
    }

    pub fn create_user_response(
        &self,
        headers: HeaderMap,
        Json(payload): Json<CreateUserRequest>,
    ) -> Response {
        let actor = match self.session_user(&headers) {
            Ok(actor) => actor,
            Err(message) => return Self::unauthorized(message),
        };

        let response =
            match self.create_user(&payload.username, &payload.password, &payload.role, &actor) {
                Ok(user) => (StatusCode::CREATED, Json(Self::user_json(&user))).into_response(),
                Err(message) => (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": message })),
                )
                    .into_response(),
            };
        Self::no_store(response)
    }

    pub fn change_password_response(
        &self,
        headers: HeaderMap,
        Json(payload): Json<ChangePasswordRequest>,
    ) -> Response {
        let actor = match self.session_user(&headers) {
            Ok(actor) => actor,
            Err(message) => return Self::unauthorized(message),
        };

        let response = match self.change_password_for_user(
            &actor.username,
            &payload.old_password,
            &payload.new_password,
        ) {
            Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true }))).into_response(),
            Err(message) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": message })),
            )
                .into_response(),
        };
        Self::no_store(response)
    }
}

#[cfg(test)]
mod tests;
