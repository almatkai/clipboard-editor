use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::Mutex;
use tezbar_clipboard_editor_v2::{handle, Host, HostFuture};

const COMMAND: &str = "edit-clipboard-text";
#[derive(Default)]
struct RecordingHost {
    calls: Mutex<Vec<(String, Vec<Value>)>>,
    replies: Mutex<VecDeque<Result<Value, String>>>,
}
impl RecordingHost {
    fn replies(&self, replies: impl IntoIterator<Item = Result<Value, String>>) {
        self.replies.lock().unwrap().extend(replies);
    }
    fn calls(&self) -> Vec<(String, Vec<Value>)> {
        self.calls.lock().unwrap().clone()
    }
}
impl Host for RecordingHost {
    fn request<'a>(&'a self, route: &'a str, args: Vec<Value>) -> HostFuture<'a> {
        Box::pin(async move {
            self.calls.lock().unwrap().push((route.into(), args));
            self.replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unplanned OS request")
        })
    }
}
async fn run(
    host: &RecordingHost,
    op: &str,
    payload: Value,
    state: &mut Value,
) -> Result<Value, String> {
    handle(host, COMMAND, op, &payload, state).await
}
fn submit(view: &Value, index: usize, edited: Value) -> Value {
    json!({"actionId":view["actions"][index]["id"],"formValues":{"edited":edited}})
}

#[tokio::test]
async fn exact_form_and_current_submissions_in_source_order() {
    for text in ["", " \t\r\n", "Қазақ 日本 😀\nsecond\r\nline\t "] {
        let host = RecordingHost::default();
        host.replies([Ok(json!(text))]);
        let mut state = json!({});
        let view = run(&host, "extension:run-command", json!({}), &mut state)
            .await
            .unwrap();
        assert_eq!(
            view["root"],
            json!({"type":"Form","props":{"isLoading":false},"children":[
                {"type":"Form.Description","props":{"text":"Edit your clipboard text in the text area below."}},
                {"type":"Form.TextArea","props":{"id":"edited","value":text}}
            ]})
        );
        assert_eq!(view["title"], "Edit Clipboard Content");
        assert_eq!(view["actions"].as_array().unwrap().len(), 2);
        for (index, (title, route)) in [
            ("Copy to Clipboard", "clipboard:write"),
            ("Paste to Active App", "core:platform:paste-text"),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(view["actions"][index]["title"], title);
            assert_eq!(view["actions"][index]["kind"], "submit-form");
            // Use a different current value; empty submission is valid too.
            let edited = if index == 0 {
                format!("{text}\nnew draft 😀  ")
            } else {
                String::new()
            };
            host.replies([Ok(Value::Null), Ok(json!({"ok":true}))]);
            let result = run(
                &host,
                "extension:invoke-action",
                submit(&view, index, json!(edited)),
                &mut state,
            )
            .await
            .unwrap();
            assert_eq!(result["mode"], "no-view");
            if index == 0 {
                assert_eq!(
                    result["effects"],
                    json!([{"kind":"hud","title":"Copied to clipboard"}])
                );
            } else {
                assert!(result.get("effects").is_none());
            }
            assert_eq!(state["draft"], edited);
            assert_eq!(
                &host.calls()[host.calls().len() - 2..],
                &[
                    (route.into(), vec![json!(edited)]),
                    ("window:hide".into(), vec![])
                ]
            );
        }
        assert_eq!(host.calls()[0], ("clipboard:read".into(), vec![]));
        assert_eq!(host.calls().len(), 5);
    }
}

#[tokio::test]
async fn refresh_leaves_renderer_draft_and_state_untouched_without_io() {
    let host = RecordingHost::default();
    host.replies([Ok(json!("initial"))]);
    let mut state = json!({});
    let view = run(&host, "extension:run-command", json!({}), &mut state)
        .await
        .unwrap();
    let snapshot = state.clone();
    for _ in 0..3 {
        let refreshed = run(&host, "extension:refresh-session", json!({}), &mut state)
            .await
            .unwrap();
        assert_eq!(refreshed, json!({"ok":true,"mode":"unchanged"}));
        assert_eq!(state, snapshot);
    }
    host.replies([Ok(Value::Null), Ok(Value::Null)]);
    run(
        &host,
        "extension:invoke-action",
        submit(&view, 0, json!("unsent local draft")),
        &mut state,
    )
    .await
    .unwrap();
    assert_eq!(state["draft"], "unsent local draft");
    assert_eq!(host.calls().len(), 3);
}

