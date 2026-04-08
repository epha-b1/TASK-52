use leptos::*;

use crate::api::client;
use crate::app::Page;
use crate::pages::address_book::AddressBookPage;
use crate::pages::admin::AdminPage;
use crate::pages::checkin::CheckinPage;
use crate::pages::evidence_search::EvidenceSearchPage;
use crate::pages::evidence_upload::EvidenceUploadPage;
use crate::pages::intake::IntakePage;
use crate::pages::profile::ProfilePage;
use crate::pages::reports::ReportsPage;
use crate::pages::supply::SupplyPage;
use crate::pages::traceability::TraceabilityPage;
use crate::pages::workspace::WorkspacePage;
use fieldtrace_shared::UserResponse;

#[component]
pub fn DashboardPage(
    user: ReadSignal<Option<UserResponse>>,
    set_page: WriteSignal<Page>,
    set_user: WriteSignal<Option<UserResponse>>,
) -> impl IntoView {
    let (health, set_health) = create_signal(Option::<Result<String, String>>::None);

    spawn_local(async move {
        let result = client::check_health().await;
        set_health.set(Some(result));
    });

    // Session-expiry heartbeat: periodically verify session via /auth/me.
    // On 401, immediately redirect to login (centralized expiry handling).
    {
        let set_page = set_page;
        let set_user = set_user;
        spawn_local(async move {
            loop {
                gloo_timers::future::sleep(std::time::Duration::from_secs(60)).await;
                if client::get_me().await.is_err() {
                    set_user.set(None);
                    set_page.set(Page::Login);
                    break;
                }
            }
        });
    }

    let is_admin = move || {
        user.get().map(|u| u.role == "administrator").unwrap_or(false)
    };

    view! {
        <div class="app-body">
            <div class="card">
                <h2>"System Status"</h2>
                {move || match health.get() {
                    None => view! {
                        <span class="status-indicator status-loading">
                            <span class="dot dot-loading"></span>
                            "Checking..."
                        </span>
                    }.into_view(),
                    Some(Ok(s)) => view! {
                        <span class="status-indicator status-ok">
                            <span class="dot dot-ok"></span>
                            {format!("System: {}", s)}
                        </span>
                    }.into_view(),
                    Some(Err(e)) => view! {
                        <span class="status-indicator status-error">
                            <span class="dot dot-error"></span>
                            {format!("Error: {}", e)}
                        </span>
                    }.into_view(),
                }}
            </div>

            <WorkspacePage />
            <ReportsPage user=user />
            <IntakePage />
            <SupplyPage />
            <TraceabilityPage user=user />
            <CheckinPage user=user />
            <EvidenceUploadPage user=user />
            <EvidenceSearchPage user=user />
            <AddressBookPage />
            <ProfilePage user=user />

            // Admin operations — only visible to administrators
            {move || is_admin().then(|| view! { <AdminPage /> })}
        </div>
    }
}
