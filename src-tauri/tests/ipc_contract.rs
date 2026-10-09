//! Replays the exact payloads ui/src/api.ts sends through the real command
//! handlers, so JS/Rust argument names and shapes can't drift apart.

use std::sync::Arc;

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{Manager, WebviewWindow};
use wh_vault::crypto::KdfParams;

const FAST: KdfParams = KdfParams { mem_kib: 256, iterations: 1, parallelism: 1 };

struct Ipc {
    window: WebviewWindow<MockRuntime>,
    _dir: tempfile::TempDir,
}

impl Ipc {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let app = wormhole_lib::with_commands(mock_builder()).build(mock_context(noop_assets())).unwrap();
        app.manage(wh_app::App::new(dir.path().to_path_buf(), FAST, Arc::new(|_| {})).unwrap());
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
        Self { window, _dir: dir }
    }

    fn call(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        self.call_from("tauri://localhost", cmd, args)
    }

    fn call_from(&self, origin: &str, cmd: &str, args: Value) -> Result<Value, Value> {
        let req = InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: origin.parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        };
        get_ipc_response(&self.window, req).map(|b| b.deserialize::<Value>().unwrap())
    }

    fn ok(&self, cmd: &str, args: Value) -> Value {
        self.call(cmd, args).unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
    }
}

fn settings() -> Value {
    json!({ "port": null, "username": null, "identity": null, "jump_chain": null, "env": {},
            "startup_snippet": null, "agent_forwarding": null })
}

#[test]
fn vault_and_inventory_payloads_match_api_ts() {
    let ipc = Ipc::new();
    assert_eq!(ipc.ok("vault_status", json!({})), json!({ "initialized": false, "unlocked": false }));
    ipc.ok("vault_create", json!({ "password": "pw" }));

    let group = json!({ "id": "11111111-1111-4111-8111-111111111111", "name": "prod", "parent": null, "settings": settings() });
    ipc.ok("save_group", json!({ "group": group }));
    let host = json!({ "id": "22222222-2222-4222-8222-222222222222", "label": "web", "address": "10.0.0.1",
                       "group": "11111111-1111-4111-8111-111111111111", "tags": ["a"], "settings": settings() });
    ipc.ok("save_host", json!({ "host": host }));
    ipc.ok("set_password", json!({ "owner": "22222222-2222-4222-8222-222222222222", "password": "x" }));
    let identity = json!({ "id": "33333333-3333-4333-8333-333333333333", "label": "ops", "username": "ops", "key": null });
    ipc.ok("save_identity", json!({ "identity": identity }));
    ipc.ok("save_snippet", json!({ "snippet": { "id": "44444444-4444-4444-8444-444444444444", "label": "up", "script": "uptime {{x}}" } }));
    assert_eq!(ipc.ok("snippet_variables", json!({ "script": "a {{x}}" })), json!(["x"]));
    let fwd = json!({ "id": "55555555-5555-4555-8555-555555555555", "label": "db", "host": "22222222-2222-4222-8222-222222222222",
                      "kind": "dynamic", "bind_address": "127.0.0.1", "bind_port": 1080, "dest_host": "", "dest_port": 0 });
    ipc.ok("save_forward", json!({ "forward": fwd }));
    ipc.ok("key_generate", json!({ "label": "k", "kind": { "type": "ed25519" } }));

    let inv = ipc.ok("inventory", json!({}));
    assert_eq!(inv["hosts"][0]["label"], "web");
    assert_eq!(inv["has_password"], json!(["22222222-2222-4222-8222-222222222222"]));
    assert_eq!(inv["forwards"][0]["kind"], "dynamic");
    assert_eq!(inv["keys"].as_array().unwrap().len(), 1);
    assert_eq!(ipc.ok("resolve_host", json!({ "id": "22222222-2222-4222-8222-222222222222" }))["port"], 22);

    // camelCase arguments from JS.
    let err = ipc.call("delete_group", json!({ "id": "11111111-1111-4111-8111-111111111111", "moveContents": false })).unwrap_err();
    assert_eq!(err["kind"], "group_not_empty");
    ipc.ok("delete_group", json!({ "id": "11111111-1111-4111-8111-111111111111", "moveContents": true }));
    let err = ipc.call("key_import", json!({ "label": "x", "privateKey": "junk", "passphrase": null })).unwrap_err();
    assert_eq!(err["kind"], "unsupported_key");

    ipc.ok("vault_change_password", json!({ "old": "pw", "new": "pw2" }));
    ipc.ok("save_settings", json!({ "settings": { "session_logs": true, "auto_lock_minutes": 3 } }));
    assert_eq!(ipc.ok("get_settings", json!({})), json!({ "session_logs": true, "auto_lock_minutes": 3 }));
    ipc.ok("vault_lock", json!({}));
    assert_eq!(ipc.call("vault_unlock", json!({ "password": "pw" })).unwrap_err()["kind"], "bad_password");
    ipc.ok("vault_unlock", json!({ "password": "pw2" }));
}

#[test]
fn session_and_file_payloads_match_api_ts() {
    let ipc = Ipc::new();
    ipc.ok("vault_create", json!({ "password": "pw" }));
    let missing = "99999999-9999-4999-8999-999999999999";
    assert_eq!(ipc.call("session_write", json!({ "id": missing, "data": "ls\r" })).unwrap_err()["kind"], "not_found");
    assert_eq!(ipc.call("session_resize", json!({ "id": missing, "cols": 80, "rows": 24 })).unwrap_err()["kind"], "not_found");
    assert_eq!(ipc.ok("session_broadcast", json!({ "ids": [missing], "data": "x" })), json!(0));
    assert_eq!(ipc.call("host_key_respond", json!({ "prompt": missing, "accept": true })).unwrap_err()["kind"], "not_found");
    assert_eq!(ipc.ok("forwards_active", json!({})), json!([]));
    assert_eq!(ipc.call("forward_stop", json!({ "rule": missing })).unwrap_err()["kind"], "not_found");
    assert_eq!(ipc.call("sftp_home", json!({ "sftp": missing })).unwrap_err()["kind"], "not_found");
    let listing = ipc.ok("local_list", json!({ "path": env!("CARGO_MANIFEST_DIR") }));
    assert!(listing.as_array().unwrap().iter().any(|e| e["name"] == "Cargo.toml" && e["is_dir"] == false));
    let vars = json!({ "x": "1" });
    assert_eq!(ipc.call("run_snippet", json!({ "script": "{{y}}", "vars": vars, "hosts": [] })).unwrap_err()["kind"], "validation");
    assert_eq!(ipc.ok("known_hosts", json!({})), json!([]));
}

// Sync commands run on the UI thread, which has no Tokio runtime. This used to
// panic there and abort the whole app.
#[test]
fn local_terminal_opens_through_real_handler() {
    let ipc = Ipc::new();
    ipc.ok("vault_create", json!({ "password": "pw" }));
    let id = ipc.ok("open_local_session", json!({ "cols": 80, "rows": 24 }));
    ipc.ok("session_write", json!({ "id": id, "data": "exit\r" }));
    ipc.ok("session_close", json!({ "id": id }));
}

#[test]
fn remote_pages_cannot_call_app_commands() {
    let ipc = Ipc::new();
    let err = ipc.call_from("https://evil.example", "vault_status", json!({})).unwrap_err();
    assert!(err.as_str().unwrap_or_default().contains("not allowed"), "{err}");
    assert!(ipc.call("vault_status", json!({})).is_ok());
}
