# Pinned beta host fixtures

Read-only snapshots from Tezbar beta commit
`fd300e276356587726566026f072563eb9c34cb1`, repository
https://github.com/almatkai/Tezbar . Captured byte-for-byte with `git show`:

| Local file | Original path | SHA-256 |
| --- | --- | --- |
| beta_common.rs | src-tauri/src/native_core/common.rs | 280a0a2f0dd04a8570b320aafc0c3ece0f5de8ddb21c363645567d6a35d84da7 |
| extension-v2.schema.json | schemas/extension-v2.schema.json | 6d5afda1b652605982104bdf16d1547547b98dfd707a7b2969316406c49f32af |
| TEZBAR-LICENSE.txt | LICENSE | 0ed6b1b2760294d4aaaa45be38e166fcc0168e1dd5190423c096fcbe1c94afbf |

The host snapshots are Apache-2.0, retained in TEZBAR-LICENSE.txt. The adapter
contract compiles the production bridge unchanged with real common.rs. Only
Tauri AppHandle/Emitter and the OS router are recording stubs; no profile,
clipboard, window or installed extension is accessed. Upstream common.rs's own
tests use temporary directories. The V2 library and original icon are MIT;
package SPDX is MIT AND Apache-2.0 because it includes the test snapshots.
