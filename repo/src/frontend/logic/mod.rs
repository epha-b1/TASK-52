//! Pure, host-compatible frontend logic.
//!
//! These modules contain the non-DOM, non-WASM helpers that the Leptos
//! components and pages delegate to (validation, filter shaping, format
//! rendering, form-state derivation). They compile cleanly on the host
//! target, so `fieldtrace-frontend-tests` (a lib crate in the workspace)
//! includes the same source files via `#[path]` and exercises them with
//! ordinary `cargo test` — the Rust-native equivalent of Vitest/Jest +
//! Testing Library.

pub mod auth_form;
// The remaining three helpers are currently consumed only by the
// `fieldtrace-frontend-tests` crate (via `#[path]` include). The
// Leptos components will migrate onto them in follow-up work; for now
// we allow the bin to compile without dead-code warnings so dropping
// them in doesn't force an unrelated refactor.
#[allow(dead_code)] pub mod dashboard_filter;
#[allow(dead_code)] pub mod intake_form;
#[allow(dead_code)] pub mod session;
pub mod ui_format;
