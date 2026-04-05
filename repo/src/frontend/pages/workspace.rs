//! Operational workspace with today's intake, transfer queue, pending
//! inspections, and exceptions. Each section displays feedback states
//! ("saved locally", "needs review", "blocked by policy") derived from
//! backend responses.

use leptos::*;
use crate::api::client;
use fieldtrace_shared::{InspectionResponse, IntakeResponse, TransferResponse};

#[component]
pub fn WorkspacePage() -> impl IntoView {
    let (intake, set_intake) = create_signal(Vec::<IntakeResponse>::new());
    let (inspections, set_inspections) = create_signal(Vec::<InspectionResponse>::new());
    // Transfer queue is now sourced from the real /transfers endpoint, not
    // from filtering intake status.
    let (transfers, set_transfers) = create_signal(Vec::<TransferResponse>::new());
    let (feedback, set_feedback) = create_signal(String::new());
    let (biometric_on, set_biometric_on) = create_signal(false);

    // Load on mount
    spawn_local(async move {
        match client::list_intake().await {
            Ok(list) => { set_intake.set(list); set_feedback.set("Saved locally".into()); }
            Err(e) => set_feedback.set(format!("Error: {}", e.message)),
        }
    });
    spawn_local(async move {
        if let Ok(list) = client::list_inspections().await {
            set_inspections.set(list);
        }
    });
    spawn_local(async move {
        if let Ok(list) = client::list_transfers().await {
            set_transfers.set(list);
        }
    });

    view! {
        <div class="card workspace">
            <h2>"Operational Workspace"</h2>
            <div class="feedback-bar">
                <span class="feedback-state">{move || feedback.get()}</span>
            </div>

            // ── Today's Intake ──────────────────────────────────────
            <section class="ws-section">
                <h3>"Today's Intake"</h3>
                {move || {
                    let items = intake.get();
                    if items.is_empty() {
                        view! { <p class="muted">"No intake records yet."</p> }.into_view()
                    } else {
                        items.into_iter().take(10).map(|i| {
                            let status_tag = match i.status.as_str() {
                                "received" => "tag-info",
                                "in_care" => "tag-info",
                                "adopted" => "tag-ok",
                                "disposed" => "tag-default",
                                _ => "tag-default",
                            };
                            view! {
                                <div class="ws-row">
                                    <strong>{i.intake_type.clone()}</strong>
                                    <span class={format!("tag {}", status_tag)}>{i.status.clone()}</span>
                                    <span class="muted">" "{i.created_at.clone()}</span>
                                </div>
                            }
                        }).collect_view()
                    }
                }}
            </section>

            // ── Transfer Queue ──────────────────────────────────────
            // Sourced from the /transfers first-class endpoint (NOT from
            // filtering intake status) so lifecycle states come from the
            // transfers table directly.
            <section class="ws-section">
                <h3>"Transfer Queue"</h3>
                {move || {
                    let queue = transfers.get();
                    if queue.is_empty() {
                        view! { <p class="muted">"Transfer queue is empty."</p> }.into_view()
                    } else {
                        queue.into_iter().map(|t| {
                            let status_tag = match t.status.as_str() {
                                "queued" => "tag-info",
                                "approved" => "tag-info",
                                "in_transit" => "tag-info",
                                "received" => "tag-ok",
                                "canceled" => "tag-default",
                                _ => "tag-default",
                            };
                            view! {
                                <div class="ws-row">
                                    <strong>{t.destination.clone()}</strong>
                                    <span class={format!("tag {}", status_tag)}>{t.status.clone()}</span>
                                    <span class="muted">" "{t.reason.clone()}</span>
                                </div>
                            }
                        }).collect_view()
                    }
                }}
            </section>

            // ── Pending Inspections ─────────────────────────────────
            <section class="ws-section">
                <h3>"Pending Inspections"</h3>
                {move || {
                    let pending: Vec<_> = inspections.get().into_iter()
                        .filter(|i| i.status == "pending")
                        .collect();
                    if pending.is_empty() {
                        view! { <p class="muted">"All inspections resolved."</p> }.into_view()
                    } else {
                        pending.into_iter().map(|i| view! {
                            <div class="ws-row">
                                <strong>"Inspection for intake "{i.intake_id.clone()}</strong>
                                <span class="tag tag-info">"pending"</span>
                            </div>
                        }).collect_view()
                    }
                }}
            </section>

            // ── Exceptions ──────────────────────────────────────────
            <section class="ws-section">
                <h3>"Exceptions"</h3>
                {move || {
                    let exc: Vec<_> = inspections.get().into_iter()
                        .filter(|i| i.status == "failed")
                        .collect();
                    if exc.is_empty() {
                        view! { <p class="muted">"No exceptions."</p> }.into_view()
                    } else {
                        exc.into_iter().map(|i| view! {
                            <div class="ws-row ws-exception">
                                <span class="tag tag-error">"failed"</span>
                                <span>{i.outcome_notes.clone()}</span>
                            </div>
                        }).collect_view()
                    }
                }}
            </section>

            // ── Feedback state legend ───────────────────────────────
            <section class="ws-section">
                <h3>"State Legend"</h3>
                <div class="legend">
                    <span class="tag tag-ok">"saved locally"</span>
                    <span class="tag tag-info">"needs review"</span>
                    <span class="tag tag-error">"blocked by policy"</span>
                </div>
            </section>

            // ── Placeholder biometric toggle (no processing) ────────
            <section class="ws-section">
                <h3>"Biometric Capture (Placeholder)"</h3>
                <label class="bio-toggle">
                    <input type="checkbox"
                           prop:checked=biometric_on
                           on:change=move |ev| set_biometric_on.set(event_target_checked(&ev)) />
                    <span>"Enable facial recognition capture"</span>
                </label>
                <p class="muted">
                    {move || if biometric_on.get() {
                        "Biometric capture UI enabled — no processing performed (placeholder only)."
                    } else {
                        "Biometric capture is disabled."
                    }}
                </p>
            </section>
        </div>
    }
}
