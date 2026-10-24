# Clipboard Editor — Native V2 Rust library

A compiled-in Tezbar Native V2 library, with the original icon and source form
behavior. Original extension: **pomdtr** (copyright 2021 Raycast, MIT); Tezbar
adaptation: **almatkai**. V1 source, metadata, assets, locks and license remain
unchanged. The repository README links here.

Reference host: Tezbar beta **fd300e276356587726566026f072563eb9c34cb1**.
The integration follows its existing NATIVE_V2_FORMAT.md, extension-v2 schema,
v2_sdk.rs, native.rs, v2_runtime.rs, v2_amphetamine.rs, v2_timers.rs, common.rs,
platform routes and ExtensionRuntimeView/form renderer. See the pinned, hashed
fixtures in `tests/fixtures/PROVENANCE.md` and icon in `assets/PROVENANCE.md`.

## Build and verify

From `v2/`:

```sh
cargo build --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo +1.85.0 test --locked
cargo +1.85.0 clippy --all-targets --locked -- -D warnings
cargo check --all-targets --locked --target x86_64-pc-windows-gnu
cargo clippy --all-targets --locked --target x86_64-pc-windows-gnu -- -D warnings
cargo package --allow-dirty --locked
```

Rust 1.85 minimum; Cargo.lock is included and resolved for that version.
`serde_json` is the only runtime dependency. JSON Schema, SHA-256, Tokio, log and
tempfile support tests only. This is a standalone library crate with no binary,
stdin protocol, WASM or custom loader. Rust tests require no Node or Bun and are
self-contained in the Cargo package. Extract the `.crate` archive and run
`cargo test --locked` in it to verify packaging. Windows cross checks require
that Rust target installed; they do not run Windows OS integration.

Tests cover the exact form schema and two submit actions in source order,
one clipboard read per launch, empty/Unicode/whitespace/multiline preservation,
current submitted values, refresh without I/O, one read-failure toast and an
editable empty form, retries after read/write/paste/hide failures (exceptions
and error envelopes), forged/cross-session/stale/replayed action rejection,
revision tombstones, schema/metadata/icon hashes, and the unchanged concrete
bridge compiled with beta's real CoreContext. Only Tauri AppHandle/Emitter and
the OS router are stubbed. No real clipboard, window or installed profile is
used. The included common.rs also runs its own temporary-file tests.

V1 checks can run in a disposable `v2/target/v1-check` copy of the V1 files:
`pnpm import` (from its copied package-lock.json), `pnpm install --frozen-lockfile
--ignore-scripts`, `pnpm test`, `pnpm typecheck`. Do not replace the root npm lock.

## Interface and lifecycle

```rust,ignore
pub fn commands() -> &'static [&'static str]; // edit-clipboard-text
pub const ICON: &[u8];
pub const MANIFEST: &str;
pub async fn handle(
    host: &impl Host,
    command: &str,
    operation: &str,
    payload: &serde_json::Value,
    state: &mut serde_json::Value,
) -> Result<serde_json::Value, String>;
```

`Host::request(route, args)` returns a Send boxed async future. `tezbar_adapter.rs`
wraps the actual CoreContext and provides beta's concrete signature. `commands()`
reports intended macOS and Windows compiled coverage, empty on other platforms.
The host owns installed-command checks, session IDs, serialized state access and
cancellation; retain these checks. Each launch also creates unique action IDs
bound to that adapter state and a monotonically increasing revision. Dispose
removes text and invalidates actions but retains the revision tombstone.

Only these operations are accepted:

- `extension:run-command`: read clipboard text once, initialize the form. Read
  errors return the editable empty form and the original failure toast once.
- `extension:invoke-action`: require a current action ID and a string at
  `payload.formValues.edited`. Missing/nonstring values fail; empty strings are
  valid. Never trim, transform or substitute the launch snapshot. Store the
  submitted draft before any side effect so failures retain it for retry.
- `extension:refresh-session`: return `unchanged`, preserving the renderer's
  unsent draft and action IDs. No clipboard read/write, paste or hide occurs.
- `extension:dispose-session`: invalidate actions and clear text without I/O.

The form has the exact description “Edit your clipboard text in the text area
below.” and `Form.TextArea` id `edited`. Beta initializes fields from `props.value`,
so this supplies V1's initial clipboard/default value using that existing schema.
Both actions use `kind: "submit-form"`, in this order:

