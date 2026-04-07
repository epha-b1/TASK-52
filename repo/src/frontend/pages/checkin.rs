use leptos::*;
use crate::api::client;
use fieldtrace_shared::{CheckinRequest, MemberRequest, MemberResponse, UserResponse};

#[component]
pub fn CheckinPage(user: ReadSignal<Option<UserResponse>>) -> impl IntoView {
    let (members, set_members) = create_signal(Vec::<MemberResponse>::new());
    let (history, set_history) = create_signal(Vec::<serde_json::Value>::new());
    let (msg, set_msg) = create_signal(Option::<String>::None);
    let (err, set_err) = create_signal(Option::<String>::None);
    let (show_add, set_show_add) = create_signal(false);
    let (checkin_id, set_checkin_id) = create_signal(String::new());
    let (loading, set_loading) = create_signal(false);

    let refresh = move || {
        spawn_local(async move {
            if let Ok(list) = client::list_members().await { set_members.set(list); }
            if let Ok(h) = client::checkin_history().await {
                if let Some(arr) = h.get("history").and_then(|v| v.as_array()) {
                    set_history.set(arr.clone());
                }
            }
        });
    };
    refresh();

    let is_admin = move || {
        user.get().map(|u| u.role == "administrator").unwrap_or(false)
    };

    let do_checkin = move |_| {
        let mid = checkin_id.get();
        if mid.is_empty() { set_err.set(Some("Select a member".into())); return; }
        set_loading.set(true);
        set_err.set(None);
        set_msg.set(None);
        let refresh = refresh.clone();
        spawn_local(async move {
            let req = CheckinRequest { member_id: mid, override_reason: None };
            match client::checkin(&req).await {
                Ok(r) => {
                    set_msg.set(Some(format!("Checked in: {}", r.member_id)));
                    refresh();
                }
                Err(e) => set_err.set(Some(e.message)),
            }
            set_loading.set(false);
        });
    };

    view! {
        <div class="card">
            <h2>"Member Check-In"</h2>
            {move || msg.get().map(|m| view! { <div class="msg msg-info">{m}</div> })}
            {move || err.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}

            <div class="checkin-controls">
                <select on:change=move |e| set_checkin_id.set(event_target_value(&e))>
                    <option value="">"-- Select Member --"</option>
                    {move || members.get().into_iter().map(|m| {
                        let mid = m.member_id.clone();
                        let label = format!("{} ({})", m.name, m.member_id);
                        view! { <option value={mid}>{label}</option> }
                    }).collect_view()}
                </select>
                <button class="btn" on:click=do_checkin disabled=move || loading.get()>
                    {move || if loading.get() { "Processing..." } else { "Check In" }}
                </button>
            </div>

            <button class="btn btn-sm" on:click=move |_| set_show_add.update(|v| *v = !*v)>
                {move || if show_add.get() { "Cancel" } else { "Add Member" }}
            </button>
            {move || show_add.get().then(|| {
                let refresh = refresh.clone();
                let set_show = set_show_add;
                view! { <AddMemberForm on_done=move || { refresh(); set_show.set(false); } /> }
            })}

            <section class="ws-section">
                <h3>"Recent Check-Ins"</h3>
                {move || {
                    let items = history.get();
                    if items.is_empty() {
                        view! { <p class="muted">"No check-in history."</p> }.into_view()
                    } else {
                        items.into_iter().take(20).map(|h| {
                            let mid = h.get("member_id").and_then(|v| v.as_str()).unwrap_or("-").to_string();
                            let at = h.get("checked_in_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            view! {
                                <div class="ws-row">
                                    <strong>{mid}</strong>
                                    <span class="muted">" "{at}</span>
                                </div>
                            }
                        }).collect_view()
                    }
                }}
            </section>

            {move || is_admin().then(|| view! {
                <p class="muted">"As admin, you can override anti-passback via the API with override_reason."</p>
            })}
        </div>
    }
}

#[component]
fn AddMemberForm<F: Fn() + Clone + 'static>(on_done: F) -> impl IntoView {
    let (mid, set_mid) = create_signal(String::new());
    let (name, set_name) = create_signal(String::new());
    let (err, set_err) = create_signal(Option::<String>::None);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_err.set(None);
        let req = MemberRequest { member_id: mid.get(), name: name.get() };
        let on_done = on_done.clone();
        spawn_local(async move {
            match client::create_member(&req).await {
                Ok(_) => on_done(),
                Err(e) => set_err.set(Some(e.message)),
            }
        });
    };

    view! {
        <form class="addr-form" on:submit=submit>
            {move || err.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}
            <input placeholder="Member ID" prop:value=mid
                on:input=move |e| set_mid.set(event_target_value(&e)) required=true />
            <input placeholder="Full Name" prop:value=name
                on:input=move |e| set_name.set(event_target_value(&e)) required=true />
            <button type="submit" class="btn">"Create Member"</button>
        </form>
    }
}
