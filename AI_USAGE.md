# AI Usage

**Tool:** Claude Code (Anthropic, Claude Opus 5.5).

**AI-generated:** the PRD/TRD drafts, the project scaffold, the handlers, migrations, integration tests, the validation script and this documentation. Everything was produced from the assignment text in one guided session.

**Human-directed decisions and review** (to be completed by the candidate before submission):
- Chose SQLite and an in-memory cache because Postgres, Redis and Docker were not available locally; the limitation is documented.
- Stored 2FA codes as a keyed HMAC (not plain SHA-256) so the stored hashes can't be brute-forced offline.
- Kept the email log table metadata-only so the code is never persisted in plaintext.
- Reviewed the output and ran `cargo test` and `scripts/validate.ps1` against the live server.

_Candidate: list any manual edits you make here, and be ready to explain each module (see `docs/TRD.md`)._
