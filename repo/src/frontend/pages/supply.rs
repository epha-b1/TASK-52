use leptos::*;
use crate::api::client;
use fieldtrace_shared::{SupplyRequest, SupplyResolveRequest, SupplyResponse};

#[component]
pub fn SupplyPage() -> impl IntoView {
    let (entries, set_entries) = create_signal(Vec::<SupplyResponse>::new());
    let (show_form, set_show_form) = create_signal(false);
    let (err, set_err) = create_signal(Option::<String>::None);

    let refresh = move || {
        spawn_local(async move {
            match client::list_supply().await {
                Ok(list) => set_entries.set(list),
                Err(e) => set_err.set(Some(e.message)),
            }
        });
    };
    refresh();

    view! {
        <div class="card">
            <h2>"Supply Entries"</h2>
            {move || err.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}
            <button class="btn" on:click=move |_| set_show_form.update(|v| *v = !*v)>
                {move || if show_form.get() { "Cancel" } else { "New Supply Entry" }}
            </button>
            {move || show_form.get().then(|| {
                let refresh = refresh.clone();
                let set_show = set_show_form;
                view! { <SupplyForm on_done=move || { refresh(); set_show.set(false); } /> }
            })}
            <div class="list">
                {move || entries.get().into_iter().map(|s| {
                    let id = s.id.clone();
                    let needs_review = s.parse_status == "needs_review";
                    let refresh = refresh.clone();
                    let status_tag = if needs_review { "tag-error" } else { "tag-ok" };
                    view! {
                        <div class="list-item">
                            <strong>{s.name.clone()}</strong>
                            {s.sku.clone().map(|sku| view! { <span class="muted">" SKU: "{sku}</span> })}
                            <span class={format!("tag {}", status_tag)}>{s.parse_status.clone()}</span>
                            {s.canonical_color.clone().map(|c| view! { <span class="tag tag-info">{c}</span> })}
                            {s.canonical_size.clone().map(|sz| view! { <span class="tag tag-info">{sz}</span> })}
                            {needs_review.then(|| {
                                let id = id.clone();
                                let refresh = refresh.clone();
                                view! {
                                    <button class="btn btn-sm" on:click=move |_| {
                                        let id = id.clone();
                                        let refresh = refresh.clone();
                                        spawn_local(async move {
                                            let req = SupplyResolveRequest {
                                                canonical_color: None,
                                                canonical_size: None,
                                            };
                                            let _ = client::resolve_supply(&id, &req).await;
                                            refresh();
                                        });
                                    }>"Resolve"</button>
                                }
                            })}
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}

#[component]
fn SupplyForm<F: Fn() + Clone + 'static>(on_done: F) -> impl IntoView {
    let (name, set_name) = create_signal(String::new());
    let (sku, set_sku) = create_signal(String::new());
    let (size, set_size) = create_signal(String::new());
    let (color, set_color) = create_signal(String::new());
    let (price, set_price) = create_signal(String::new());
    let (notes, set_notes) = create_signal(String::new());
    let (err, set_err) = create_signal(Option::<String>::None);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_err.set(None);
        let price_cents = price.get().parse::<i64>().ok();
        let sku_val = {
            let s = sku.get();
            if s.is_empty() { None } else { Some(s) }
        };
        let req = SupplyRequest {
            name: name.get(),
            sku: sku_val,
            size: size.get(),
            color: color.get(),
            price_cents,
            discount_cents: None,
            notes: notes.get(),
        };
        let on_done = on_done.clone();
        spawn_local(async move {
            match client::create_supply(&req).await {
                Ok(_) => on_done(),
                Err(e) => set_err.set(Some(e.message)),
            }
        });
    };

    view! {
        <form class="addr-form" on:submit=submit>
            {move || err.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}
            <input placeholder="Name" prop:value=name
                on:input=move |e| set_name.set(event_target_value(&e)) required=true />
            <input placeholder="SKU (optional)" prop:value=sku
                on:input=move |e| set_sku.set(event_target_value(&e)) />
            <input placeholder="Size (e.g. large, XL)" prop:value=size
                on:input=move |e| set_size.set(event_target_value(&e)) required=true />
            <input placeholder="Color (e.g. red, blue)" prop:value=color
                on:input=move |e| set_color.set(event_target_value(&e)) required=true />
            <input placeholder="Price (cents)" prop:value=price type="number"
                on:input=move |e| set_price.set(event_target_value(&e)) />
            <input placeholder="Notes" prop:value=notes
                on:input=move |e| set_notes.set(event_target_value(&e)) />
            <button type="submit" class="btn">"Create Supply Entry"</button>
        </form>
    }
}
