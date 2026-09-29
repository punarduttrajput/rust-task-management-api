# TRD — Task Management API

## Stack
Axum 0.8 · Tokio · SQLx 0.8 (SQLite, embedded migrations) · Argon2id · jsonwebtoken (HS256) · HMAC-SHA256 for OTP hashing · in-memory TTL cache.

## Modules (`src/`)
| Module | Responsibility |
|---|---|
| `config.rs` | Env config (`.env` supported) |
| `error.rs` | `AppError` → JSON `{error:{code,message}}` + status |
| `models.rs` | DB rows, enums (`Role`, `TaskStatus`, `TaskPriority`), DTOs |
| `auth.rs` | Password hashing, OTP generate/hash/verify, JWT, `AuthUser` extractor, `require_admin` |
| `email.rs` | Dev mailer: console log + in-memory outbox + `email_logs` metadata row |
| `cache.rs` | `TtlCache<V>`: `RwLock<HashMap<key,(expires,V)>>` |
| `routes.rs` | Handlers + router |
| `lib.rs` | `AppState`, `build_state`, `router` (used by `main` and tests) |

## Schema (`migrations/0001_init.sql`)
- `users(id, full_name, email UNIQUE, hashed_password, role CHECK, created_at, updated_at)`
- `tasks(id, title, description, status CHECK, priority CHECK, created_by_id FK, assigned_to_id FK NULL, created_at, updated_at)`
- `login_challenges(id, user_id FK, code_hash, attempts, expires_at, consumed_at NULL, created_at)`
- `email_logs(id, to_email, subject, login_challenge_id, created_at)` — **no code stored**

## 2FA design
- Code: 6 random digits. Stored as `HMAC-SHA256(OTP_SECRET, challenge_id || code)` — a plain SHA of a 6-digit code would be brute-forceable offline; the keyed hash is not without the server secret.
- Verify order: challenge exists → not consumed → not expired (5 min) → attempts < 5 → constant-time HMAC check (wrong → `attempts += 1`).
- Single use is enforced atomically: `UPDATE ... SET consumed_at=? WHERE id=? AND consumed_at IS NULL`, requiring `rows_affected == 1`.
- The plaintext code exists only in the in-memory dev outbox and console (dev only).

## JWT
HS256, claims `{sub, email, role, iat, exp}`, TTL 1 h. `AuthUser` extractor rejects missing/invalid tokens with 401; `require_admin` returns 403.

## Caching
- Key `my_tasks:{user_id}`, value = full payload minus cache metadata; TTL 60 s.
- Invalidation after DB commit on assign (new + previous assignees) and on update (current + previous assignee).
- **Limitation:** in-process only — not shared across instances, lost on restart. A Redis impl would slot in behind the same `get/set/invalidate` API. A narrow race (fill after invalidate) is bounded by the TTL.

## Endpoints
`POST /seed/users`, `POST /auth/login`, `POST /auth/verify-2fa`, `GET /dev/email-logs/latest?to=`, `POST /tasks`, `GET /tasks`, `POST /tasks/assign`, `PATCH /tasks/{id}`, `GET /tasks/view-my-tasks`, `GET /health`. Dev routes (`/seed`, `/dev`) are mounted only when `APP_ENV=development`.

## Testing
`tests/api.rs` drives the router in-process (`tower::oneshot`) against in-memory SQLite: full flow, wrong/expired/reused codes, 403s, cache hit/miss and invalidation on assign and update.
