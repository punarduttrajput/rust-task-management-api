use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
pub struct SentEmail {
    pub id: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub code: String,
    pub login_challenge_id: String,
    pub sent_at: DateTime<Utc>,
}

/// Development mailer: prints to the console and keeps the plaintext message in
/// memory only. The `email_logs` table records metadata without the code.
#[derive(Default)]
pub struct DevMailer {
    outbox: Mutex<Vec<SentEmail>>,
}

impl DevMailer {
    pub async fn send_login_code(
        &self,
        db: &SqlitePool,
        to: &str,
        challenge_id: &str,
        code: &str,
    ) -> Result<(), sqlx::Error> {
        let email = SentEmail {
            id: uuid::Uuid::new_v4().to_string(),
            to: to.to_string(),
            subject: "Your login verification code".into(),
            body: format!("Your verification code is {code}. It expires in 5 minutes."),
            code: code.to_string(),
            login_challenge_id: challenge_id.to_string(),
            sent_at: Utc::now(),
        };
        sqlx::query("INSERT INTO email_logs (id, to_email, subject, login_challenge_id, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&email.id)
            .bind(&email.to)
            .bind(&email.subject)
            .bind(challenge_id)
            .bind(email.sent_at)
            .execute(db)
            .await?;
        tracing::info!(to = %email.to, challenge_id, code, "[DEV EMAIL] 2FA code sent");
        self.outbox.lock().await.push(email);
        Ok(())
    }

    pub async fn latest(&self, to: Option<&str>) -> Option<SentEmail> {
        let outbox = self.outbox.lock().await;
        outbox
            .iter()
            .rev()
            .find(|e| to.is_none_or(|t| e.to.eq_ignore_ascii_case(t)))
            .cloned()
    }
}
