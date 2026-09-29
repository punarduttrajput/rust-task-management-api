use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    auth::{self, AuthUser},
    cache::my_tasks_key,
    error::{AppError, AppResult},
    models::{LoginChallenge, MyTasks, Role, TaskPriority, TaskStatus, TaskSummary, TaskView, User, UserSummary},
    AppState,
};

pub fn router(state: AppState) -> Router {
    let mut app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/login", post(login))
        .route("/auth/verify-2fa", post(verify_2fa))
        .route("/tasks", post(create_task).get(list_tasks))
        .route("/tasks/assign", post(assign_tasks))
        .route("/tasks/view-my-tasks", get(view_my_tasks))
        .route("/tasks/{id}", patch(update_task));
    if state.config.dev_mode {
        app = app
            .route("/seed/users", post(seed_users))
            .route("/dev/email-logs/latest", get(latest_email));
    }
    app.with_state(state)
}

const TASK_VIEW_SELECT: &str = "SELECT t.id, t.title, t.description, t.status, t.priority, \
     u.email AS assigned_to, t.created_at, t.updated_at \
     FROM tasks t LEFT JOIN users u ON u.id = t.assigned_to_id";

async fn find_user_by_email(state: &AppState, email: &str) -> AppResult<Option<User>> {
    Ok(sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = ? COLLATE NOCASE")
        .bind(email)
        .fetch_optional(&state.db)
        .await?)
}

async fn fetch_task_view(state: &AppState, id: &str) -> AppResult<TaskView> {
    sqlx::query_as::<_, TaskView>(&format!("{TASK_VIEW_SELECT} WHERE t.id = ?"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("task {id}")))
}

async fn invalidate_users(state: &AppState, user_ids: impl IntoIterator<Item = String>) {
    for id in user_ids {
        state.my_tasks_cache.invalidate(&my_tasks_key(&id)).await;
    }
}

// ---------- dev ----------

const SEED_USERS: [(&str, &str, &str, Role); 2] = [
    ("Admin", "admin@example.com", "Admin@12345", Role::Admin),
    ("James Bond", "jamesbond@example.com", "Bond@007007", Role::Staff),
];

