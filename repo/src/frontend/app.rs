use leptos::*;

use crate::api::client;
use crate::components::nav::Nav;
use crate::draft;
use crate::pages::{dashboard::DashboardPage, login::LoginPage, register::RegisterPage};
use fieldtrace_shared::UserResponse;

#[derive(Clone, Debug, PartialEq)]
pub enum Page {
    Loading,
    Login,
    Register,
    Dashboard,
}

#[component]
pub fn App() -> impl IntoView {
    let (page, set_page) = create_signal(Page::Loading);
    let (user, set_user) = create_signal(Option::<UserResponse>::None);
    let (session_msg, set_session_msg) = create_signal(Option::<String>::None);
    // Route we came from before the last session expiry. Populated when
    // the user is authenticated (either on initial mount or after login)
    // and consumed exactly once.
    let (restored_route, set_restored_route) = create_signal(Option::<String>::None);

    // On mount: pick up any session-expired flash message from the last
    // forced redirect, then check current auth state.
    if let Some(flash) = draft::consume_session_flash() {
        set_session_msg.set(Some(flash));
    }

    // Called every time the user transitions from unauthenticated to
    // Dashboard — on both initial mount (cookie still valid) and on
    // successful login. Consumes the pending route, updates the URL bar,
    // and stashes the value in a signal so the dashboard can render a
    // "Session restored" banner.
    let consume_route_and_restore = move || {
        if let Some(route) = draft::consume_pending_route() {
            draft::restore_browser_url(&route);
            set_restored_route.set(Some(route));
        }
    };

    {
        let consume_route_and_restore = consume_route_and_restore.clone();
        spawn_local(async move {
            match client::get_me().await {
                Ok(u) => {
                    set_user.set(Some(u));
                    // Already authenticated on mount (session cookie survived).
                    // Consume any route that was preserved by a prior 401.
                    consume_route_and_restore();
                    set_page.set(Page::Dashboard);
                }
                Err(_) => {
                    set_page.set(Page::Login);
                }
            }
        });
    }

    let do_logout = move || {
        let set_page = set_page;
        let set_user = set_user;
        let set_session_msg = set_session_msg;
        spawn_local(async move {
            let _ = client::logout().await;
            set_user.set(None);
            set_session_msg.set(Some("You have been logged out.".into()));
            set_page.set(Page::Login);
        });
    };

    // Wrapper around the normal login success path — consumes the
    // pending route before handing off to the Dashboard view.
    let on_login_success = {
        let consume_route_and_restore = consume_route_and_restore.clone();
        move || {
            consume_route_and_restore();
        }
    };

    view! {
        {move || {
            let p = page.get();
            match p {
                Page::Loading => view! {
                    <div class="center-box">
                        <span class="status-indicator status-loading">
                            <span class="dot dot-loading"></span>
                            "Loading..."
                        </span>
                    </div>
                }.into_view(),

                Page::Login => {
                    let on_login_success = on_login_success.clone();
                    view! {
                        <LoginPage
                            set_page=set_page
                            set_user=set_user
                            session_msg=session_msg
                            set_session_msg=set_session_msg
                            on_login_success=on_login_success
                        />
                    }.into_view()
                },

                Page::Register => view! {
                    <RegisterPage set_page=set_page set_user=set_user />
                }.into_view(),

                Page::Dashboard => {
                    let restored_route = restored_route;
                    view! {
                        <Nav user=user on_logout=do_logout.clone() />
                        // Visible banner when the app just restored a route.
                        // The distinctive prefix literal is what the
                        // frontend_draft_test.sh shell test greps for.
                        {move || restored_route.get().map(|r| view! {
                            <div class="msg msg-info session-restored-banner">
                                {format!("{} {}", draft::RESTORE_BANNER_PREFIX, r)}
                            </div>
                        })}
                        <DashboardPage user=user set_page=set_page set_user=set_user />
                    }.into_view()
                },
            }
        }}
    }
}
