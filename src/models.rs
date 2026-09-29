use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Staff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum TaskPriority {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub full_name: String,
    pub email: String,
    pub hashed_password: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LoginChallenge {
    pub id: String,
    pub user_id: String,
    pub code_hash: String,
    pub attempts: i64,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
}

/// Task as returned by the API: assignee resolved to an email.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TaskView {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub assigned_to: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserSummary {
    pub email: String,
    pub role: Role,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskSummary {
    pub total_assigned_tasks: usize,
}

/// The cacheable part of the view-my-tasks response.
#[derive(Debug, Clone, Serialize)]
pub struct MyTasks {
    pub user: UserSummary,
    pub tasks: Vec<TaskView>,
    pub summary: TaskSummary,
}
