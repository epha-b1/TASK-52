//! Host-target unit tests for the real frontend source files.
//!
//! This crate does **not** reimplement or mock any frontend module —
//! every test reaches into the actual files under `src/frontend/` via
//! `#[path = "..."]` `mod` declarations and exercises their exported
//! items. That means a failing assertion here points directly at the
//! frontend code that ships in the WASM bundle.
//!
//! Test layout mirrors the categories the project-wide audit requires
//! for a fullstack app:
//!   * `session_behavior`   – app shell / session state machine + draft store
//!   * `auth_form`          – login/register credential validation
//!   * `dashboard_view`     – dashboard/reporting filter serialisation
//!   * `intake_form`        – form-heavy module (intake create)
//!   * `ui_format`          – shared UI formatting helpers (Nav, tables)

#![allow(dead_code)]

// ── Real frontend modules, imported by path ──────────────────────────
// Each `#[path]` points at the exact same `.rs` file the frontend bin
// compiles into the WASM bundle, so the tests can't drift from prod.

#[path = "../frontend/draft.rs"]
pub mod frontend_draft;

#[path = "../frontend/logic/auth_form.rs"]
pub mod frontend_auth_form;

#[path = "../frontend/logic/dashboard_filter.rs"]
pub mod frontend_dashboard_filter;

#[path = "../frontend/logic/intake_form.rs"]
pub mod frontend_intake_form;

#[path = "../frontend/logic/session.rs"]
pub mod frontend_session;

#[path = "../frontend/logic/ui_format.rs"]
pub mod frontend_ui_format;
