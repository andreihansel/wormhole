//! Tauri glue: every command forwards to `wh_app::App`. No logic lives here.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use tauri::{Emitter, Manager, State};
use uuid::Uuid;
use wh_app::admin::{ImportSummary, Settings};
use wh_app::inventory::{InventoryDto, NewKey};
use wh_app::sftp::LocalEntry;
use wh_app::state::VaultStatus;
use wh_app::tools::{ActiveForward, SnippetRun};
use wh_app::{ApiError, App};
use wh_core::inventory::Group;
use wh_core::model::{Host, Id, Identity, KnownHost, PortForward, Snippet, SshKey};
use wh_core::resolve::Resolved;
use wh_ssh::sftp::RemoteEntry;

type R<T> = Result<T, ApiError>;

// --- vault ---------------------------------------------------------------

#[tauri::command]
fn vault_status(app: State<'_, App>) -> VaultStatus {
    app.vault_status()
}
#[tauri::command]
fn vault_create(app: State<'_, App>, password: String) -> R<()> {
    app.vault_create(&password)
}
#[tauri::command]
fn vault_unlock(app: State<'_, App>, password: String) -> R<()> {
    app.vault_unlock(&password)
}
#[tauri::command]
fn vault_lock(app: State<'_, App>) {
    app.vault_lock()
}
#[tauri::command]
fn vault_change_password(app: State<'_, App>, old: String, new: String) -> R<()> {
    app.vault_change_password(&old, &new)
}
#[tauri::command]
fn vault_export(app: State<'_, App>, path: String, password: String) -> R<()> {
    app.vault_export(&path, &password)
}
#[tauri::command]
fn vault_import(app: State<'_, App>, path: String, password: String) -> R<usize> {
    app.vault_import(&path, &password)
}

// --- inventory -----------------------------------------------------------

#[tauri::command]
fn inventory(app: State<'_, App>) -> R<InventoryDto> {
    app.inventory()
}
#[tauri::command]
fn save_host(app: State<'_, App>, host: Host) -> R<Host> {
    app.save_host(host)
}
#[tauri::command]
fn delete_host(app: State<'_, App>, id: Id) -> R<()> {
    app.delete_host(id)
}
#[tauri::command]
fn save_group(app: State<'_, App>, group: Group) -> R<Group> {
    app.save_group(group)
}
#[tauri::command]
fn delete_group(app: State<'_, App>, id: Id, move_contents: bool) -> R<()> {
    app.delete_group(id, move_contents)
}
#[tauri::command]
fn save_identity(app: State<'_, App>, identity: Identity) -> R<Identity> {
    app.save_identity(identity)
}
#[tauri::command]
fn delete_identity(app: State<'_, App>, id: Id) -> R<()> {
    app.delete_identity(id)
}
#[tauri::command]
fn set_password(app: State<'_, App>, owner: Id, password: Option<String>) -> R<()> {
    app.set_password(owner, password.as_deref())
}
#[tauri::command]
fn resolve_host(app: State<'_, App>, id: Id) -> R<Resolved> {
    app.resolve_host(id)
}
#[tauri::command]
fn key_generate(app: State<'_, App>, label: String, kind: NewKey) -> R<SshKey> {
    app.key_generate(&label, kind)
}
#[tauri::command]
fn key_import(app: State<'_, App>, label: String, private_key: String, passphrase: Option<String>) -> R<SshKey> {
    app.key_import(&label, &private_key, passphrase.as_deref())
}
#[tauri::command]
fn delete_key(app: State<'_, App>, id: Id) -> R<()> {
    app.delete_key(id)
}
#[tauri::command]
fn save_snippet(app: State<'_, App>, snippet: Snippet) -> R<Snippet> {
    app.save_snippet(snippet)
}
#[tauri::command]
fn delete_snippet(app: State<'_, App>, id: Id) -> R<()> {
    app.delete_snippet(id)
}
#[tauri::command]
fn snippet_variables(script: String) -> Vec<String> {
    wh_core::snippet::variables(&script)
}
#[tauri::command]
fn save_forward(app: State<'_, App>, forward: PortForward) -> R<PortForward> {
    app.save_forward(forward)
}
#[tauri::command]
fn delete_forward(app: State<'_, App>, id: Id) -> R<()> {
    app.delete_forward(id)
}
#[tauri::command]
fn known_hosts(app: State<'_, App>) -> R<Vec<KnownHost>> {
    app.known_hosts()
}
#[tauri::command]
fn delete_known_host(app: State<'_, App>, id: Id) -> R<()> {
    app.delete_known_host(id)
}

// --- import & settings ---------------------------------------------------

fn ssh_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".ssh")
}
#[tauri::command]
fn import_ssh_config(app: State<'_, App>, path: Option<String>) -> R<ImportSummary> {
    let dir = ssh_dir();
    let config = path.map(PathBuf::from).unwrap_or_else(|| dir.join("config"));
    app.import_ssh_config(&config, &dir)
}
#[tauri::command]
fn import_known_hosts(app: State<'_, App>, path: Option<String>) -> R<usize> {
    let path = path.map(PathBuf::from).unwrap_or_else(|| ssh_dir().join("known_hosts"));
    let text = std::fs::read_to_string(&path).map_err(|e| ApiError::new("not_found", e.to_string()))?;
    app.import_known_hosts(&text)
}
#[tauri::command]
fn get_settings(app: State<'_, App>) -> Settings {
    app.settings()
}
#[tauri::command]
fn save_settings(app: State<'_, App>, settings: Settings) -> R<()> {
    app.save_settings(settings)
}

