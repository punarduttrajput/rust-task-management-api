# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Rust (Axum 0.8 + SQLx 0.8/SQLite) task-management API with email 2FA, JWT, admin/staff RBAC and a per-user cache. Spec: `docs/PRD.md`, design: `docs/TRD.md`.

## Commands
- Build/run: `cargo run` (http://127.0.0.1:8080; config from env/`.env`, see `.env.example`)
- Tests: `cargo test`; single test: `cargo test --test api <name>` or `cargo test <unit_name>`
- E2E against a running server: `powershell -File scripts/validate.ps1`

## Architecture
- `lib.rs` builds `AppState` (pool, config, `DevMailer`, `TtlCache<MyTasks>`) and runs embedded migrations; `main.rs` and `tests/api.rs` both use `build_state` + `router`. Tests use `Config::for_tests()` (in-memory SQLite, pool size 1).
- `routes.rs` holds all handlers. `/seed/*` and `/dev/*` are mounted only when `APP_ENV=development`.
- Auth: the `AuthUser` extractor (`auth.rs`) decodes the Bearer JWT; handlers call `user.require_admin()` for 403.
- 2FA: codes are stored as HMAC bound to the challenge id. Plaintext lives only in the in-memory dev outbox (`email.rs`); the `email_logs` table is metadata-only. Keep it that way.
- Cache: any handler that changes a task's assignee or fields must invalidate `my_tasks_key(user_id)` for both old and new assignees **after** the DB write.
- Queries use runtime `sqlx::query_as` (no `DATABASE_URL` needed at compile time). Enums map to lowercase/snake_case TEXT via `sqlx::Type`.
- `Cargo.toml` optimises `argon2`/`blake2` in the dev profile; without it, debug tests are slow.
