//! Admin Operations page — visible only to administrators.
//! Provides config view, version history/rollback, diagnostics export,
//! jobs report, and structured logs view.

use leptos::*;
use crate::api::client;

#[component]
pub fn AdminPage() -> impl IntoView {
    let (msg, set_msg) = create_signal(Option::<String>::None);
    let (err, set_err) = create_signal(Option::<String>::None);
    let (loading, set_loading) = create_signal(false);

    // Config state
    let (config, set_config) = create_signal(Option::<serde_json::Value>::None);
    let (versions, set_versions) = create_signal(Vec::<serde_json::Value>::new());
    let (jobs, set_jobs) = create_signal(Vec::<serde_json::Value>::new());
    let (logs, set_logs) = create_signal(Vec::<serde_json::Value>::new());

    // Load all admin data on mount
    spawn_local(async move {
        if let Ok(c) = client::admin_get_config().await { set_config.set(Some(c)); }
        if let Ok(v) = client::admin_config_versions().await {
            if let Some(arr) = v.as_array() { set_versions.set(arr.clone()); }
        }
        if let Ok(j) = client::admin_jobs().await {
            if let Some(arr) = j.as_array() { set_jobs.set(arr.clone()); }
        }
        if let Ok(l) = client::admin_logs().await {
            if let Some(arr) = l.as_array() { set_logs.set(arr.clone()); }
        }
    });

    // Rollback action
    let do_rollback = move |version_id: String| {
        set_loading.set(true);
        set_err.set(None);
        spawn_local(async move {
            match client::admin_rollback(&version_id).await {
                Ok(_) => {
                    set_msg.set(Some(format!("Rolled back to version {}", version_id)));
                    // Refresh config
                    if let Ok(c) = client::admin_get_config().await { set_config.set(Some(c)); }
                    if let Ok(v) = client::admin_config_versions().await {
                        if let Some(arr) = v.as_array() { set_versions.set(arr.clone()); }
                    }
                }
                Err(e) => set_err.set(Some(e.message)),
            }
            set_loading.set(false);
        });
    };

    // Export diagnostics — triggers ZIP creation and stores download URL
    let (diag_url, set_diag_url) = create_signal(Option::<String>::None);
    let do_export = move |_| {
        set_loading.set(true);
        set_err.set(None);
        set_diag_url.set(None);
        spawn_local(async move {
            match client::admin_export_diagnostics().await {
                Ok(resp) => {
                    let dl_url = resp.get("download_url").and_then(|v| v.as_str())
                        .unwrap_or("/admin/diagnostics/download/unknown").to_string();
                    set_diag_url.set(Some(dl_url.clone()));
                    set_msg.set(Some("Diagnostics ZIP ready for download.".into()));
                }
                Err(e) => set_err.set(Some(e.message)),
            }
            set_loading.set(false);
        });
    };

    view! {
        <div class="card">
            <h2>"Admin Operations"</h2>
            {move || msg.get().map(|m| view! { <div class="msg msg-info">{m}</div> })}
            {move || err.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}

            // ── Current Config ─────────────────────────────────────────
            <section class="ws-section">
                <h3>"Current Configuration"</h3>
                {move || config.get().map(|c| {
                    let text = serde_json::to_string_pretty(&c).unwrap_or_default();
                    view! { <pre class="config-snapshot">{text}</pre> }
                })}
            </section>

            // ── Config Versions + Rollback ─────────────────────────────
            <section class="ws-section">
                <h3>"Config Version History"</h3>
                {move || {
                    let vs = versions.get();
                    if vs.is_empty() {
                        view! { <p class="muted">"No config versions yet."</p> }.into_view()
                    } else {
                        vs.into_iter().map(|v| {
                            let id = v.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
                            let id_str = id.to_string();
                            let created = v.get("created_at").and_then(|c| c.as_str())
                                .unwrap_or("").to_string();
                            let do_rollback = do_rollback.clone();
                            view! {
                                <div class="ws-row">
                                    <strong>{format!("Version #{}", id)}</strong>
                                    <span class="muted">" "{created}</span>
                                    <button class="btn btn-sm" on:click=move |_| {
                                        do_rollback(id_str.clone());
                                    } disabled=move || loading.get()>"Rollback"</button>
                                </div>
                            }
                        }).collect_view()
                    }
                }}
            </section>

            // ── Diagnostics Export + Download ──────────────────────────
            <section class="ws-section">
                <h3>"Diagnostics"</h3>
                <button class="btn" on:click=do_export disabled=move || loading.get()>
                    {move || if loading.get() { "Exporting..." } else { "Export Diagnostics ZIP" }}
                </button>
                {move || diag_url.get().map(|url| view! {
                    <a class="btn btn-sm" href={url.clone()} download="diagnostics.zip"
                       target="_blank">"Download ZIP"</a>
                })}
            </section>

            // ── Background Jobs ────────────────────────────────────────
            <section class="ws-section">
                <h3>"Background Jobs"</h3>
                {move || {
                    let js = jobs.get();
                    if js.is_empty() {
                        view! { <p class="muted">"No job metrics recorded yet."</p> }.into_view()
                    } else {
                        js.into_iter().map(|j| {
                            let name = j.get("job_name").and_then(|n| n.as_str())
                                .unwrap_or("-").to_string();
                            let status = j.get("status").and_then(|s| s.as_str())
                                .unwrap_or("-").to_string();
                            let last_run = j.get("last_run_at").and_then(|l| l.as_str())
                                .unwrap_or("").to_string();
                            let tag = if status == "ok" { "tag-ok" } else { "tag-error" };
                            view! {
                                <div class="ws-row">
                                    <strong>{name}</strong>
                                    <span class={format!("tag {}", tag)}>{status}</span>
                                    <span class="muted">" "{last_run}</span>
                                </div>
                            }
                        }).collect_view()
                    }
                }}
            </section>

            // ── Structured Logs ────────────────────────────────────────
            <section class="ws-section">
                <h3>"Recent Logs"</h3>
                {move || {
                    let ls = logs.get();
                    if ls.is_empty() {
                        view! { <p class="muted">"No logs."</p> }.into_view()
                    } else {
                        ls.into_iter().take(50).map(|l| {
                            let level = l.get("level").and_then(|v| v.as_str())
                                .unwrap_or("info").to_string();
                            let message = l.get("message").and_then(|v| v.as_str())
                                .unwrap_or("").to_string();
                            let at = l.get("created_at").and_then(|v| v.as_str())
                                .unwrap_or("").to_string();
                            let tag = match level.as_str() {
                                "error" => "tag-error",
                                "warn" => "tag-warn",
                                _ => "tag-info",
                            };
                            view! {
                                <div class="ws-row log-entry">
                                    <span class={format!("tag {}", tag)}>{level}</span>
                                    <span>{message}</span>
                                    <span class="muted">" "{at}</span>
                                </div>
                            }
                        }).collect_view()
                    }
                }}
            </section>
        </div>
    }
}
