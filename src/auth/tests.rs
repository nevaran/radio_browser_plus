use super::*;

#[test]
fn create_user_for_reader_is_allowed_for_admin() {
    let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
    let auth = AuthService::new(&temp_dir);
    auth.ensure_default_admin("admin", "secret");

    let admin = auth
        .current_user_from_session(&auth.login("admin", "secret").unwrap().1)
        .unwrap();

    let created = auth
        .create_user("reader01", "hello", "reader", &admin)
        .unwrap();
    assert_eq!(created.role, UserRole::Reader);
    // favorites_path_for_user provisions the per-user directory; the
    // favorites file itself is created on first write.
    let fav_path = auth.favorites_path_for_user("reader01");
    assert!(fav_path.parent().is_some_and(|dir| dir.exists()));

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn admin_required_for_user_creation() {
    let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
    let auth = AuthService::new(&temp_dir);
    auth.ensure_default_admin("admin", "secret");

    let reader_user = auth
        .create_user_internal("reader", "pass", UserRole::Reader, false)
        .unwrap();
    let session = auth.login("reader", "pass").unwrap().1;
    let actor = auth.current_user_from_session(&session).unwrap();

    assert_ne!(actor.role, UserRole::Admin);
    assert!(auth
        .create_user("reader2", "pass", "reader", &reader_user)
        .is_err());

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn user_can_change_their_own_password() {
    let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
    let auth = AuthService::new(&temp_dir);
    auth.ensure_default_admin("admin", "secret");

    auth.change_password_for_user("admin", "secret", "new-secret")
        .unwrap();
    assert!(auth.login("admin", "new-secret").is_ok());
    assert!(auth.login("admin", "secret").is_err());

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn unsafe_usernames_are_rejected() {
    for bad in [
        "",
        "   ",
        "../evil",
        "../../etc/x",
        "user/name",
        "user\\name",
        "a".repeat(65).as_str(),
        "user name",
        "user@host",
    ] {
        assert!(
            !is_valid_username(&bad.to_lowercase()),
            "should reject {bad:?}"
        );
    }
    for good in ["admin", "reader01", "user.name_1-2", "a"] {
        assert!(is_valid_username(good), "should accept {good:?}");
    }
}

#[test]
fn traversal_username_cannot_create_user() {
    let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
    let auth = AuthService::new(&temp_dir);
    auth.ensure_default_admin("admin", "secret");
    let admin = auth
        .current_user_from_session(&auth.login("admin", "secret").unwrap().1)
        .unwrap();

    assert!(auth
        .create_user("../../evil", "pass", "reader", &admin)
        .is_err());
    assert!(auth.create_user("a/b", "pass", "reader", &admin).is_err());
    // Nothing escaped the data directory.
    let entries: Vec<_> = fs::read_dir(&temp_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(!entries
        .iter()
        .any(|name| name.to_string_lossy().contains("evil")));

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn password_change_revokes_sessions() {
    let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
    let auth = AuthService::new(&temp_dir);
    auth.ensure_default_admin("admin", "secret");

    let (_, session_id) = auth.login("admin", "secret").unwrap();
    assert!(auth.current_user_from_session(&session_id).is_some());

    auth.change_password_for_user("admin", "secret", "brand-new")
        .unwrap();
    assert!(auth.current_user_from_session(&session_id).is_none());

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn insecure_local_http_sessions_are_accepted() {
    let mut headers = HeaderMap::new();
    headers.insert("cookie", HeaderValue::from_static("session_id=abc123"));
    assert_eq!(AuthService::extract_session_id(&headers), Some("abc123".to_string()));
}

#[test]
fn secure_proxied_sessions_still_use_host_cookie() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
    headers.insert(
        "cookie",
        HeaderValue::from_static("__Host-session_id=abc123"),
    );
    assert_eq!(AuthService::extract_session_id(&headers), Some("abc123".to_string()));
}

#[test]
fn me_uses_valid_session_when_stale_cookie_is_also_present() {
    let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
    let auth = AuthService::new(&temp_dir);
    auth.ensure_default_admin("admin", "secret");
    let (_, session_id) = auth.login("admin", "secret").unwrap();

    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
    headers.insert(
        "cookie",
        HeaderValue::from_str(&format!("__Host-session_id=stale; session_id={session_id}"))
            .unwrap(),
    );

    let response = auth.me_response(headers);
    assert_eq!(response.status(), StatusCode::OK);

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn admin_stats_only_counts_users_that_sent_a_page_heartbeat() {
    let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
    let mut auth = AuthService::new(&temp_dir);
    auth.stats_enabled = true;
    auth.ensure_default_admin("admin", "secret");
    let (_, session_id) = auth.login("admin", "secret").unwrap();

    // Signing in alone is not enough: the activity timer starts with a
    // page-load heartbeat, so other API traffic stays invisible.
    assert!(auth.admin_stats().active_users.is_empty());

    let mut headers = HeaderMap::new();
    headers.insert(
        "cookie",
        HeaderValue::from_str(&format!("session_id={session_id}")).expect("valid cookie"),
    );
    let payload = Json(HeartbeatRequest {
        station: Some("Radio Nova".to_string()),
    });
    assert_eq!(
        auth.heartbeat_response(headers.clone(), payload)
            .status(),
        StatusCode::OK
    );

    let stats = auth.admin_stats();
    assert_eq!(stats.active_users.len(), 1);
    let active = &stats.active_users[0];
    assert_eq!(active.username.as_deref(), Some("admin"));
    assert_eq!(active.station.as_deref(), Some("Radio Nova"));
    assert!(!active.guest);
    assert_eq!(active.active_seconds, 0);

    // A stale tracker entry (no heartbeat for over the timeout window)
    // disappears instead of reporting made-up activity time.
    assert_eq!(auth.presence.entries.read().unwrap().len(), 1);
    auth.presence
        .entries
        .write()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()
        .last_seen = Utc::now() - chrono::Duration::minutes(6);
    assert!(auth.admin_stats().active_users.is_empty());

    // The next heartbeat after the silence starts a fresh timer.
    let payload = Json(HeartbeatRequest { station: None });
    assert_eq!(
        auth.heartbeat_response(headers.clone(), payload)
            .status(),
        StatusCode::OK
    );
    let stats = auth.admin_stats();
    assert_eq!(stats.active_users.len(), 1);
    assert_eq!(stats.active_users[0].active_seconds, 0);
    assert_eq!(stats.active_users[0].station, None);

    // Logging out drops the entry immediately.
    auth.logout(&session_id);
    assert!(auth.admin_stats().active_users.is_empty());

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn guest_activity_tracks_one_timer_per_ip() {
    let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
    let mut auth = AuthService::new(&temp_dir);
    auth.stats_enabled = true;

    // No guests have loaded the page yet.
    assert!(auth.admin_stats().guests.is_empty());

    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.7"));
    let payload = Json(HeartbeatRequest {
        station: Some("Jazz FM".to_string()),
    });
    assert_eq!(
        auth.heartbeat_response(headers.clone(), payload)
            .status(),
        StatusCode::OK
    );

    let stats = auth.admin_stats();
    assert_eq!(stats.guests.len(), 1);
    let guest = &stats.guests[0];
    assert!(guest.guest);
    assert_eq!(guest.username, None);
    assert_eq!(guest.ip, "203.0.113.7");
    assert_eq!(guest.station.as_deref(), Some("Jazz FM"));
    assert_eq!(guest.active_seconds, 0);

    // A second guest browser on the same IP joins the same per-IP timer
    // instead of creating a duplicate row.
    let mut headers_same_ip = HeaderMap::new();
    headers_same_ip.insert(
        "x-forwarded-for",
        HeaderValue::from_static("203.0.113.7"),
    );
    let payload = Json(HeartbeatRequest { station: None });
    auth.heartbeat_response(headers_same_ip, payload);
    assert_eq!(auth.admin_stats().guests.len(), 1);

    // Idling past the timeout prunes the entry and resets the timer for
    // the next heartbeat.
    assert_eq!(auth.presence.entries.read().unwrap().len(), 1);
    auth.presence
        .entries
        .write()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()
        .last_seen = Utc::now() - chrono::Duration::minutes(6);
    assert!(auth.admin_stats().guests.is_empty());

    let payload = Json(HeartbeatRequest { station: None });
    auth.heartbeat_response(headers.clone(), payload);
    let stats = auth.admin_stats();
    assert_eq!(stats.guests.len(), 1);
    assert_eq!(stats.guests[0].active_seconds, 0);

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn enabled_activity_log_is_created_and_receives_events() {
    let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
    let mut auth = AuthService::new(&temp_dir);
    auth.activity_log.retention_days = 1;

    auth.activity_log.initialize();
    assert!(auth.activity_log.path.exists());
    auth.activity_log
        .record("session_start", Some("admin"), None, None, None);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&auth.activity_log.path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    let contents = fs::read_to_string(&auth.activity_log.path).unwrap();
    assert!(contents.contains("Session started | User: admin"));
    assert!(!contents.trim_start().starts_with('{'));

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn session_remains_valid_after_service_restart() {
    let temp_dir = std::env::temp_dir().join(format!("radio-auth-{}", uuid::Uuid::new_v4()));
    let auth = AuthService::new(&temp_dir);
    auth.ensure_default_admin("admin", "secret");

    let (_, session_id) = auth.login("admin", "secret").unwrap();

    let restarted = AuthService::new(&temp_dir);
    assert_eq!(
        restarted
            .current_user_from_session(&session_id)
            .unwrap()
            .username,
        "admin"
    );

    let _ = fs::remove_dir_all(temp_dir);
}
