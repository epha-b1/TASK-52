use leptos::*;
use crate::api::client;
use fieldtrace_shared::{TraceCodeRequest, TraceCodeResponse, TraceStepResponse, UserResponse};

#[component]
pub fn TraceabilityPage(user: ReadSignal<Option<UserResponse>>) -> impl IntoView {
    let (codes, set_codes) = create_signal(Vec::<TraceCodeResponse>::new());
    let (err, set_err) = create_signal(Option::<String>::None);
    let (msg, set_msg) = create_signal(Option::<String>::None);
    let (selected, set_selected) = create_signal(Option::<String>::None);
    let (steps, set_steps) = create_signal(Vec::<TraceStepResponse>::new());
    let (show_create, set_show_create) = create_signal(false);

    let refresh = move || {
        spawn_local(async move {
            match client::list_traceability().await {
                Ok(list) => set_codes.set(list),
                Err(e) => set_err.set(Some(e.message)),
            }
        });
    };
    refresh();

    let can_publish = move || {
        user.get().map(|u| u.role == "administrator" || u.role == "auditor").unwrap_or(false)
    };
    let can_create = move || {
        user.get().map(|u| u.role != "auditor").unwrap_or(false)
    };

    let load_steps = move |code_id: String| {
        set_selected.set(Some(code_id.clone()));
        spawn_local(async move {
            match client::list_trace_steps(&code_id).await {
                Ok(s) => set_steps.set(s),
                Err(e) => set_err.set(Some(e.message)),
            }
        });
    };

    view! {
        <div class="card">
            <h2>"Traceability Codes"</h2>
            {move || msg.get().map(|m| view! { <div class="msg msg-info">{m}</div> })}
            {move || err.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}

            {move || can_create().then(|| view! {
                <button class="btn" on:click=move |_| set_show_create.update(|v| *v = !*v)>
                    {move || if show_create.get() { "Cancel" } else { "Generate Code" }}
                </button>
            })}

            {move || show_create.get().then(|| {
                let refresh = refresh.clone();
                let set_show = set_show_create;
                let set_msg = set_msg;
                view! { <CreateCodeForm on_done=move |code: String| {
                    set_msg.set(Some(format!("Generated: {}", code)));
                    refresh();
                    set_show.set(false);
                } /> }
            })}

            <div class="list">
                {move || codes.get().into_iter().map(|c| {
                    let id = c.id.clone();
                    let code = c.code.clone();
                    let status = c.status.clone();
                    let status_tag = match status.as_str() {
                        "published" => "tag-ok",
                        "retracted" => "tag-error",
                        _ => "tag-info",
                    };
                    let id2 = id.clone();
                    let id_pub = id.clone();
                    let id_ret = id.clone();
                    let refresh_pub = refresh.clone();
                    let refresh_ret = refresh.clone();
                    let load = load_steps.clone();
                    let can_pub = can_publish();
                    let is_draft = status == "draft" || status == "retracted";
                    let is_published = status == "published";
                    view! {
                        <div class="list-item">
                            <strong>{code}</strong>
                            <span class={format!("tag {}", status_tag)}>{status}</span>
                            <span class="muted">" v"{c.version.to_string()}</span>
                            <button class="btn btn-sm" on:click=move |_| load(id2.clone())>"Steps"</button>
                            {(can_pub && is_draft).then(|| {
                                let id = id_pub.clone();
                                let refresh = refresh_pub.clone();
                                let set_msg = set_msg;
                                let set_err = set_err;
                                view! {
                                    <button class="btn btn-sm" on:click=move |_| {
                                        let id = id.clone();
                                        let refresh = refresh.clone();
                                        spawn_local(async move {
                                            match client::publish_traceability(&id, "Published via UI").await {
                                                Ok(_) => { set_msg.set(Some("Published".into())); refresh(); }
                                                Err(e) => set_err.set(Some(e.message)),
                                            }
                                        });
                                    }>"Publish"</button>
                                }
                            })}
                            {(can_pub && is_published).then(|| {
                                let id = id_ret.clone();
                                let refresh = refresh_ret.clone();
                                let set_msg = set_msg;
                                let set_err = set_err;
                                view! {
                                    <button class="btn btn-sm btn-danger" on:click=move |_| {
                                        let id = id.clone();
                                        let refresh = refresh.clone();
                                        spawn_local(async move {
                                            match client::retract_traceability(&id, "Retracted via UI").await {
                                                Ok(_) => { set_msg.set(Some("Retracted".into())); refresh(); }
                                                Err(e) => set_err.set(Some(e.message)),
                                            }
                                        });
                                    }>"Retract"</button>
                                }
                            })}
                        </div>
                    }
                }).collect_view()}
            </div>

            // Timeline panel for selected code
            {move || selected.get().map(|sid| {
                let step_list = steps.get();
                view! {
                    <section class="ws-section">
                        <h3>"Timeline for "{sid}</h3>
                        {if step_list.is_empty() {
                            view! { <p class="muted">"No steps recorded."</p> }.into_view()
                        } else {
                            step_list.into_iter().map(|s| view! {
                                <div class="ws-row">
                                    <span class="tag tag-info">{s.step_type}</span>
                                    <strong>{s.step_label}</strong>
                                    <span class="muted">" "{s.details}</span>
                                    <span class="muted">" "{s.occurred_at}</span>
                                </div>
                            }).collect_view()
                        }}
                    </section>
                }
            })}
        </div>
    }
}

#[component]
fn CreateCodeForm<F: Fn(String) + Clone + 'static>(on_done: F) -> impl IntoView {
    let (intake_id, set_intake_id) = create_signal(String::new());
    let (err, set_err) = create_signal(Option::<String>::None);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_err.set(None);
        let iid = intake_id.get();
        let intake = if iid.is_empty() { None } else { Some(iid) };
        let req = TraceCodeRequest { intake_id: intake };
        let on_done = on_done.clone();
        spawn_local(async move {
            match client::create_traceability(&req).await {
                Ok(r) => on_done(r.code),
                Err(e) => set_err.set(Some(e.message)),
            }
        });
    };

    view! {
        <form class="addr-form" on:submit=submit>
            {move || err.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}
            <input placeholder="Intake ID (optional)" prop:value=intake_id
                on:input=move |e| set_intake_id.set(event_target_value(&e)) />
            <button type="submit" class="btn">"Generate"</button>
        </form>
    }
}
