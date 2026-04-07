# FieldTrace Design Document

## Overview

FieldTrace is an offline-first facility management system built with Axum (Rust backend) + Leptos (WASM frontend) + SQLite. It covers intake, evidence/media, supply parsing, traceability, check-in policy, analytics, auditability, and security controls.

## Design Scope Index

- Architecture and module responsibilities: see `repo/README.md` (Slices Implemented section)
- API endpoints and behavior: `docs/api-spec.md`
- Requirement clarifications and assumptions: `docs/questions.md`
- Database schema: `repo/migrations/` (12 migration files)
- Test inventory: `repo/run_tests.sh`, `repo/API_tests/`, `repo/unit_tests/`

## Key Architectural Decisions

- **Single-binary deployment**: Backend + static frontend served from one Axum process
- **SQLite + WAL mode**: Supports offline operation with 5-connection pool
- **AES-256-GCM encryption at rest**: Address book fields, donor references
- **Argon2id password hashing**: 12-char minimum, account lockout (10 failures/15 min)
- **Session-based auth**: 30-minute inactivity timeout, HttpOnly cookies
- **RBAC**: administrator, operations_staff, auditor (matrix enforced per-route)
- **Background jobs**: Session cleanup, account purge, evidence retention, diagnostics cleanup
- **Config versioning**: Cap 10 versions, rollback support
- **Idempotency**: Actor-scoped, 10-minute window for mutating operations
