use leptos::*;
use crate::api::client;
use crate::draft;
use fieldtrace_shared::{AddressRequest, AddressResponse};

const ADDRESS_FORM_ID: &str = "address-form";

#[component]
pub fn AddressBookPage() -> impl IntoView {
    let (entries, set_entries) = create_signal(Vec::<AddressResponse>::new());
    let (error, set_error) = create_signal(Option::<String>::None);
    let (show_form, set_show_form) = create_signal(false);

    // Load entries on mount
    {
        let set_entries = set_entries;
        let set_error = set_error;
        spawn_local(async move {
            match client::list_addresses().await {
                Ok(list) => set_entries.set(list),
                Err(e) => set_error.set(Some(e.message)),
            }
        });
    }

    let refresh = move || {
        spawn_local(async move {
            if let Ok(list) = client::list_addresses().await {
                set_entries.set(list);
            }
        });
    };

    view! {
        <div class="card">
            <h2>"Address Book"</h2>
            {move || error.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}
            <button class="btn" on:click=move |_| set_show_form.update(|v| *v = !*v)>
                {move || if show_form.get() { "Cancel" } else { "Add Address" }}
            </button>
            {move || show_form.get().then(|| {
                let refresh = refresh.clone();
                let set_show_form = set_show_form;
                view! { <AddressForm on_done=move || { refresh(); set_show_form.set(false); } /> }
            })}
            <div class="addr-list">
                {move || entries.get().into_iter().map(|a| {
                    let id = a.id.clone();
                    let refresh = refresh.clone();
                    view! {
                        <div class="addr-item">
                            <strong>{a.label}</strong>
                            <p>{a.street.clone()}", "{a.city.clone()}", "{a.state.clone()}" "{a.zip_plus4.clone()}</p>
                            <p>"Phone: "{a.phone_masked.clone()}</p>
                            <button class="btn btn-sm btn-danger" on:click=move |_| {
                                let id = id.clone();
                                let refresh = refresh.clone();
                                spawn_local(async move {
                                    let _ = client::delete_address(&id).await;
                                    refresh();
                                });
                            }>"Delete"</button>
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}

#[component]
fn AddressForm<F: Fn() + Clone + 'static>(on_done: F) -> impl IntoView {
    // Restore from any draft preserved across session expiry.
    let restored = draft::load_draft(ADDRESS_FORM_ID);
    let pick = |key: &str| -> String {
        restored.as_ref()
            .and_then(|v| v.get(key).and_then(|s| s.as_str().map(String::from)))
            .unwrap_or_default()
    };
    let (label, set_label) = create_signal(pick("label"));
    let (street, set_street) = create_signal(pick("street"));
    let (city, set_city) = create_signal(pick("city"));
    let (st, set_st) = create_signal(pick("state"));
    let (zip, set_zip) = create_signal(pick("zip_plus4"));
    let (phone, set_phone) = create_signal(pick("phone"));
    let (err, set_err) = create_signal(Option::<String>::None);

    // Autosave every field on input change.
    create_effect(move |_| {
        let snap = serde_json::json!({
            "label": label.get(), "street": street.get(), "city": city.get(),
            "state": st.get(), "zip_plus4": zip.get(), "phone": phone.get(),
        });
        draft::save_draft(ADDRESS_FORM_ID, snap);
    });

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_err.set(None);
        let req = AddressRequest {
            label: label.get(), street: street.get(), city: city.get(),
            state: st.get(), zip_plus4: zip.get(), phone: phone.get(),
        };
        let on_done = on_done.clone();
        spawn_local(async move {
            match client::create_address(&req).await {
                Ok(_) => {
                    draft::clear_draft(ADDRESS_FORM_ID);
                    on_done();
                }
                Err(e) => set_err.set(Some(e.message)),
            }
        });
    };

    view! {
        <form class="addr-form" on:submit=submit>
            {move || err.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}
            <input placeholder="Label" prop:value=label on:input=move |e| set_label.set(event_target_value(&e)) required=true />
            <input placeholder="Street" prop:value=street on:input=move |e| set_street.set(event_target_value(&e)) required=true />
            <input placeholder="City" prop:value=city on:input=move |e| set_city.set(event_target_value(&e)) required=true />
            <input placeholder="State" prop:value=st on:input=move |e| set_st.set(event_target_value(&e)) required=true />
            <input placeholder="ZIP+4 (NNNNN-NNNN)" prop:value=zip on:input=move |e| set_zip.set(event_target_value(&e)) required=true />
            <input placeholder="Phone" prop:value=phone on:input=move |e| set_phone.set(event_target_value(&e)) required=true />
            <button type="submit" class="btn">"Save Address"</button>
        </form>
    }
}
