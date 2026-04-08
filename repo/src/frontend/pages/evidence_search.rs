//! Evidence search with keyword / tag / from / to filters, plus inline
//! link, legal-hold, and delete actions. Role-aware: auditors cannot
//! mutate; only uploader/admin can delete/link.

use leptos::*;
use crate::api::client;
use fieldtrace_shared::{EvidenceResponse, UserResponse};

#[component]
pub fn EvidenceSearchPage(user: ReadSignal<Option<UserResponse>>) -> impl IntoView {
    let (keyword, set_keyword) = create_signal(String::new());
    let (tag, set_tag) = create_signal(String::new());
    let (from, set_from) = create_signal(String::new());
    let (to, set_to) = create_signal(String::new());
    let (results, set_results) = create_signal(Vec::<EvidenceResponse>::new());
    let (err, set_err) = create_signal(Option::<String>::None);
    let (msg, set_msg) = create_signal(Option::<String>::None);

    let is_auditor = move || user.get().map(|u| u.role == "auditor").unwrap_or(true);
    let is_admin = move || user.get().map(|u| u.role == "administrator").unwrap_or(false);

    let run_search = move || {
        let k = keyword.get(); let tg = tag.get();
        let fr = from.get(); let t2 = to.get();
        spawn_local(async move {
            match client::list_evidence(&k, &tg, &fr, &t2).await {
                Ok(list) => { set_results.set(list); set_err.set(None); }
                Err(e) => set_err.set(Some(e.message)),
            }
        });
    };
    run_search.clone()();

    view! {
        <div class="card">
            <h2>"Evidence Search"</h2>
            {move || err.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}
            {move || msg.get().map(|m| view! { <div class="msg msg-info">{m}</div> })}
            <div class="filter-row">
                <input placeholder="keyword" prop:value=keyword
                    on:input=move |e| set_keyword.set(event_target_value(&e)) />
                <input placeholder="tag" prop:value=tag
                    on:input=move |e| set_tag.set(event_target_value(&e)) />
                <input type="date" prop:value=from
                    on:input=move |e| set_from.set(event_target_value(&e)) />
                <input type="date" prop:value=to
                    on:input=move |e| set_to.set(event_target_value(&e)) />
                <button class="btn" on:click=move |_| run_search()>"Search"</button>
            </div>
            <div class="list">
                {move || {
                    let items = results.get();
                    let auditor = is_auditor();
                    let admin = is_admin();
                    if items.is_empty() {
                        view! { <p class="muted">"No evidence matches those filters."</p> }.into_view()
                    } else {
                        items.into_iter().map(|e| {
                            let eid = e.id.clone();
                            let eid2 = e.id.clone();
                            let eid3 = e.id.clone();
                            let refresh = run_search.clone();
                            let refresh2 = run_search.clone();
                            let refresh3 = run_search.clone();
                            view! {
                                <div class="list-item">
                                    <div>
                                        <strong>{e.filename.clone()}</strong>
                                        <span class="tag tag-info">{e.media_type.clone()}</span>
                                        {if e.missing_exif {
                                            view! { <span class="tag tag-error">"missing EXIF"</span> }.into_view()
                                        } else { view! {}.into_view() }}
                                        {if e.linked {
                                            view! { <span class="tag tag-ok">"linked"</span> }.into_view()
                                        } else { view! {}.into_view() }}
                                        {if e.legal_hold {
                                            view! { <span class="tag tag-error">"legal hold"</span> }.into_view()
                                        } else { view! {}.into_view() }}
                                        <span class="muted">" wm: "{e.watermark_text.clone()}</span>
                                    </div>
                                    // Action buttons (hidden for auditors)
                                    {if !auditor {
                                        view! {
                                            <div class="evidence-actions">
                                                <EvidenceLinkForm evidence_id=eid.clone()
                                                    set_msg=set_msg set_err=set_err on_done=move || refresh() />
                                                {if admin {
                                                    let eid_hold = eid2.clone();
                                                    let hold = e.legal_hold;
                                                    view! {
                                                        <button class="btn btn-sm" on:click=move |_| {
                                                            let eid = eid_hold.clone();
                                                            let refresh = refresh2.clone();
                                                            spawn_local(async move {
                                                                match client::set_legal_hold(&eid, !hold).await {
                                                                    Ok(_) => { set_msg.set(Some("Legal hold updated".into())); refresh(); }
                                                                    Err(e) => set_err.set(Some(e.message)),
                                                                }
                                                            });
                                                        }>
                                                            {if hold { "Release Hold" } else { "Legal Hold" }}
                                                        </button>
                                                    }.into_view()
                                                } else { view! {}.into_view() }}
                                                {if !e.linked && !e.legal_hold {
                                                    view! {
                                                        <button class="btn btn-sm btn-danger" on:click=move |_| {
                                                            let eid = eid3.clone();
                                                            let refresh = refresh3.clone();
                                                            spawn_local(async move {
                                                                match client::delete_evidence(&eid).await {
                                                                    Ok(_) => { set_msg.set(Some("Evidence deleted".into())); refresh(); }
                                                                    Err(e) => set_err.set(Some(e.message)),
                                                                }
                                                            });
                                                        }>"Delete"</button>
                                                    }.into_view()
                                                } else { view! {}.into_view() }}
                                            </div>
                                        }.into_view()
                                    } else {
                                        view! { <span class="muted">"(read-only)"</span> }.into_view()
                                    }}
                                </div>
                            }
                        }).collect_view()
                    }
                }}
            </div>
        </div>
    }
}

/// Inline form to link evidence to a target resource.
#[component]
fn EvidenceLinkForm<F: Fn() + Clone + 'static>(
    evidence_id: String,
    set_msg: WriteSignal<Option<String>>,
    set_err: WriteSignal<Option<String>>,
    on_done: F,
) -> impl IntoView {
    let (show, set_show) = create_signal(false);
    let (target_type, set_target_type) = create_signal("intake".to_string());
    let (target_id, set_target_id) = create_signal(String::new());
    let (linking, set_linking) = create_signal(false);

    let eid = evidence_id.clone();
    let submit = move |_| {
        let tid = target_id.get();
        if tid.trim().is_empty() {
            set_err.set(Some("Target ID is required".into()));
            return;
        }
        set_linking.set(true);
        let eid = eid.clone();
        let tt = target_type.get();
        let on_done = on_done.clone();
        spawn_local(async move {
            match client::link_evidence(&eid, &tt, &tid).await {
                Ok(_) => {
                    set_msg.set(Some(format!("Linked to {} {}", tt, tid)));
                    set_show.set(false);
                    on_done();
                }
                Err(e) => set_err.set(Some(e.message)),
            }
            set_linking.set(false);
        });
    };

    view! {
        <button class="btn btn-sm" on:click=move |_| set_show.update(|v| *v = !*v)>
            {move || if show.get() { "Cancel Link" } else { "Link" }}
        </button>
        {move || show.get().then(|| view! {
            <div class="link-form">
                <select on:change=move |e| set_target_type.set(event_target_value(&e))>
                    <option value="intake">"Intake"</option>
                    <option value="inspection">"Inspection"</option>
                    <option value="traceability">"Traceability"</option>
                    <option value="checkin">"Check-in"</option>
                </select>
                <input placeholder="Target ID" prop:value=target_id
                    on:input=move |e| set_target_id.set(event_target_value(&e)) />
                <button class="btn btn-sm" on:click=submit.clone()
                    disabled=move || linking.get()>
                    {move || if linking.get() { "Linking..." } else { "Confirm" }}
                </button>
            </div>
        })}
    }
}