#[tokio::test]
async fn failed_read_yields_editable_empty_form_and_one_failure_toast_then_retry() {
    for failure in [
        Err("denied".into()),
        Ok(json!({"ok":false,"error":"denied"})),
        Ok(json!({"ok":false,"message":"denied"})),
        Ok(json!(42)),
    ] {
        let host = RecordingHost::default();
        host.replies([failure]);
        let mut state = json!({});
        let view = run(&host, "extension:run-command", json!({}), &mut state)
            .await
            .unwrap();
        assert_eq!(view["mode"], "view");
        assert_eq!(view["root"]["children"][1]["props"]["value"], "");
        assert_eq!(
            view["effects"][0]["title"],
            "Failed to read clipboard content"
        );
        assert_eq!(view["effects"][0]["style"], "failure");
        assert!(
            view["effects"][0]["message"]
                .as_str()
                .unwrap()
                .contains("clipboard")
                || view["effects"][0]["message"]
                    .as_str()
                    .unwrap()
                    .contains("Clipboard")
        );
        for _ in 0..2 {
            let result = run(&host, "extension:refresh-session", json!({}), &mut state)
                .await
                .unwrap();
            assert_eq!(result, json!({"ok":true,"mode":"unchanged"}));
        }
        assert_eq!(host.calls().len(), 1);
        host.replies([Ok(Value::Null), Ok(Value::Null)]);
        run(
            &host,
            "extension:invoke-action",
            submit(&view, 0, json!("recovered")),
            &mut state,
        )
        .await
        .unwrap();
        assert_eq!(
            host.calls()[1],
            ("clipboard:write".into(), vec![json!("recovered")])
        );
    }
}

#[tokio::test]
async fn failures_propagate_without_success_clear_or_invalidation_and_retry_works() {
    for index in 0..2 {
        for hide in [false, true] {
            for failure in [
                Err("denied".into()),
                Ok(json!({"ok":false,"error":"denied"})),
                Ok(json!({"ok":false,"message":"denied"})),
                Ok(json!({"ok":false})),
            ] {
                let host = RecordingHost::default();
                host.replies([Ok(json!("launch"))]);
                let mut state = json!({});
                let view = run(&host, "extension:run-command", json!({}), &mut state)
                    .await
                    .unwrap();
                let payload = submit(&view, index, json!("  retry\n😀\t"));
                if hide {
                    host.replies([Ok(Value::Null)]);
                }
                host.replies([failure]);
                let error = run(
                    &host,
                    "extension:invoke-action",
                    payload.clone(),
                    &mut state,
                )
                .await
                .unwrap_err();
                assert!(error.contains(if hide {
                    "window:hide"
                } else if index == 0 {
                    "clipboard:write"
                } else {
                    "core:platform:paste-text"
                }));
                assert_eq!(state["draft"], "  retry\n😀\t");
                assert_eq!(state["active"], true);
                assert_eq!(host.calls().len(), if hide { 3 } else { 2 });
                let snapshot = state.clone();
                run(&host, "extension:refresh-session", json!({}), &mut state)
                    .await
                    .unwrap();
                assert_eq!(state, snapshot);
                host.replies([Ok(Value::Null), Ok(Value::Null)]);
                let success = run(&host, "extension:invoke-action", payload, &mut state)
                    .await
                    .unwrap();
                assert_eq!(success["ok"], true);
            }
        }
    }
}

#[tokio::test]
async fn reject_unknown_missing_nonstring_and_cross_session_stale_disposed_actions() {
    let host = RecordingHost::default();
    host.replies([Ok(json!("one")), Ok(json!("two")), Ok(json!("three"))]);
    let mut state = json!({});
    let first = run(&host, "extension:run-command", json!({}), &mut state)
        .await
        .unwrap();
    let mut other = json!({});
    let second = run(&host, "extension:run-command", json!({}), &mut other)
        .await
        .unwrap();
    let calls = host.calls().len();
    for payload in [
        json!({}),
        json!({"actionId":"forged"}),
        submit(&second, 0, json!("wrong session")),
        json!({"actionId":first["actions"][0]["id"]}),
        submit(&first, 0, Value::Null),
        submit(&first, 0, json!(7)),
        submit(&first, 0, json!({})),
        submit(&first, 0, json!(false)),
    ] {
        assert!(run(&host, "extension:invoke-action", payload, &mut state)
            .await
            .is_err());
    }
    for operation in [
        "run-command",
        "extension:search-text-changed",
        "extension:load-more",
        "unknown",
    ] {
        assert!(run(&host, operation, json!({}), &mut state).await.is_err());
    }
    assert!(handle(
        &host,
        "unknown",
        "extension:run-command",
        &json!({}),
        &mut state
    )
    .await
    .is_err());
    assert_eq!(host.calls().len(), calls);
    let revision = state["revision"].as_u64().unwrap();
    run(&host, "extension:dispose-session", json!({}), &mut state)
        .await
        .unwrap();
    assert_eq!(state, json!({"active":false,"revision":revision}));
    assert!(run(
        &host,
        "extension:invoke-action",
        submit(&first, 0, json!("disposed")),
        &mut state
    )
    .await
    .is_err());
    assert!(
        run(&host, "extension:refresh-session", json!({}), &mut state)
            .await
            .is_err()
    );
    let third = run(&host, "extension:run-command", json!({}), &mut state)
        .await
        .unwrap();
    assert!(state["revision"].as_u64().unwrap() > revision);
    assert_ne!(first["actions"][0]["id"], third["actions"][0]["id"]);
    assert!(run(
        &host,
        "extension:invoke-action",
        submit(&first, 0, json!("replayed")),
        &mut state
    )
    .await
    .is_err());
    // Relaunch an active state as well.
    host.replies([Ok(json!("four"))]);
    run(&host, "extension:run-command", json!({}), &mut state)
        .await
        .unwrap();
    assert!(run(
        &host,
        "extension:invoke-action",
        submit(&third, 1, json!("stale")),
        &mut state
    )
    .await
    .is_err());
    assert_eq!(host.calls().len(), 4);
}
