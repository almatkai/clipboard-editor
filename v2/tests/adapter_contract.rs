//! Compile the unchanged bridge with the real pinned beta CoreContext.
extern crate self as tauri;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tezbar_clipboard_editor_v2::MANIFEST;

#[derive(Clone)]
pub struct AppHandle;
pub trait Emitter {
    fn emit(&self, channel: &str, payload: Value) -> Result<(), String>;
}
impl Emitter for AppHandle {
    fn emit(&self, _: &str, _: Value) -> Result<(), String> {
        panic!("No real Tauri app in contract tests")
    }
}
mod native_core {
    #[allow(dead_code, clippy::all)]
    pub mod common {
        include!("fixtures/beta_common.rs");
    }
}
#[path = "../tezbar_adapter.rs"]
mod adapter;

#[test]
fn pinned_schema_manifest_icon_license_and_provenance() {
    let schema: Value =
        serde_json::from_str(include_str!("fixtures/extension-v2.schema.json")).unwrap();
    let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
    jsonschema::validator_for(&schema)
        .unwrap()
        .validate(&manifest)
        .unwrap();
    assert_eq!(manifest["id"], "com.tezbar.native.clipboard-editor");
    assert_eq!(manifest["replaces"], json!(["raycast.clipboard-editor"]));
    assert_eq!(manifest["icon"], "/extensions/icons/clipboard-editor.png");
    assert_eq!(manifest["platforms"], json!(["macOS", "Windows"]));
    assert_eq!(
        manifest["verification"],
        json!([{"platform":"macOS","state":"not-run"},{"platform":"Windows","state":"not-run"}])
    );
    assert!(manifest.get("preferences").is_none());
    assert!(manifest.get("author").is_none());
    for credit in ["pomdtr", "almatkai"] {
        assert!(manifest["description"].as_str().unwrap().contains(credit));
    }
    assert_eq!(adapter::commands(), &["edit-clipboard-text"]);
    assert_eq!(adapter::ICON.len(), 190932);
    for (bytes, hash) in [
        (
            adapter::ICON,
            "9b5ec7b59eaea92fa3c6ec75b4016d464635adc0e57d435df031f1424ac824c3",
        ),
        (
            include_bytes!("fixtures/beta_common.rs").as_slice(),
            "280a0a2f0dd04a8570b320aafc0c3ece0f5de8ddb21c363645567d6a35d84da7",
        ),
        (
            include_bytes!("fixtures/extension-v2.schema.json").as_slice(),
            "6d5afda1b652605982104bdf16d1547547b98dfd707a7b2969316406c49f32af",
        ),
        (
            include_bytes!("fixtures/TEZBAR-LICENSE.txt").as_slice(),
            "0ed6b1b2760294d4aaaa45be38e166fcc0168e1dd5190423c096fcbe1c94afbf",
        ),
    ] {
        assert_eq!(format!("{:x}", Sha256::digest(bytes)), hash);
    }
    let provenance = include_str!("../assets/PROVENANCE.md");
    assert!(provenance.contains("6734d85992290066fd31a7f5bee3cca3d3545a1a"));
    assert!(provenance.contains("extensions/clipboard-editor/assets/icon.png"));
    assert!(provenance.contains("9b5ec7b59eaea92fa3c6ec75b4016d464635adc0e57d435df031f1424ac824c3"));
    assert!(include_str!("../LICENSE")
        .contains("Copyright (c) 2021 Raycast\nCopyright (c) 2026 almatkai\n"));
    // Optional surrounding V1 parity; extracted Cargo package stays self-contained.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    if root.join("package.json").is_file() {
        let original: Value =
            serde_json::from_slice(&std::fs::read(root.join("package.json")).unwrap()).unwrap();
        assert_eq!(original["platforms"], manifest["platforms"]);
        assert_eq!(original["preferences"], manifest["preferences"]);
        for key in ["name", "title", "description", "mode", "platforms"] {
            assert_eq!(original["commands"][0][key], manifest["commands"][0][key]);
        }
        assert_eq!(
            std::fs::read(root.join("assets/icon.png")).unwrap(),
            adapter::ICON
        );
    }
}