async fn seed_users(State(state): State<AppState>) -> AppResult<(StatusCode, Json<Value>)> {
    let mut out = Vec::new();
    for (name, email, password, role) in SEED_USERS {
        let user = match find_user_by_email(&state, email).await? {
            Some(u) => u,
            None => {
                let hashed = tokio::task::spawn_blocking(move || auth::hash_password(password))
                    .await
                    .map_err(|e| AppError::Internal(e.to_string()))??;
                let now = Utc::now();
                let id = uuid::Uuid::new_v4().to_string();
                sqlx::query("INSERT INTO users (id, full_name, email, hashed_password, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
                    .bind(&id).bind(name).bind(email).bind(&hashed).bind(role).bind(now).bind(now)
                    .execute(&state.db)
                    .await?;
                find_user_by_email(&state, email).await?.expect("just inserted")
            }
        };
        out.push(json!({ "id": user.id, "full_name": user.full_name, "email": user.email, "role": user.role }));
    }
    Ok((StatusCode::CREATED, Json(json!({ "users": out }))))
}

#[derive(Deserialize)]
struct LatestEmailQuery {
    to: Option<String>,
}

async fn latest_email(State(state): State<AppState>, Query(q): Query<LatestEmailQuery>) -> AppResult<Json<Value>> {
    let email = state
        .mailer
        .latest(q.to.as_deref())
        .await
        .ok_or_else(|| AppError::NotFound("email".into()))?;
    Ok(Json(json!(email)))
}

// ---------- auth ----------

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

async fn login(State(state): State<AppState>, Json(req): Json<LoginRequest>) -> AppResult<Json<Value>> {
    let invalid = || AppError::unauthorized("invalid_credentials", "invalid email or password");
    let user = find_user_by_email(&state, &req.email).await?.ok_or_else(invalid)?;
    let hash = user.hashed_password.clone();
    let ok = tokio::task::spawn_blocking(move || auth::verify_password(&req.password, &hash))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    if !ok {
        return Err(invalid());
    }

    let challenge_id = uuid::Uuid::new_v4().to_string();
    let code = auth::generate_otp();
    let now = Utc::now();
    let expires_at = now + Duration::seconds(state.config.otp_ttl_secs);
    sqlx::query("INSERT INTO login_challenges (id, user_id, code_hash, attempts, expires_at, created_at) VALUES (?, ?, ?, 0, ?, ?)")
        .bind(&challenge_id)
        .bind(&user.id)
        .bind(auth::hash_otp(&state.config.otp_secret, &challenge_id, &code))
        .bind(expires_at)
        .bind(now)
        .execute(&state.db)
        .await?;
    state.mailer.send_login_code(&state.db, &user.email, &challenge_id, &code).await?;

    Ok(Json(json!({
        "login_challenge_id": challenge_id,
        "expires_at": expires_at,
        "message": "A verification code has been sent to your email."
    })))
}

#[derive(Deserialize)]
struct VerifyRequest {
    login_challenge_id: String,
    code: String,
}

async fn verify_2fa(State(state): State<AppState>, Json(req): Json<VerifyRequest>) -> AppResult<Json<Value>> {
    let ch = sqlx::query_as::<_, LoginChallenge>("SELECT id, user_id, code_hash, attempts, expires_at, consumed_at FROM login_challenges WHERE id = ?")
        .bind(&req.login_challenge_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::unauthorized("invalid_challenge", "unknown login challenge"))?;

    if ch.consumed_at.is_some() {
        return Err(AppError::unauthorized("code_already_used", "verification code already used"));
    }
    if ch.expires_at <= Utc::now() {
        return Err(AppError::unauthorized("code_expired", "verification code expired"));
    }
    if ch.attempts >= state.config.otp_max_attempts {
        return Err(AppError::unauthorized("too_many_attempts", "too many failed attempts; log in again"));
    }
    if !auth::verify_otp(&state.config.otp_secret, &ch.id, req.code.trim(), &ch.code_hash) {
        sqlx::query("UPDATE login_challenges SET attempts = attempts + 1 WHERE id = ?")
            .bind(&ch.id)
            .execute(&state.db)
            .await?;
        return Err(AppError::unauthorized("invalid_code", "invalid verification code"));
    }

    // Atomic consume: only one concurrent request can win.
    let consumed = sqlx::query("UPDATE login_challenges SET consumed_at = ? WHERE id = ? AND consumed_at IS NULL")
        .bind(Utc::now())
        .bind(&ch.id)
        .execute(&state.db)
        .await?;
    if consumed.rows_affected() != 1 {
        return Err(AppError::unauthorized("code_already_used", "verification code already used"));
    }

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(&ch.user_id)
        .fetch_one(&state.db)
        .await?;
    let token = auth::issue_jwt(&state.config.jwt_secret, state.config.jwt_ttl_secs, &user.id, &user.email, user.role)?;
    Ok(Json(json!({
        "access_token": token,
        "token_type": "Bearer",
        "expires_in": state.config.jwt_ttl_secs,
        "user": { "id": user.id, "email": user.email, "full_name": user.full_name, "role": user.role }
    })))
}

// ---------- tasks ----------

#[derive(Deserialize)]
struct CreateTaskRequest {
    title: String,
    #[serde(default)]
    description: String,
    status: Option<TaskStatus>,
    priority: Option<TaskPriority>,
}

async fn create_task(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateTaskRequest>,
) -> AppResult<(StatusCode, Json<TaskView>)> {
    user.require_admin()?;
    if req.title.trim().is_empty() {
        return Err(AppError::BadRequest("title must not be empty".into()));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();
    sqlx::query("INSERT INTO tasks (id, title, description, status, priority, created_by_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(req.title.trim())
        .bind(&req.description)
        .bind(req.status.unwrap_or(TaskStatus::Todo))
        .bind(req.priority.unwrap_or(TaskPriority::Medium))
        .bind(&user.0.sub)
        .bind(now)
        .bind(now)
        .execute(&state.db)
        .await?;
    Ok((StatusCode::CREATED, Json(fetch_task_view(&state, &id).await?)))
}

async fn list_tasks(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Vec<TaskView>>> {
    user.require_admin()?;
    let tasks = sqlx::query_as::<_, TaskView>(&format!("{TASK_VIEW_SELECT} ORDER BY t.created_at"))
        .fetch_all(&state.db)
        .await?;
    Ok(Json(tasks))
}

#[derive(Deserialize)]
struct AssignRequest {
    task_ids: Vec<String>,
    assignee_email: Option<String>,
    assignee_id: Option<String>,
}

async fn assign_tasks(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<AssignRequest>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    if req.task_ids.is_empty() {
        return Err(AppError::BadRequest("task_ids must not be empty".into()));
    }
    let assignee = match (&req.assignee_id, &req.assignee_email) {
        (Some(id), _) => sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.db)
            .await?,
        (None, Some(email)) => find_user_by_email(&state, email).await?,
        (None, None) => return Err(AppError::BadRequest("assignee_email or assignee_id required".into())),
    }
    .ok_or_else(|| AppError::NotFound("assignee".into()))?;

    let mut affected = vec![assignee.id.clone()];
    let mut tx = state.db.begin().await?;
    for task_id in &req.task_ids {
        let previous: Option<Option<String>> = sqlx::query_scalar("SELECT assigned_to_id FROM tasks WHERE id = ?")
            .bind(task_id)
            .fetch_optional(&mut *tx)
            .await?;
        let previous = previous.ok_or_else(|| AppError::NotFound(format!("task {task_id}")))?;
        affected.extend(previous);
        sqlx::query("UPDATE tasks SET assigned_to_id = ?, updated_at = ? WHERE id = ?")
            .bind(&assignee.id)
            .bind(Utc::now())
            .bind(task_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    invalidate_users(&state, affected).await;

    Ok(Json(json!({
        "assigned_to": assignee.email,
        "task_ids": req.task_ids,
        "assigned_count": req.task_ids.len()
    })))
}

#[derive(Deserialize)]
struct UpdateTaskRequest {
    title: Option<String>,
    description: Option<String>,
    status: Option<TaskStatus>,
    priority: Option<TaskPriority>,
}

async fn update_task(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> AppResult<Json<TaskView>> {
    user.require_admin()?;
    let assignee: Option<String> = sqlx::query_scalar::<_, Option<String>>("SELECT assigned_to_id FROM tasks WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("task {id}")))?;
    sqlx::query(
        "UPDATE tasks SET title = COALESCE(?, title), description = COALESCE(?, description), \
         status = COALESCE(?, status), priority = COALESCE(?, priority), updated_at = ? WHERE id = ?",
    )
    .bind(req.title)
    .bind(req.description)
    .bind(req.status)
    .bind(req.priority)
    .bind(Utc::now())
    .bind(&id)
    .execute(&state.db)
    .await?;
    invalidate_users(&state, assignee).await;
    Ok(Json(fetch_task_view(&state, &id).await?))
}

async fn view_my_tasks(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    let key = my_tasks_key(&user.0.sub);
    let (data, hit) = match state.my_tasks_cache.get(&key).await {
        Some(data) => (data, true),
        None => {
            let me = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
                .bind(&user.0.sub)
                .fetch_optional(&state.db)
                .await?
                .ok_or_else(|| AppError::unauthorized("invalid_token", "user no longer exists"))?;
            let tasks = sqlx::query_as::<_, TaskView>(&format!(
                "{TASK_VIEW_SELECT} WHERE t.assigned_to_id = ? \
                 ORDER BY CASE t.priority WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END, t.created_at"
            ))
            .bind(&me.id)
            .fetch_all(&state.db)
            .await?;
            let data = MyTasks {
                user: UserSummary { email: me.email, role: me.role },
                summary: TaskSummary { total_assigned_tasks: tasks.len() },
                tasks,
            };
            state.my_tasks_cache.set(key, data.clone()).await;
            (data, false)
        }
    };
    let mut body = serde_json::to_value(data).map_err(|e| AppError::Internal(e.to_string()))?;
    body["cache"] = json!({ "hit": hit });
    Ok(Json(body))
}
