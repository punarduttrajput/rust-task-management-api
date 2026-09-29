# PRD — Task Management API (Auth, 2FA, RBAC, Caching)

## 1. Problem

A small team needs a backend where an administrator creates tasks and hands them to staff, and staff see only
their own work. Access must be protected by password + email-based two-factor authentication, and the most
frequently hit read ("what are my tasks?") should be served from a per-user cache.

This is a local-development assignment: no frontend, no real email delivery, no deployment.

## 2. Users & roles

| Role    | Example                           | Can do                                                        |
|---------|-----------------------------------|---------------------------------------------------------------|
| `admin` | Admin (`admin@example.com`)       | Log in, create tasks, list all tasks, assign tasks, update tasks |
| `staff` | James Bond (`jamesbond@example.com`) | Log in, view tasks assigned to them                        |

## 3. Core user journey (acceptance flow)

1. Operator seeds the two users (`POST /seed/users`).
2. Admin submits email + password → receives a `login_challenge_id` (**no JWT yet**); a 6-digit code is "emailed".
3. Operator reads the code from the dev mailbox (`GET /dev/email-logs/latest`) or the server console.
4. Admin submits challenge id + code → receives a JWT.
5. Admin creates exactly 5 tasks and assigns exactly 3 of them to James Bond.
6. James Bond completes the same 2FA login and receives his JWT.
7. James Bond tries to create a task → **403 Forbidden**.
8. James Bond calls `GET /tasks/view-my-tasks` → exactly 3 tasks, `cache.hit = false`.
9. Same call again → same data, `cache.hit = true`.

## 4. Functional requirements

### FR-1 Users
- A user has id, full name, email (unique), hashed password, role (`admin`|`staff`), created/updated timestamps.
- A development seed endpoint creates Admin and James Bond idempotently.

### FR-2 Login with email 2FA
- `POST /auth/login` validates credentials. On success it creates a login challenge and sends a one-time
  6-digit code. It never returns a JWT.
- Invalid credentials return 401 with a generic message (no user enumeration).
- Codes expire after **5 minutes**, are **single use**, and wrong/expired/reused codes are rejected.
- Repeated wrong guesses lock the challenge after a small number of attempts (5).
- Codes are **never persisted in plain text**.
- `POST /auth/verify-2fa` issues a JWT only after successful verification.

### FR-3 Development email log
- Every 2FA email is printed to the console and recorded.
- `GET /dev/email-logs/latest` (optionally filtered by recipient) returns the most recent message including the
  code. Available only when the app runs in development mode.

### FR-4 Tasks
- A task has id, title, description, status (`todo`|`in_progress`|`done`), priority (`low`|`medium`|`high`),
  created_by, assigned_to (nullable), created/updated timestamps.
- `POST /tasks` — admin only; staff get 403.
- `POST /tasks/assign` — admin only; assigns a list of task ids to a user (by email or id).
- `PATCH /tasks/{id}` — admin only; updates title/description/status/priority.
- `GET /tasks` — admin only; lists all tasks (convenience for picking ids).

### FR-5 View my tasks
- `GET /tasks/view-my-tasks` returns the caller's identity, their assigned tasks (from the database), a summary
  count and cache metadata, in the shape given by the assignment.
- Tasks are ordered by priority (high → low), then creation time.

### FR-6 Caching
- The view-my-tasks payload is cached per user.
- First call: `cache.hit = false`; subsequent identical calls: `cache.hit = true`.
- Assigning or updating a task invalidates the cache of every affected user (new and previous assignee).
- Entries also expire after a TTL (default 60 s) as a safety net.

## 5. Non-functional requirements

- Rust (edition 2021), async (Tokio), idiomatic and modular structure.
- Passwords hashed with Argon2id.
- Consistent JSON error envelope and correct HTTP status codes (400/401/403/404/409/500).
- Migrations are versioned SQL files, applied automatically on startup.
- `cargo test` covers the full workflow and the 2FA/RBAC/cache edge cases.
- One-command local run; validation scriptable with curl/PowerShell.

## 6. Out of scope

- Real SMTP delivery, refresh tokens, logout/token revocation, password reset, user self-registration.
- Staff updating their own task status.
- Pagination, search, multi-tenant support, frontend.

## 7. Success criteria

- The acceptance flow in §3 passes end-to-end via the provided script and the integration test.
- The final James Bond response matches the assignment's expected shape, with 3 tasks and `cache.hit` false then true.

## 8. Deliverables

README (setup, migrate, run, seed, validate, test, final response), AI_USAGE.md, `.env.example`, source,
migrations, tests, this PRD and the TRD.
