# Task Management API (Rust)

Axum + SQLx (SQLite) API with password login, email-based 2FA, JWT auth, admin/staff roles, and a per-user cache for `GET /tasks/view-my-tasks`.
Design docs: [docs/PRD.md](docs/PRD.md), [docs/TRD.md](docs/TRD.md).

## Setup

Requires Rust stable (edition 2021).

```bash
cp .env.example .env      # optional: every setting has a dev default
cargo build
```

## Migrations

SQL migrations live in `migrations/` and are **applied automatically on startup** (`sqlx::migrate!`).
To run them manually with sqlx-cli instead:

```bash
cargo install sqlx-cli --no-default-features --features sqlite
sqlx database create && sqlx migrate run   # uses DATABASE_URL
```

## Run

```bash
cargo run          # listens on http://127.0.0.1:8080
```

2FA codes are also printed in the server log (`[DEV EMAIL] 2FA code sent ... code=123456`).

## Seed

```bash
curl -X POST http://127.0.0.1:8080/seed/users
```

| User | Email | Password | Role |
|---|---|---|---|
| Admin | admin@example.com | `Admin@12345` | admin |
| James Bond | jamesbond@example.com | `Bond@007007` | staff |

Seeding is idempotent. `/seed/*` and `/dev/*` exist only when `APP_ENV=development` (the default).

## Endpoints

| Method | Path | Auth | Purpose |
|---|---|---|---|
| POST | `/seed/users` | – (dev) | Create Admin and James Bond |
| POST | `/auth/login` | – | `{email,password}` → `{login_challenge_id}` and sends the code (no JWT) |
| GET | `/dev/email-logs/latest?to=<email>` | – (dev) | Latest dev email, including `code` |
| POST | `/auth/verify-2fa` | – | `{login_challenge_id,code}` → `{access_token}` |
| POST | `/tasks` | admin | `{title, description?, priority?, status?}` |
| GET | `/tasks` | admin | List all tasks |
| POST | `/tasks/assign` | admin | `{task_ids:[...], assignee_email}` (or `assignee_id`) |
| PATCH | `/tasks/{id}` | admin | Update title/description/status/priority |
| GET | `/tasks/view-my-tasks` | any | Caller's tasks + `cache.hit` |

Statuses: `todo`, `in_progress`, `done`. Priorities: `low`, `medium`, `high`.

## Validation

**Scripted (PowerShell):** start the server, then:

```bash
powershell -ExecutionPolicy Bypass -File scripts/validate.ps1
```

It seeds, logs in Admin via 2FA, creates 5 tasks, assigns 3 to James Bond, logs in James via 2FA, shows that James gets 403 when creating a task, then calls view-my-tasks twice (`cache.hit` False, then True).

**Manual (curl):**

```bash
B=http://127.0.0.1:8080
curl -X POST $B/seed/users
curl -X POST $B/auth/login -H 'content-type: application/json' -d '{"email":"admin@example.com","password":"Admin@12345"}'
curl "$B/dev/email-logs/latest?to=admin@example.com"          # read "code"
curl -X POST $B/auth/verify-2fa -H 'content-type: application/json' -d '{"login_challenge_id":"<id>","code":"<code>"}'
ADMIN=<access_token>
curl -X POST $B/tasks -H "authorization: Bearer $ADMIN" -H 'content-type: application/json' -d '{"title":"Task 1","priority":"high"}'   # x5
curl -X POST $B/tasks/assign -H "authorization: Bearer $ADMIN" -H 'content-type: application/json' -d '{"task_ids":["<id1>","<id2>","<id3>"],"assignee_email":"jamesbond@example.com"}'
# repeat login + verify for jamesbond@example.com / Bond@007007 -> JAMES
curl -i -X POST $B/tasks -H "authorization: Bearer $JAMES" -H 'content-type: application/json' -d '{"title":"x"}'   # 403
curl $B/tasks/view-my-tasks -H "authorization: Bearer $JAMES"   # cache.hit false
curl $B/tasks/view-my-tasks -H "authorization: Bearer $JAMES"   # cache.hit true
```

### Final response (captured from a real run)

`GET /tasks/view-my-tasks` as James Bond, first call:

```json
{
  "user": { "email": "jamesbond@example.com", "role": "staff" },
  "tasks": [
    {
      "id": "24e47e58-5f11-4e88-83f7-0ba0b216ce96",
      "title": "Mission (high)",
      "description": "Created by Admin",
      "status": "todo",
      "priority": "high",
      "assigned_to": "jamesbond@example.com",
      "created_at": "2026-09-29T04:01:07.350651Z",
      "updated_at": "2026-09-29T04:01:07.405962400Z"
    },
    {
      "id": "679ce00d-00b9-46c4-8198-cd018a5fd411",
      "title": "Mission (medium)",
      "description": "Created by Admin",
      "status": "todo",
      "priority": "medium",
      "assigned_to": "jamesbond@example.com",
      "created_at": "2026-09-29T04:01:07.362220300Z",
      "updated_at": "2026-09-29T04:01:07.407092700Z"
    },
    {
      "id": "af2d4a99-9609-4421-9ca4-d3baf9cac9da",
      "title": "Mission (low)",
      "description": "Created by Admin",
      "status": "todo",
      "priority": "low",
      "assigned_to": "jamesbond@example.com",
      "created_at": "2026-09-29T04:01:07.369404300Z",
      "updated_at": "2026-09-29T04:01:07.407350400Z"
    }
  ],
  "summary": { "total_assigned_tasks": 3 },
  "cache": { "hit": false }
}
```

The second call returns the same body with `"cache": { "hit": true }`.

## Tests

```bash
cargo test                                   # unit + integration
cargo test --test api full_validation_workflow   # one test
```

Integration tests (`tests/api.rs`) run the router in-process against an in-memory SQLite database. They cover the full flow; wrong, reused and expired codes; 403 for staff; 401 without a token; and cache invalidation on assign and update.

## Design notes and limitations

- **2FA:** 6-digit code, 5-minute expiry, single use (atomic `UPDATE ... WHERE consumed_at IS NULL`), locked after 5 wrong attempts. Stored as HMAC-SHA256(`OTP_SECRET`, challenge_id:code), never in plaintext. The `email_logs` table stores metadata only; the readable message exists only in the in-memory dev outbox and the console.
- **Cache:** in-memory per-process TTL cache (default 60 s), keyed `my_tasks:{user_id}`. It is invalidated for every affected user (new and previous assignee) after assign/update. **Limitation:** it isn't shared across instances and is lost on restart; Redis would replace `TtlCache` behind the same API in production.
- **Database:** SQLite, to keep local setup free of dependencies. The SQL is portable to Postgres.
- **Not included:** refresh tokens and logout, real SMTP, staff editing their own tasks, OpenAPI docs.
