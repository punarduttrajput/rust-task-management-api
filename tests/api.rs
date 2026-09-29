use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use task_api::{build_state, config::Config, router, AppState};
use tower::ServiceExt;

struct TestApp {
    app: Router,
    state: AppState,
}

impl TestApp {
    async fn new() -> Self {
        let state = build_state(Config::for_tests()).await.unwrap();
        Self { app: router(state.clone()), state }
    }

    async fn call(&self, method: &str, uri: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri).header("content-type", "application/json");
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let body = body.map(|b| Body::from(b.to_string())).unwrap_or_else(Body::empty);
        let res = self.app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn start_login(&self, email: &str, password: &str) -> (String, String) {
        let (s, body) = self.call("POST", "/auth/login", None, Some(json!({"email": email, "password": password}))).await;
        assert_eq!(s, StatusCode::OK, "{body}");
        assert!(body.get("access_token").is_none(), "login must not return a JWT");
        let challenge = body["login_challenge_id"].as_str().unwrap().to_string();
        let (_, mail) = self.call("GET", &format!("/dev/email-logs/latest?to={email}"), None, None).await;
        assert_eq!(mail["login_challenge_id"], challenge.as_str());
        (challenge, mail["code"].as_str().unwrap().to_string())
    }

    async fn verify(&self, challenge: &str, code: &str) -> (StatusCode, Value) {
        self.call("POST", "/auth/verify-2fa", None, Some(json!({"login_challenge_id": challenge, "code": code}))).await
    }

    async fn login(&self, email: &str, password: &str) -> String {
        let (c, code) = self.start_login(email, password).await;
        let (s, body) = self.verify(&c, &code).await;
        assert_eq!(s, StatusCode::OK, "{body}");
        body["access_token"].as_str().unwrap().to_string()
    }

    /// Seeds users, logs both in, creates 5 tasks, assigns 3 to James.
    async fn setup_workflow(&self) -> (String, String, Vec<String>) {
        let (s, _) = self.call("POST", "/seed/users", None, None).await;
        assert_eq!(s, StatusCode::CREATED);
        let admin = self.login("admin@example.com", "Admin@12345").await;
        let mut ids = Vec::new();
        for (i, p) in ["high", "medium", "low", "medium", "low"].iter().enumerate() {
            let (s, t) = self
                .call("POST", "/tasks", Some(&admin), Some(json!({"title": format!("Task {}", i + 1), "priority": p})))
                .await;
            assert_eq!(s, StatusCode::CREATED, "{t}");
            ids.push(t["id"].as_str().unwrap().to_string());
        }
        let (s, body) = self
            .call("POST", "/tasks/assign", Some(&admin), Some(json!({"task_ids": &ids[..3], "assignee_email": "jamesbond@example.com"})))
            .await;
        assert_eq!(s, StatusCode::OK, "{body}");
        let james = self.login("jamesbond@example.com", "Bond@007007").await;
        (admin, james, ids)
    }
}

#[tokio::test]
async fn full_validation_workflow() {
    let t = TestApp::new().await;
    let (_admin, james, _) = t.setup_workflow().await;

    let (s, _) = t.call("POST", "/tasks", Some(&james), Some(json!({"title": "nope"}))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (s, first) = t.call("GET", "/tasks/view-my-tasks", Some(&james), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(first["user"], json!({"email": "jamesbond@example.com", "role": "staff"}));
    assert_eq!(first["summary"]["total_assigned_tasks"], 3);
    let tasks = first["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 3);
    let priorities: Vec<_> = tasks.iter().map(|t| t["priority"].as_str().unwrap()).collect();
    assert_eq!(priorities, ["high", "medium", "low"]);
    assert!(tasks.iter().all(|t| t["assigned_to"] == "jamesbond@example.com" && t["status"] == "todo"));
    assert_eq!(first["cache"]["hit"], false);

    let (_, second) = t.call("GET", "/tasks/view-my-tasks", Some(&james), None).await;
    assert_eq!(second["cache"]["hit"], true);
    assert_eq!(second["tasks"], first["tasks"]);
}

#[tokio::test]
async fn wrong_reused_and_expired_codes_are_rejected() {
    let t = TestApp::new().await;
    t.call("POST", "/seed/users", None, None).await;

    let (s, _) = t.call("POST", "/auth/login", None, Some(json!({"email": "admin@example.com", "password": "bad"}))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // wrong, then correct, then reuse
    let (c, code) = t.start_login("admin@example.com", "Admin@12345").await;
    let wrong = if code == "000000" { "111111" } else { "000000" };
    let (s, body) = t.verify(&c, wrong).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("invalid_code")));
    let (s, _) = t.verify(&c, &code).await;
    assert_eq!(s, StatusCode::OK);
    let (s, body) = t.verify(&c, &code).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("code_already_used")));

    // expired
    let (c, code) = t.start_login("admin@example.com", "Admin@12345").await;
    sqlx::query("UPDATE login_challenges SET expires_at = ? WHERE id = ?")
        .bind(chrono::Utc::now() - chrono::Duration::seconds(1))
        .bind(&c)
        .execute(&t.state.db)
        .await
        .unwrap();
    let (s, body) = t.verify(&c, &code).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("code_expired")));

    // code is not stored in plaintext
    let hash: String = sqlx::query_scalar("SELECT code_hash FROM login_challenges WHERE id = ?")
        .bind(&c)
        .fetch_one(&t.state.db)
        .await
        .unwrap();
    assert_ne!(hash, code);
}

#[tokio::test]
async fn staff_cannot_assign_and_anonymous_is_rejected() {
    let t = TestApp::new().await;
    let (_, james, ids) = t.setup_workflow().await;
    let (s, _) = t
        .call("POST", "/tasks/assign", Some(&james), Some(json!({"task_ids": [ids[3]], "assignee_email": "jamesbond@example.com"})))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = t.call("GET", "/tasks/view-my-tasks", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn assignment_and_update_invalidate_cache() {
    let t = TestApp::new().await;
    let (admin, james, ids) = t.setup_workflow().await;
    t.call("GET", "/tasks/view-my-tasks", Some(&james), None).await;
    let (_, warm) = t.call("GET", "/tasks/view-my-tasks", Some(&james), None).await;
    assert_eq!(warm["cache"]["hit"], true);

    // assign a 4th task -> cache invalidated
    t.call("POST", "/tasks/assign", Some(&admin), Some(json!({"task_ids": [ids[3]], "assignee_email": "jamesbond@example.com"})))
        .await;
    let (_, after_assign) = t.call("GET", "/tasks/view-my-tasks", Some(&james), None).await;
    assert_eq!(after_assign["cache"]["hit"], false);
    assert_eq!(after_assign["summary"]["total_assigned_tasks"], 4);

    // update a task -> cache invalidated and new status visible
    let (s, _) = t.call("PATCH", &format!("/tasks/{}", ids[0]), Some(&admin), Some(json!({"status": "done"}))).await;
    assert_eq!(s, StatusCode::OK);
    let (_, after_update) = t.call("GET", "/tasks/view-my-tasks", Some(&james), None).await;
    assert_eq!(after_update["cache"]["hit"], false);
    assert!(after_update["tasks"].as_array().unwrap().iter().any(|t| t["status"] == "done"));
}