// --- sessions ------------------------------------------------------------

#[tauri::command]
async fn open_ssh_session(app: State<'_, App>, host: Id, cols: u32, rows: u32) -> R<Uuid> {
    app.open_ssh_session(host, cols, rows).await
}
// async so it runs on Tauri's Tokio runtime: the session spawns Tokio tasks.
#[tauri::command]
async fn open_local_session(app: State<'_, App>, cols: u16, rows: u16) -> R<Uuid> {
    app.open_local_session(cols, rows)
}
#[tauri::command]
fn session_write(app: State<'_, App>, id: Uuid, data: String) -> R<()> {
    app.session_write(id, data.as_bytes())
}
#[tauri::command]
fn session_resize(app: State<'_, App>, id: Uuid, cols: u32, rows: u32) -> R<()> {
    app.session_resize(id, cols, rows)
}
#[tauri::command]
fn session_close(app: State<'_, App>, id: Uuid) -> R<()> {
    app.session_close(id)
}
#[tauri::command]
fn session_broadcast(app: State<'_, App>, ids: Vec<Uuid>, data: String) -> usize {
    app.session_broadcast(&ids, data.as_bytes())
}
#[tauri::command]
fn host_key_respond(app: State<'_, App>, prompt: Uuid, accept: bool) -> R<()> {
    app.host_key_respond(prompt, accept)
}

// --- forwards & snippets ---------------------------------------------------

#[tauri::command]
async fn forward_start(app: State<'_, App>, rule: Id) -> R<ActiveForward> {
    app.forward_start(rule).await
}
#[tauri::command]
async fn forward_stop(app: State<'_, App>, rule: Id) -> R<()> {
    app.forward_stop(rule).await
}
#[tauri::command]
async fn forwards_active(app: State<'_, App>) -> R<Vec<ActiveForward>> {
    Ok(app.forwards_active().await)
}
#[tauri::command]
async fn run_snippet(app: State<'_, App>, script: String, vars: BTreeMap<String, String>, hosts: Vec<Id>) -> R<Vec<SnippetRun>> {
    app.run_snippet(&script, &vars, &hosts).await
}

// --- sftp ------------------------------------------------------------------

#[tauri::command]
async fn sftp_open(app: State<'_, App>, host: Id) -> R<Uuid> {
    app.sftp_open(host).await
}
#[tauri::command]
async fn sftp_close(app: State<'_, App>, sftp: Uuid) -> R<()> {
    app.sftp_close(sftp).await
}
#[tauri::command]
async fn sftp_home(app: State<'_, App>, sftp: Uuid) -> R<String> {
    app.sftp_home(sftp).await
}
#[tauri::command]
async fn sftp_list(app: State<'_, App>, sftp: Uuid, path: String) -> R<Vec<RemoteEntry>> {
    app.sftp_list(sftp, &path).await
}
#[tauri::command]
async fn sftp_mkdir(app: State<'_, App>, sftp: Uuid, path: String) -> R<()> {
    app.sftp_mkdir(sftp, &path).await
}
#[tauri::command]
async fn sftp_rename(app: State<'_, App>, sftp: Uuid, from: String, to: String) -> R<()> {
    app.sftp_rename(sftp, &from, &to).await
}
#[tauri::command]
async fn sftp_remove(app: State<'_, App>, sftp: Uuid, path: String) -> R<()> {
    app.sftp_remove(sftp, &path).await
}
#[tauri::command]
async fn sftp_upload(app: State<'_, App>, sftp: Uuid, transfer: Uuid, local: String, remote: String) -> R<u64> {
    app.sftp_upload(sftp, transfer, &local, &remote).await
}
#[tauri::command]
async fn sftp_download(app: State<'_, App>, sftp: Uuid, transfer: Uuid, remote: String, local: String) -> R<u64> {
    app.sftp_download(sftp, transfer, &remote, &local).await
}
#[tauri::command]
fn local_list(app: State<'_, App>, path: String) -> R<Vec<LocalEntry>> {
    app.local_list(&path)
}
#[tauri::command]
fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".into())
}

/// Registers every command; shared by the app and the IPC contract tests.
pub fn with_commands<Rt: tauri::Runtime>(builder: tauri::Builder<Rt>) -> tauri::Builder<Rt> {
    builder
        .invoke_handler(tauri::generate_handler![
            vault_status, vault_create, vault_unlock, vault_lock, vault_change_password, vault_export, vault_import,
            inventory, save_host, delete_host, save_group, delete_group, save_identity, delete_identity,
            set_password, resolve_host, key_generate, key_import, delete_key, save_snippet, delete_snippet,
            snippet_variables, save_forward, delete_forward, known_hosts, delete_known_host,
            import_ssh_config, import_known_hosts, get_settings, save_settings,
            open_ssh_session, open_local_session, session_write, session_resize, session_close,
            session_broadcast, host_key_respond,
            forward_start, forward_stop, forwards_active, run_snippet,
            sftp_open, sftp_close, sftp_home, sftp_list, sftp_mkdir, sftp_rename, sftp_remove,
            sftp_upload, sftp_download, local_list, home_dir,
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    with_commands(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .setup(|tauri_app| {
            let data_dir = tauri_app.path().app_data_dir()?;
            let handle = tauri_app.handle().clone();
            let events = Arc::new(move |e: wh_app::AppEvent| {
                let _ = handle.emit("wh-event", e);
            });
            let app = App::new(data_dir, Default::default(), events).map_err(|e| e.message)?;
            tauri_app.manage(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
