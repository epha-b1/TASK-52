//! Evidence capture/upload flow.
//!
//! Operations Staff and Administrators can upload photo/video/audio evidence
//! through this page. The file is split into 2 MiB chunks, base64-encoded,
//! and sent sequentially via the `/media/upload/start|chunk|complete` API.
//! Auditors see no upload controls (role-aware gating).

use leptos::*;
use crate::api::client;
use fieldtrace_shared::*;

const CHUNK_SIZE: usize = 2 * 1024 * 1024; // 2 MiB

#[component]
pub fn EvidenceUploadPage(user: ReadSignal<Option<UserResponse>>) -> impl IntoView {
    let is_auditor = move || {
        user.get().map(|u| u.role == "auditor").unwrap_or(true)
    };

    // Form fields
    let (media_type, set_media_type) = create_signal("photo".to_string());
    let (tags, set_tags) = create_signal(String::new());
    let (keyword, set_keyword) = create_signal(String::new());
    let (exif_time, set_exif_time) = create_signal(String::new());

    // File data (stored as base64 chunks in memory)
    let (file_name, set_file_name) = create_signal(String::new());
    let (file_bytes, set_file_bytes) = create_signal(Option::<Vec<u8>>::None);

    // Progress / feedback
    let (uploading, set_uploading) = create_signal(false);
    let (progress, set_progress) = create_signal(String::new());
    let (error, set_error) = create_signal(Option::<String>::None);
    let (success, set_success) = create_signal(Option::<String>::None);

    // File input handler — reads file into memory via FileReader API
    let on_file_change = move |ev: leptos::ev::Event| {
        use wasm_bindgen::JsCast;
        set_error.set(None);
        set_success.set(None);

        let input: web_sys::HtmlInputElement = ev.target().unwrap().unchecked_into();
        let files = input.files().unwrap();
        if files.length() == 0 { return; }
        let file = files.get(0).unwrap();
        set_file_name.set(file.name());

        let reader = web_sys::FileReader::new().unwrap();
        let reader_clone = reader.clone();
        let onload = wasm_bindgen::closure::Closure::wrap(Box::new(move |_: web_sys::Event| {
            let result = reader_clone.result().unwrap();
            let array_buffer = result.dyn_into::<js_sys::ArrayBuffer>().unwrap();
            let uint8 = js_sys::Uint8Array::new(&array_buffer);
            let mut bytes = vec![0u8; uint8.length() as usize];
            uint8.copy_to(&mut bytes);
            set_file_bytes.set(Some(bytes));
        }) as Box<dyn Fn(_)>);
        reader.set_onload(Some(onload.as_ref().unchecked_ref()));
        onload.forget(); // prevent GC
        reader.read_as_array_buffer(&file).unwrap();
    };

    let do_upload = move |_| {
        let bytes = match file_bytes.get() {
            Some(b) if !b.is_empty() => b,
            _ => {
                set_error.set(Some("Please select a file first".into()));
                return;
            }
        };
        let fname = file_name.get();
        let mt = media_type.get();
        let total_size = bytes.len() as i64;
        let tag_val = tags.get();
        let kw_val = keyword.get();
        let exif_val = exif_time.get();

        set_uploading.set(true);
        set_error.set(None);
        set_success.set(None);
        set_progress.set("Starting upload...".into());

        spawn_local(async move {
            // Step 1: Start upload session
            let start_req = UploadStartRequest {
                filename: fname.clone(),
                media_type: mt.clone(),
                total_size,
                duration_seconds: 0,
            };
            let start_resp = match client::upload_start(&start_req).await {
                Ok(r) => r,
                Err(e) => {
                    set_error.set(Some(format!("Start failed: {}", e.message)));
                    set_uploading.set(false);
                    return;
                }
            };

            let upload_id = start_resp.upload_id;
            let total_chunks = start_resp.total_chunks;

            // Step 2: Send chunks sequentially
            for idx in 0..total_chunks {
                let chunk_start = (idx as usize) * CHUNK_SIZE;
                let chunk_end = ((idx as usize + 1) * CHUNK_SIZE).min(bytes.len());
                let chunk_data = &bytes[chunk_start..chunk_end];

                // Base64 encode chunk
                use base64::Engine;
                let b64 = base64::engine::general_purpose::STANDARD.encode(chunk_data);

                set_progress.set(format!("Uploading chunk {}/{}...", idx + 1, total_chunks));

                let chunk_req = UploadChunkRequest {
                    upload_id: upload_id.clone(),
                    chunk_index: idx,
                    data: b64,
                };
                if let Err(e) = client::upload_chunk(&chunk_req).await {
                    set_error.set(Some(format!("Chunk {} failed: {}", idx, e.message)));
                    set_uploading.set(false);
                    return;
                }
            }

            // Step 3: Generate fingerprint (simple hash of first+last bytes)
            let fingerprint = {
                let mut h: u64 = 0xcbf29ce484222325; // FNV-1a offset
                for &b in bytes.iter() {
                    h ^= b as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
                format!("{:016x}", h)
            };

            // Step 4: Complete
            set_progress.set("Finalizing upload...".into());
            let exif = if exif_val.is_empty() { None } else { Some(exif_val) };
            let tags_opt = if tag_val.is_empty() { None } else { Some(tag_val) };
            let kw_opt = if kw_val.is_empty() { None } else { Some(kw_val) };

            let complete_req = UploadCompleteRequest {
                upload_id,
                fingerprint,
                total_size,
                exif_capture_time: exif,
                tags: tags_opt,
                keyword: kw_opt,
            };
            match client::upload_complete(&complete_req).await {
                Ok(resp) => {
                    set_success.set(Some(format!(
                        "Upload complete! Evidence ID: {} ({})",
                        resp.id, resp.media_type
                    )));
                    set_file_bytes.set(None);
                    set_file_name.set(String::new());
                }
                Err(e) => {
                    set_error.set(Some(format!("Finalize failed: {}", e.message)));
                }
            }
            set_uploading.set(false);
            set_progress.set(String::new());
        });
    };

    view! {
        <div class="card">
            <h2>"Evidence Upload"</h2>

            // Role gate: auditors cannot upload
            {move || if is_auditor() {
                view! {
                    <p class="muted">"Auditors have read-only access. Upload is not available."</p>
                }.into_view()
            } else {
                view! {
                    <div class="evidence-upload-form">
                        {move || error.get().map(|e| view! { <div class="msg msg-error">{e}</div> })}
                        {move || success.get().map(|s| view! { <div class="msg msg-info">{s}</div> })}

                        <div class="form-group">
                            <label>"Media Type"</label>
                            <select on:change=move |e| set_media_type.set(event_target_value(&e))>
                                <option value="photo">"Photo"</option>
                                <option value="video">"Video"</option>
                                <option value="audio">"Audio"</option>
                            </select>
                        </div>

                        <div class="form-group">
                            <label>"File"</label>
                            <input type="file"
                                on:change=on_file_change
                                disabled=move || uploading.get() />
                            {move || {
                                let name = file_name.get();
                                let size_info = file_bytes.get().map(|b| format!(" ({} bytes)", b.len())).unwrap_or_default();
                                if name.is_empty() { None } else {
                                    Some(view! { <span class="muted">{format!("{}{}", name, size_info)}</span> })
                                }
                            }}
                        </div>

                        <div class="form-group">
                            <label>"Tags (optional)"</label>
                            <input placeholder="comma-separated tags" prop:value=tags
                                on:input=move |e| set_tags.set(event_target_value(&e))
                                disabled=move || uploading.get() />
                        </div>

                        <div class="form-group">
                            <label>"Keyword (optional)"</label>
                            <input placeholder="search keyword" prop:value=keyword
                                on:input=move |e| set_keyword.set(event_target_value(&e))
                                disabled=move || uploading.get() />
                        </div>

                        <div class="form-group">
                            <label>"EXIF Capture Time (optional)"</label>
                            <input type="datetime-local" prop:value=exif_time
                                on:input=move |e| set_exif_time.set(event_target_value(&e))
                                disabled=move || uploading.get() />
                        </div>

                        <button class="btn" on:click=do_upload
                            disabled=move || uploading.get() || file_bytes.get().is_none()>
                            {move || if uploading.get() { "Uploading..." } else { "Upload Evidence" }}
                        </button>

                        {move || {
                            let p = progress.get();
                            if p.is_empty() { None } else {
                                Some(view! {
                                    <div class="upload-progress">
                                        <span class="status-indicator status-loading">
                                            <span class="dot dot-loading"></span>
                                            {p}
                                        </span>
                                    </div>
                                })
                            }
                        }}
                    </div>
                }.into_view()
            }}
        </div>
    }
}
