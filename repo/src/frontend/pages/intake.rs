use leptos::*;
use crate::api::client;
use crate::draft;
use fieldtrace_shared::{IntakeRequest, IntakeResponse};

const INTAKE_FORM_ID: &str = "intake-form";

#[component]
pub fn IntakePage() -> impl IntoView {
    let (entries, set_entries) = create_signal(Vec::<IntakeResponse>::new());
    let (show_form, set_show_form) = create_signal(false);

    let refresh = move || {
        spawn_local(async move {
            if let Ok(list) = client::list_intake().await { set_entries.set(list); }
        });
    };
    refresh();

    // If a draft is present (from a previous session that timed out)
    // open the form automatically so the user sees their preserved input.
    if draft::load_draft(INTAKE_FORM_ID).is_some() {
        set_show_form.set(true);
    }

    view! {
        <div class="card">
            <h2>"Intake Records"</h2>
            <button class="btn" on:click=move |_| set_show_form.update(|v| *v = !*v)>
                {move || if show_form.get() { "Cancel" } else { "New Intake" }}
            </button>
            {move || show_form.get().then(|| {
                let refresh = refresh.clone();
                let set_show = set_show_form;
                view! { <IntakeForm on_done=move || { refresh(); set_show.set(false); } /> }
            })}
            <div class="list">
                {move || entries.get().into_iter().map(|r| {
                    let id = r.id.clone();
                    let status_class = match r.status.as_str() {
                        "received" => "tag-info",
                        "adopted" => "tag-ok",
                        _ => "tag-default",
                    };
                    view! {
                        <div class="list-item">
                            <strong>{r.intake_type.clone()}</strong>
                            <span class={format!("tag {}", status_class)}>{r.status.clone()}</span>
                            <span class="muted">" ID: "{id}</span>
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}

#[component]
fn IntakeForm<F: Fn() + Clone + 'static>(on_done: F) -> impl IntoView {
    // Seed from any preserved draft. This is what makes a forced re-login
    // round-trip with the user's typing still in the form.
    let restored = draft::load_draft(INTAKE_FORM_ID);
    let pick = |k: &str, default: &str| -> String {
        restored.as_ref()
            .and_then(|v| v.get(k).and_then(|s| s.as_str().map(String::from)))
            .unwrap_or_else(|| default.to_string())
    };

    let (itype, set_itype) = create_signal(pick("intake_type", "animal"));
    let (details, set_details) = create_signal(pick("details", ""));
    let (region, set_region) = create_signal(pick("region", ""));
    let (tags, set_tags) = create_signal(pick("tags", ""));
    let (err, set_err) = create_signal(Option::<String>::None);

    // Autosave on every field change. Cheap — just a JSON write to
    // localStorage.
    create_effect(move |_| {
        let snapshot = serde_json::json!({
            "intake_type": itype.get(),
            "details": details.get(),
            "region": region.get(),
            "tags": tags.get(),
        });
        draft::save_draft(INTAKE_FORM_ID, snapshot);
    });

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let req = IntakeRequest {
            intake_type: itype.get(),
            details: details.get(),
            region: region.get(),
            tags: tags.get(),
        };
        let on_done = on_done.clone();
        spawn_local(async move {
            match client::create_intake(&req).await {
                Ok(_) => {
                    draft::clear_draft(INTAKE_FORM_ID);
                    on_done();
                }
                Err(e) => set_err.set(Some(e.message)),
            }
        });
    };

    view! {
        <form class="addr-form" on:submit=submit>
            {move || err.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}
            <select on:change=move |e| set_itype.set(event_target_value(&e))>
                <option value="animal">"Animal"</option>
                <option value="supply">"Supply"</option>
                <option value="donation">"Donation"</option>
            </select>
            <input placeholder="Details (JSON)" prop:value=details
                on:input=move |e| set_details.set(event_target_value(&e)) />
            <input placeholder="Region (e.g. north, warehouse-2)" prop:value=region
                on:input=move |e| set_region.set(event_target_value(&e)) />
            <input placeholder="Tags (comma-separated)" prop:value=tags
                on:input=move |e| set_tags.set(event_target_value(&e)) />
            <button type="submit" class="btn">"Create Intake"</button>
        </form>
    }
}
