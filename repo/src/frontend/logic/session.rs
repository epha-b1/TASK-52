//! App-shell session state — the small state machine that drives the
//! `App` component's page transitions. Pure so the test crate can
//! exercise the full auth lifecycle without a browser.

/// What the app shell is currently rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Loading,
    Login,
    Register,
    Dashboard,
}

/// Events the shell reacts to: session check result, credential flow
/// outcomes, logout. The Leptos component in `app.rs` produces these
/// as async side-effects land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    AuthCheckOk,
    AuthCheckFailed,
    LoginSucceeded,
    LoginFailed,
    LogoutRequested,
    NavigateToRegister,
    NavigateToLogin,
}

/// Pure reducer for `(Page, SessionEvent) -> Page`. Any event that is
/// not valid in the current page is a no-op — the shell never crashes
/// on a stale event from a cancelled request.
pub fn next_page(current: &Page, ev: &SessionEvent) -> Page {
    use Page::*;
    use SessionEvent::*;
    match (current, ev) {
        (Loading, AuthCheckOk) => Dashboard,
        (Loading, AuthCheckFailed) => Login,
        (Login, LoginSucceeded) => Dashboard,
        (Login, NavigateToRegister) => Register,
        (Register, NavigateToLogin) => Login,
        (Register, LoginSucceeded) => Dashboard,
        (Dashboard, LogoutRequested) => Login,
        (_, LoginFailed) => Login,
        _ => current.clone(),
    }
}