1. **Copy to Clipboard**: await `clipboard:write` with `[edited]`, then
   `window:hide` with `[]`, then return HUD effect
   `{"kind":"hud","title":"Copied to clipboard"}`.
2. **Paste to Active App**: await `core:platform:paste-text` with `[edited]`,
   then `window:hide` with `[]`. No copy HUD or extra clipboard write.

Beta's paste-text route already writes, hides, waits for focus, and sends
Cmd/Ctrl+V. The final hide corresponds to the source's closeMainWindow call.
All requests are awaited; `Err` and `{ok:false,error/message}` propagate with
route context. A failed operation yields no success response/effect and leaves
the draft/action IDs usable. A hide failure after a successful write/paste cannot
roll back that OS side effect; retry repeats the requested operation.

**Navigation adaptation:** beta has no native popToRoot SDK route. Successful
submits use the existing window-hide/no-view behavior instead. Beta's renderer
keeps the previous surface on no-view, so the adapter deliberately keeps the
session active and does not clear the draft. Restoring the hidden window can
show the existing form; refresh cannot reset it or repeat the read-failure toast.
No invented navigation route or premature dispose is used.

## Integrate with the host

A Tezbar host change and rebuild are required. This crate cannot be installed
directly as Rust from the current extension store. A JSON manifest/marker alone
does not register executable code. These steps document later host work; this
repository does not apply it to an app checkout or installed profile.

1. Vendor the contents of `v2/` at
   `src-tauri/native-extensions/clipboard-editor/`. Preserve LICENSE, lockfile and
   fixture attribution. Add to `src-tauri/Cargo.toml`:

   ```toml
   tezbar-clipboard-editor-v2 = { path = "native-extensions/clipboard-editor" }
   ```

   Update/review the host's Cargo.lock. Copy `tezbar_adapter.rs` unchanged to
   `src-tauri/src/native_core/extensions/v2_clipboard_editor.rs`.

2. Add `mod v2_clipboard_editor;` to
   `src-tauri/src/native_core/extensions/mod.rs`. In `v2_runtime.rs`, add
   `"clipboard-editor"` to the `slug()` allowlist and the following arms to
   `commands()` and `handle()` respectively:

   ```rust,ignore
   "clipboard-editor" => super::v2_clipboard_editor::commands(),
   "clipboard-editor" => super::v2_clipboard_editor::handle(ctx, command, operation, payload, state).await,
   ```

3. Append this crate's `manifest.json` object to the host `v2_catalog.json`.
   Canonical ID is `com.tezbar.native.clipboard-editor`, replacing
   `raycast.clipboard-editor`. Keep the command name `edit-clipboard-text`, its
   platforms, and absent preferences. Validate against beta's real schema; it
   requires the icon URL and does not allow an author field, so credits are in
   description. Update catalog-count tests and add compiled-dispatch coverage.

4. Copy **`assets/icon.png` byte-for-byte** to
   **`src/renderer/public/extensions/icons/clipboard-editor.png`**, the host's
   Vite publicDir. Manifest URL stays `/extensions/icons/clipboard-editor.png`.
   `ICON` embeds those same original 190932 bytes for offline integration. If
   managed-package fallback needs the icon too, write `v2_clipboard_editor::ICON`
   under that canonical package's `assets/icon.png` during catalog installation,
   before recording success; resolve fallback there explicitly. Beta's existing
   catalog installer writes JSON only. Do not overwrite V1 assets or alter the
   manifest's schema-required absolute URL.

5. Rebuild/test Tezbar, check its actual compiled V2 CLI coverage, then install
   the canonical ID via the normal Native V2 catalog route. Retain V1 for
   fallback. No JavaScript fallback or external loader is part of this library.

6. Perform real macOS and Windows UI QA: read denial, typing, empty text, Unicode
   and multiline text, both submit actions, clipboard permission failures,
   focus/paste behavior, hide/restore draft preservation, retries and relaunch.
   Record QA artifacts and commit before changing platform verification.

**Remaining verification:** no full host rebuild, macOS UI/clipboard/focus QA or
Windows execution has run. Both manifest verification entries remain `not-run`.
The host fixture is Apache-2.0 (`tests/fixtures/TEZBAR-LICENSE.txt`); library and
original icon are MIT. The packaged combination uses `MIT AND Apache-2.0`.
