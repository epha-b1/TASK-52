use leptos::*;
use crate::api::client;
use fieldtrace_shared::UserResponse;

#[component]
pub fn ReportsPage(user: ReadSignal<Option<UserResponse>>) -> impl IntoView {
    let (summary, set_summary) = create_signal(Option::<serde_json::Value>::None);
    let (err, set_err) = create_signal(Option::<String>::None);
    let (loading, set_loading) = create_signal(false);

    // Filter signals
    let (from, set_from) = create_signal(String::new());
    let (to, set_to) = create_signal(String::new());
    let (status, set_status) = create_signal(String::new());
    let (intake_type, set_intake_type) = create_signal(String::new());
    let (region, set_region) = create_signal(String::new());
    let (tags, set_tags) = create_signal(String::new());
    let (q, set_q) = create_signal(String::new());

    let can_export = move || {
        user.get().map(|u| u.role == "administrator" || u.role == "auditor").unwrap_or(false)
    };

    let run_query = move || {
        set_loading.set(true);
        set_err.set(None);
        spawn_local(async move {
            match client::reports_summary_filtered(
                &from.get(), &to.get(), &status.get(), &intake_type.get(),
                &region.get(), &tags.get(), &q.get(),
            ).await {
                Ok(v) => set_summary.set(Some(v)),
                Err(e) => set_err.set(Some(e.message)),
            }
            set_loading.set(false);
        });
    };

    // Initial load
    run_query.clone()();

    let export_url = move || {
        client::build_export_url(
            &from.get(), &to.get(), &status.get(), &intake_type.get(),
            &region.get(), &tags.get(), &q.get(),
        )
    };

    view! {
        <div class="card">
            <h2>"Dashboard & Reports"</h2>
            {move || err.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}

            <div class="filter-row">
                <input type="date" placeholder="From" prop:value=from
                    on:input=move |e| set_from.set(event_target_value(&e)) />
                <input type="date" placeholder="To" prop:value=to
                    on:input=move |e| set_to.set(event_target_value(&e)) />
                <select on:change=move |e| set_status.set(event_target_value(&e))>
                    <option value="">"All Statuses"</option>
                    <option value="received">"Received"</option>
                    <option value="in_care">"In Care"</option>
                    <option value="in_stock">"In Stock"</option>
                    <option value="adopted">"Adopted"</option>
                    <option value="transferred">"Transferred"</option>
                    <option value="disposed">"Disposed"</option>
                </select>
                <select on:change=move |e| set_intake_type.set(event_target_value(&e))>
                    <option value="">"All Types"</option>
                    <option value="animal">"Animal"</option>
                    <option value="supply">"Supply"</option>
                    <option value="donation">"Donation"</option>
                </select>
                <input placeholder="Region" prop:value=region
                    on:input=move |e| set_region.set(event_target_value(&e)) />
                <input placeholder="Tags" prop:value=tags
                    on:input=move |e| set_tags.set(event_target_value(&e)) />
                <input placeholder="Search (q)" prop:value=q
                    on:input=move |e| set_q.set(event_target_value(&e)) />
                <button class="btn" on:click=move |_| run_query() disabled=move || loading.get()>
                    {move || if loading.get() { "Loading..." } else { "Apply Filters" }}
                </button>
            </div>

            {move || summary.get().map(|v| view! {
                <div class="metrics">
                    <div class="metric"><strong>"Rescue Volume: "</strong>{v["rescue_volume"].to_string()}</div>
                    <div class="metric"><strong>"Donations Logged: "</strong>{v["donations_logged"].to_string()}</div>
                    <div class="metric"><strong>"Inventory on Hand: "</strong>{v["inventory_on_hand"].to_string()}</div>
                    <div class="metric"><strong>"Adoption Rate: "</strong>{v["adoption_conversion"].to_string()}</div>
                    <div class="metric"><strong>"Task Completion Rate: "</strong>{v["task_completion_rate"].to_string()}</div>
                </div>
            })}

            {move || can_export().then(|| {
                let url = export_url();
                view! {
                    <div class="export-controls">
                        <a href={url} class="btn" download="report.csv">"Export CSV"</a>
                    </div>
                }
            })}
        </div>
    }
}