#[tokio::test]
async fn real_corecontext_routes_exact_positional_arguments_and_errors() {
    use native_core::common::{CoreContext, CorePaths};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/adapter-fixture");
    let ctx = CoreContext::headless(CorePaths {
        data_dir: root.join("data"),
        config_dir: root.join("config"),
        resource_dir: root.join("resources"),
        temp_dir: root.join("tmp"),
        home_dir: root,
    });
    let calls = Arc::new(Mutex::new(Vec::new()));
    let failure = Arc::new(Mutex::new(None::<(String, bool)>));
    let recorded = calls.clone();
    let failed = failure.clone();
    ctx.set_router(Arc::new(move |route, args| {
        let recorded = recorded.clone();
        let failed = failed.clone();
        Box::pin(async move {
            recorded.lock().unwrap().push((route.clone(), args.clone()));
            if let Some((fail_route, envelope)) = &*failed.lock().unwrap() {
                if &route == fail_route {
                    return if *envelope {
                        Ok(json!({"ok":false,"message":"recorded denial"}))
                    } else {
                        Err("recorded denial".into())
                    };
                }
            }
            match route.as_str() {
                "clipboard:read" => {
                    assert!(args.is_empty());
                    Ok(json!(" \nҚазақ 😀\t"))
                }
                "clipboard:write" | "core:platform:paste-text" => {
                    assert_eq!(args.len(), 1);
                    assert!(args[0].is_string());
                    Ok(Value::Null)
                }
                "window:hide" => {
                    assert!(args.is_empty());
                    Ok(json!({"ok":true}))
                }
                _ => panic!("Invented route: {route}"),
            }
        })
    }))
    .unwrap();
    let mut state = json!({});
    let view = adapter::handle(
        &ctx,
        "edit-clipboard-text",
        "extension:run-command",
        &json!({}),
        &mut state,
    )
    .await
    .unwrap();
    assert_eq!(
        view["root"]["children"][1]["props"]["value"],
        " \nҚазақ 😀\t"
    );
    for (index, route) in [(0, "clipboard:write"), (1, "core:platform:paste-text")] {
        for envelope in [false, true] {
            for fail_route in [route, "window:hide"] {
                *failure.lock().unwrap() = Some((fail_route.into(), envelope));
                let payload = json!({"actionId":view["actions"][index]["id"],"formValues":{"edited":"current\n😀  "}});
                let error = adapter::handle(
                    &ctx,
                    "edit-clipboard-text",
                    "extension:invoke-action",
                    &payload,
                    &mut state,
                )
                .await
                .unwrap_err();
                assert!(error.contains("recorded denial"));
                assert_eq!(state["draft"], "current\n😀  ");
                *failure.lock().unwrap() = None;
                let start = calls.lock().unwrap().len();
                adapter::handle(
                    &ctx,
                    "edit-clipboard-text",
                    "extension:invoke-action",
                    &payload,
                    &mut state,
                )
                .await
                .unwrap();
                assert_eq!(
                    &calls.lock().unwrap()[start..],
                    &[
                        (route.into(), vec![json!("current\n😀  ")]),
                        ("window:hide".into(), vec![])
                    ]
                );
            }
        }
    }
    let count = calls.lock().unwrap().len();
    adapter::handle(
        &ctx,
        "edit-clipboard-text",
        "extension:refresh-session",
        &json!({}),
        &mut state,
    )
    .await
    .unwrap();
    adapter::handle(
        &ctx,
        "edit-clipboard-text",
        "extension:dispose-session",
        &json!({}),
        &mut state,
    )
    .await
    .unwrap();
    assert_eq!(calls.lock().unwrap().len(), count);
    assert!(!ctx.paths.config_dir.exists());
}
