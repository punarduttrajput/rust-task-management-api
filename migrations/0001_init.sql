CREATE TABLE users (
    id              TEXT PRIMARY KEY,
    full_name       TEXT NOT NULL,
    email           TEXT NOT NULL UNIQUE,
    hashed_password TEXT NOT NULL,
    role            TEXT NOT NULL CHECK (role IN ('admin', 'staff')),
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE tasks (
    id             TEXT PRIMARY KEY,
    title          TEXT NOT NULL,
    description    TEXT NOT NULL DEFAULT '',
    status         TEXT NOT NULL CHECK (status IN ('todo', 'in_progress', 'done')),
    priority       TEXT NOT NULL CHECK (priority IN ('low', 'medium', 'high')),
    created_by_id  TEXT NOT NULL REFERENCES users(id),
    assigned_to_id TEXT REFERENCES users(id),
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);
CREATE INDEX idx_tasks_assigned_to ON tasks(assigned_to_id);

CREATE TABLE login_challenges (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id),
    code_hash   TEXT NOT NULL,
    attempts    INTEGER NOT NULL DEFAULT 0,
    expires_at  TEXT NOT NULL,
    consumed_at TEXT,
    created_at  TEXT NOT NULL
);

-- Metadata only: the verification code itself is never persisted.
CREATE TABLE email_logs (
    id                 TEXT PRIMARY KEY,
    to_email           TEXT NOT NULL,
    subject            TEXT NOT NULL,
    login_challenge_id TEXT REFERENCES login_challenges(id),
    created_at         TEXT NOT NULL
);
