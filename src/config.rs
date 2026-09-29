use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    pub jwt_secret: String,
    pub jwt_ttl_secs: i64,
    pub otp_secret: String,
    pub otp_ttl_secs: i64,
    pub otp_max_attempts: i64,
    pub cache_ttl_secs: u64,
    pub dev_mode: bool,
}

fn var_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn num_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_url: var_or("DATABASE_URL", "sqlite://task_api.db"),
            bind_addr: var_or("BIND_ADDR", "127.0.0.1:8080"),
            jwt_secret: var_or("JWT_SECRET", "dev-jwt-secret-change-me"),
            jwt_ttl_secs: num_or("JWT_TTL_SECONDS", 3600),
            otp_secret: var_or("OTP_SECRET", "dev-otp-secret-change-me"),
            otp_ttl_secs: num_or("OTP_TTL_SECONDS", 300),
            otp_max_attempts: num_or("OTP_MAX_ATTEMPTS", 5),
            cache_ttl_secs: num_or("CACHE_TTL_SECONDS", 60),
            dev_mode: var_or("APP_ENV", "development") == "development",
        }
    }

    /// Config for integration tests: isolated in-memory database.
    pub fn for_tests() -> Self {
        Self {
            database_url: "sqlite::memory:".into(),
            ..Self::from_env()
        }
    }
}
