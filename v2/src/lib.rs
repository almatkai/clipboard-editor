//! Compiled-in clipboard editor. All OS effects go through the supplied host.
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

pub const ICON: &[u8] = include_bytes!("../assets/icon.png");
pub const MANIFEST: &str = include_str!("../manifest.json");
pub type HostFuture<'a> = Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'a>>;

pub trait Host {
    fn request<'a>(&'a self, route: &'a str, args: Vec<Value>) -> HostFuture<'a>;
}

pub fn commands() -> &'static [&'static str] {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        &["edit-clipboard-text"]
    } else {
        &[]
    }
}

// A launch nonce also separates independent host sessions whose revisions both
// start at one. State retains its revision tombstone when disposed.
static NEXT_LAUNCH: AtomicU64 = AtomicU64::new(1);

pub async fn handle(
    host: &impl Host,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> Result<Value, String> {
    if !commands().contains(&command) {
        return Err(format!("Unsupported clipboard editor command: {command}"));
    }
    match operation {
        "extension:run-command" => {
            let revision = state["revision"]
                .as_u64()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("Clipboard editor revision exhausted")?;
            let nonce = NEXT_LAUNCH
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .map_err(|_| "Clipboard editor launch IDs exhausted")?;
            *state = json!({"active":true,"revision":revision,"nonce":nonce,"draft":""});
            let read = request(host, "clipboard:read", vec![]).await.and_then(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "Clipboard read did not return text".into())
            });
            let mut view = form(state);
            match read {
                Ok(text) => {
                    state["draft"] = json!(text);
                    view = form(state);
                }
                Err(error) => {
                    view["effects"] = json!([{
                        "kind":"toast","style":"failure",
                        "title":"Failed to read clipboard content","message":error
                    }])
                }
            }
            Ok(view)
        }
        "extension:invoke-action" => {
            active(state)?;
            let id = payload["actionId"].as_str().ok_or("Missing actionId")?;
            let copy = id == action_id(state, "copy");
            if !copy && id != action_id(state, "paste") {
                return Err("Action is no longer available in this session.".into());
            }
            let edited = payload["formValues"]["edited"]
                .as_str()
                .ok_or("formValues.edited must be a string")?;
            // Never replace a submitted draft with the launch snapshot, even on
            // failure. Returning Err leaves the renderer's current form intact.
            state["draft"] = json!(edited);
            let route = if copy {
                "clipboard:write"
            } else {
                "core:platform:paste-text"
            };
            request(host, route, vec![json!(edited)]).await?;
            request(host, "window:hide", vec![]).await?;
            // Beta has no popToRoot route. Its no-view result keeps the existing
            // surface, so keep this session and its action IDs usable on restore.
            let mut result = json!({"ok":true,"mode":"no-view"});
            if copy {
                result["effects"] = json!([{"kind":"hud","title":"Copied to clipboard"}]);
            }
            Ok(result)
        }
        "extension:refresh-session" => {
            active(state)?;
            Ok(json!({"ok":true,"mode":"unchanged"}))
        }
        "extension:dispose-session" => {
            let revision = state["revision"].as_u64().unwrap_or(0);
            *state = json!({"active":false,"revision":revision});
            Ok(json!({"ok":true,"mode":"unchanged"}))
        }
        _ => Err(format!(
            "Unsupported clipboard editor operation: {operation}"
        )),
    }
}

fn active(state: &Value) -> Result<(), String> {
    if state["active"] == true && state["nonce"].is_u64() && state["revision"].is_u64() {
        Ok(())
    } else {
        Err("Clipboard editor session is no longer active.".into())
    }
}

fn action_id(state: &Value, verb: &str) -> String {
    format!(
        "clipboard-editor:{}:{}:{verb}",
        state["nonce"], state["revision"]
    )
}

fn form(state: &Value) -> Value {
    json!({"ok":true,"mode":"view","title":"Edit Clipboard Content",
    "root":{"type":"Form","props":{"isLoading":false},"children":[
        {"type":"Form.Description","props":{"text":"Edit your clipboard text in the text area below."}},
        {"type":"Form.TextArea","props":{"id":"edited","value":state["draft"]}}
    ]},
    "actions":[
        {"id":action_id(state,"copy"),"title":"Copy to Clipboard","kind":"submit-form","style":"default","icon":"Clipboard"},
        {"id":action_id(state,"paste"),"title":"Paste to Active App","kind":"submit-form","style":"default","icon":"Clipboard"}
    ]})
}

async fn request(host: &impl Host, route: &str, args: Vec<Value>) -> Result<Value, String> {
    let value = host
        .request(route, args)
        .await
        .map_err(|e| format!("{route}: {e}"))?;
    if value["ok"] == false {
        let error = value["error"]
            .as_str()
            .or_else(|| value["message"].as_str())
            .unwrap_or("Host request failed");
        return Err(format!("{route}: {error}"));
    }
    Ok(value)
}
