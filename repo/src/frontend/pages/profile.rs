use leptos::*;
use crate::api::client;
use fieldtrace_shared::UserResponse;

#[component]
pub fn ProfilePage(user: ReadSignal<Option<UserResponse>>) -> impl IntoView {
    let (msg, set_msg) = create_signal(Option::<String>::None);
    let (err, set_err) = create_signal(Option::<String>::None);
    let (show_pw, set_show_pw) = create_signal(false);

    let request_delete = move |_| {
        set_err.set(None);
        spawn_local(async move {
            match client::request_account_deletion().await {
                Ok(v) => set_msg.set(Some(
                    v.get("message").and_then(|m| m.as_str()).unwrap_or("Deletion requested").to_string(),
                )),
                Err(e) => set_err.set(Some(e.message)),
            }
        });
    };

    let cancel_delete = move |_| {
        set_err.set(None);
        spawn_local(async move {
            match client::cancel_account_deletion().await {
                Ok(_) => set_msg.set(Some("Deletion cancelled".into())),
                Err(e) => set_err.set(Some(e.message)),
            }
        });
    };

    view! {
        <div class="card">
            <h2>"Profile & Privacy"</h2>
            {move || msg.get().map(|m| view! { <div class="msg msg-info">{m}</div> })}
            {move || err.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}

            {move || user.get().map(|u| view! {
                <div class="profile-info">
                    <p><strong>"Username: "</strong>{u.username.clone()}</p>
                    <p><strong>"Role: "</strong>{u.role.clone()}</p>
                    <p><strong>"Account Created: "</strong>{u.created_at.clone()}</p>
                </div>
            })}

            <section class="ws-section">
                <h3>"Change Password"</h3>
                <button class="btn btn-sm" on:click=move |_| set_show_pw.update(|v| *v = !*v)>
                    {move || if show_pw.get() { "Cancel" } else { "Change Password" }}
                </button>
                {move || show_pw.get().then(|| {
                    let set_msg = set_msg;
                    let set_err = set_err;
                    let set_show = set_show_pw;
                    view! { <ChangePasswordForm
                        on_done=move || { set_msg.set(Some("Password changed".into())); set_show.set(false); }
                        on_error=move |e: String| set_err.set(Some(e))
                    /> }
                })}
            </section>

            <section class="ws-section">
                <h3>"Privacy & Data"</h3>
                <p class="muted">"Address book data is encrypted at rest (AES-256-GCM). \
                    Sensitive fields (street, city, phone) are masked in the UI to reduce incidental exposure."</p>
                <p class="muted">"Audit logs are append-only and cannot be altered. \
                    Log exports redact sensitive content automatically."</p>
            </section>

            <section class="ws-section">
                <h3>"Account Lifecycle"</h3>
                <p class="muted">"Requesting deletion starts a 7-day cooling-off window. \
                    You can cancel anytime during that period. After 7 days, your account \
                    is anonymized (username replaced, address book cleared, sessions deleted)."</p>
                <button class="btn btn-danger" on:click=request_delete>"Request Account Deletion"</button>
                <button class="btn" on:click=cancel_delete>"Cancel Deletion"</button>
            </section>
        </div>
    }
}

#[component]
fn ChangePasswordForm<F, E>(on_done: F, on_error: E) -> impl IntoView
where
    F: Fn() + Clone + 'static,
    E: Fn(String) + Clone + 'static,
{
    let (current, set_current) = create_signal(String::new());
    let (new_pw, set_new_pw) = create_signal(String::new());
    let (confirm, set_confirm) = create_signal(String::new());
    let (err, set_err) = create_signal(Option::<String>::None);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_err.set(None);
        if new_pw.get() != confirm.get() {
            set_err.set(Some("Passwords do not match".into()));
            return;
        }
        if new_pw.get().len() < 12 {
            set_err.set(Some("Password must be at least 12 characters".into()));
            return;
        }
        let on_done = on_done.clone();
        let on_error = on_error.clone();
        spawn_local(async move {
            match client::change_password(&current.get(), &new_pw.get()).await {
                Ok(_) => on_done(),
                Err(e) => on_error(e.message),
            }
        });
    };

    view! {
        <form class="addr-form" on:submit=submit>
            {move || err.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}
            <input type="password" placeholder="Current Password" prop:value=current
                on:input=move |e| set_current.set(event_target_value(&e)) required=true />
            <input type="password" placeholder="New Password (min 12 chars)" prop:value=new_pw
                on:input=move |e| set_new_pw.set(event_target_value(&e)) required=true />
            <input type="password" placeholder="Confirm New Password" prop:value=confirm
                on:input=move |e| set_confirm.set(event_target_value(&e)) required=true />
            <button type="submit" class="btn">"Update Password"</button>
        </form>
    }
}
