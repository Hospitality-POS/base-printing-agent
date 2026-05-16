use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePool, Row};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobLine {
    #[serde(rename = "type")]
    pub line_type: String,
    pub text: String,
    pub qty: Option<u32>,
    pub price: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintJob {
    pub job_id: String,
    pub branch_id: String,
    pub destination_zone: String,
    pub content_type: String,
    pub lines: Vec<PrintJobLine>,
    pub cut_paper: bool,
    pub priority: String,
    pub created_at: String,
}

pub async fn init_db(db_url: &str) -> Result<SqlitePool, String> {
    let pool = SqlitePool::connect(db_url)
        .await
        .map_err(|e| e.to_string())?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS print_jobs (
            job_id      TEXT PRIMARY KEY,
            payload     TEXT NOT NULL,
            status      TEXT NOT NULL DEFAULT 'pending',
            created_at  TEXT NOT NULL,
            printed_at  TEXT
        )
        "#,
    )
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(pool)
}

pub async fn insert_job(pool: &SqlitePool, job: &PrintJob) -> Result<(), String> {
    let payload = serde_json::to_string(job).map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT OR IGNORE INTO print_jobs (job_id, payload, status, created_at) VALUES (?, ?, 'pending', ?)"
    )
    .bind(&job.job_id)
    .bind(&payload)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn mark_printed(pool: &SqlitePool, job_id: &str) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE print_jobs SET status = 'printed', printed_at = ? WHERE job_id = ?",
    )
    .bind(&now)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn mark_failed(pool: &SqlitePool, job_id: &str) -> Result<(), String> {
    sqlx::query("UPDATE print_jobs SET status = 'failed' WHERE job_id = ?")
        .bind(job_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn is_already_printed(pool: &SqlitePool, job_id: &str) -> bool {
    let result = sqlx::query(
        "SELECT status FROM print_jobs WHERE job_id = ? AND status = 'printed'",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await;

    matches!(result, Ok(Some(_)))
}

pub async fn get_pending_jobs(pool: &SqlitePool) -> Vec<PrintJob> {
    let rows = sqlx::query("SELECT payload FROM print_jobs WHERE status = 'pending' ORDER BY created_at ASC")
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    rows.iter()
        .filter_map(|row| {
            let payload: String = row.try_get("payload").ok()?;
            serde_json::from_str(&payload).ok()
        })
        .collect()
}

pub async fn get_pending_count(pool: &SqlitePool) -> i64 {
    sqlx::query("SELECT COUNT(*) as cnt FROM print_jobs WHERE status = 'pending'")
        .fetch_one(pool)
        .await
        .map(|row| row.try_get::<i64, _>("cnt").unwrap_or(0))
        .unwrap_or(0)
}

pub async fn get_pending_count_for_agent(pool: &SqlitePool, agent_id: &str) -> Result<i64, sqlx::Error> {
    sqlx::query("SELECT COUNT(*) as cnt FROM print_jobs WHERE status = 'pending' AND agent_id = ?")
        .bind(agent_id)
        .fetch_one(pool)
        .await
        .map(|row| row.try_get::<i64, _>("cnt").unwrap_or(0))
}
