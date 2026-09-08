use devkit_core::{watch, DbCredentials, DevKit, PhpExtensionInfo, PortsSettings, StackStatus};
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

struct KitState(Arc<Mutex<DevKit>>);

fn map_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

async fn with_kit<T, F>(state: &KitState, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&mut DevKit) -> Result<T, String> + Send + 'static,
{
    let kit = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = kit.lock();
        f(&mut guard)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_status(state: State<'_, KitState>) -> Result<StackStatus, String> {
    with_kit(&state, |kit| kit.status_light().map_err(map_err)).await
}

#[tauri::command]
async fn check_ports(state: State<'_, KitState>) -> Result<StackStatus, String> {
    with_kit(&state, |kit| kit.status().map_err(map_err)).await
}

#[tauri::command]
async fn start_all(state: State<'_, KitState>) -> Result<StackStatus, String> {
    with_kit(&state, |kit| kit.start_all(true).map_err(map_err)).await
}

#[tauri::command]
async fn stop_all(state: State<'_, KitState>) -> Result<StackStatus, String> {
    with_kit(&state, |kit| kit.stop_all().map_err(map_err)).await
}

#[tauri::command]
async fn start_service(state: State<'_, KitState>, id: String) -> Result<StackStatus, String> {
    with_kit(&state, move |kit| {
        kit.start_service(&id, true).map_err(map_err)?;
        kit.status_light().map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn stop_service(state: State<'_, KitState>, id: String) -> Result<StackStatus, String> {
    with_kit(&state, move |kit| {
        kit.stop_service(&id).map_err(map_err)?;
        kit.status_light().map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn reload_sites(state: State<'_, KitState>) -> Result<StackStatus, String> {
    with_kit(&state, |kit| {
        kit.reload_sites().map_err(map_err)?;
        kit.status_light().map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn open_terminal(state: State<'_, KitState>) -> Result<(), String> {
    with_kit(&state, |kit| kit.open_terminal().map_err(map_err)).await
}

#[tauri::command]
async fn adminer_url(state: State<'_, KitState>) -> Result<String, String> {
    with_kit(&state, |kit| Ok(kit.adminer_url())).await
}

#[tauri::command]
async fn default_url(state: State<'_, KitState>) -> Result<String, String> {
    with_kit(&state, |kit| Ok(kit.default_url())).await
}

#[tauri::command]
async fn isolated_path(state: State<'_, KitState>) -> Result<String, String> {
    with_kit(&state, |kit| Ok(kit.isolated_path())).await
}

#[tauri::command]
async fn create_site(
    state: State<'_, KitState>,
    kind: String,
    name: String,
) -> Result<StackStatus, String> {
    with_kit(&state, move |kit| {
        kit.create_site(&kind, &name).map_err(map_err)?;
        kit.status_light().map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn setup_stack(
    state: State<'_, KitState>,
    full: bool,
    php: Option<String>,
) -> Result<Vec<String>, String> {
    with_kit(&state, move |kit| {
        kit.setup(full, php.as_deref()).map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn php_versions(state: State<'_, KitState>) -> Result<Vec<String>, String> {
    with_kit(&state, |kit| {
        let mut v = kit.php_versions().map_err(map_err)?;
        if !v.contains(&kit.config.php_version) {
            v.push(kit.config.php_version.clone());
        }
        Ok(v)
    })
    .await
}

#[tauri::command]
async fn use_php(state: State<'_, KitState>, version: String) -> Result<StackStatus, String> {
    with_kit(&state, move |kit| {
        kit.use_php(&version).map_err(map_err)?;
        kit.status_light().map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn enable_ssl(state: State<'_, KitState>) -> Result<String, String> {
    with_kit(&state, |kit| kit.enable_ssl().map_err(map_err)).await
}

#[tauri::command]
async fn pack_portable(state: State<'_, KitState>) -> Result<String, String> {
    with_kit(&state, |kit| {
        kit.pack(None)
            .map(|p| p.display().to_string())
            .map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn get_ports_config(state: State<'_, KitState>) -> Result<PortsSettings, String> {
    with_kit(&state, |kit| Ok(kit.ports_config())).await
}

#[tauri::command]
async fn update_ports(
    state: State<'_, KitState>,
    ports: PortsSettings,
) -> Result<StackStatus, String> {
    with_kit(&state, move |kit| kit.update_ports(ports).map_err(map_err)).await
}

#[tauri::command]
async fn ensure_hosts(state: State<'_, KitState>) -> Result<String, String> {
    with_kit(&state, |kit| kit.ensure_hosts().map_err(map_err)).await
}

#[tauri::command]
async fn get_db_credentials(state: State<'_, KitState>) -> Result<DbCredentials, String> {
    with_kit(&state, |kit| kit.db_credentials().map_err(map_err)).await
}

#[tauri::command]
async fn update_db_credentials(
    state: State<'_, KitState>,
    creds: DbCredentials,
) -> Result<String, String> {
    with_kit(&state, move |kit| {
        kit.update_db_credentials(creds).map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn regenerate_vhosts(state: State<'_, KitState>) -> Result<String, String> {
    with_kit(&state, |kit| kit.regenerate_vhosts().map_err(map_err)).await
}

#[tauri::command]
async fn open_site_vhost(state: State<'_, KitState>, name: String) -> Result<String, String> {
    with_kit(&state, move |kit| kit.open_site_vhost(&name).map_err(map_err)).await
}

#[tauri::command]
async fn list_php_extensions(state: State<'_, KitState>) -> Result<Vec<PhpExtensionInfo>, String> {
    with_kit(&state, |kit| kit.list_php_extensions().map_err(map_err)).await
}

#[tauri::command]
async fn set_php_extensions(
    state: State<'_, KitState>,
    extensions: Vec<String>,
) -> Result<String, String> {
    with_kit(&state, move |kit| {
        kit.set_php_extensions(extensions).map_err(map_err)
    })
    .await
}

#[tauri::command]
fn open_in_browser(app: AppHandle, url: String) -> Result<(), String> {
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| e.to_string())
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let kit = DevKit::load().unwrap_or_else(|e| {
        eprintln!("DevKit load: {e}");
        panic!("{e}");
    });
    let shared = Arc::new(std::sync::Mutex::new(kit));
    let _ = watch::spawn_www_watcher(shared.clone());

    // parking_lot mutex for tauri commands; keep a second DevKit? We can't easily share two mutex types.
    // Re-load for Tauri state (same files on disk). Watcher uses its own instance.
    let ui_kit = DevKit::load().expect("DevKit");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(KitState(Arc::new(Mutex::new(ui_kit))))
        .invoke_handler(tauri::generate_handler![
            get_status,
            check_ports,
            get_ports_config,
            update_ports,
            ensure_hosts,
            get_db_credentials,
            update_db_credentials,
            regenerate_vhosts,
            open_site_vhost,
            list_php_extensions,
            set_php_extensions,
            start_all,
            stop_all,
            start_service,
            stop_service,
            reload_sites,
            open_terminal,
            adminer_url,
            default_url,
            isolated_path,
            create_site,
            setup_stack,
            php_versions,
            use_php,
            enable_ssl,
            pack_portable,
            open_in_browser
        ])
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "Abrir panel", true, None::<&str>)?;
            let start = MenuItem::with_id(app, "start", "Start All", true, None::<&str>)?;
            let stop = MenuItem::with_id(app, "stop", "Stop All", true, None::<&str>)?;
            let term = MenuItem::with_id(app, "term", "Terminal DevKit", true, None::<&str>)?;
            let adminer = MenuItem::with_id(app, "adminer", "Adminer", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &sep, &start, &stop, &term, &adminer, &sep, &quit])?;

            let handle = app.handle().clone();
            TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("DevKit")
                .icon(app.default_window_icon().unwrap().clone())
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "start" => {
                        if let Some(s) = app.try_state::<KitState>() {
                            let kit = s.0.clone();
                            std::thread::spawn(move || {
                                let _ = kit.lock().start_all(true);
                            });
                        }
                    }
                    "stop" => {
                        if let Some(s) = app.try_state::<KitState>() {
                            let kit = s.0.clone();
                            std::thread::spawn(move || {
                                let _ = kit.lock().stop_all();
                            });
                        }
                    }
                    "term" => {
                        if let Some(s) = app.try_state::<KitState>() {
                            let kit = s.0.clone();
                            std::thread::spawn(move || {
                                let _ = kit.lock().open_terminal();
                            });
                        }
                    }
                    "adminer" => {
                        if let Some(s) = app.try_state::<KitState>() {
                            let url = s.0.lock().adminer_url();
                            let _ = handle.opener().open_url(&url, None::<&str>);
                        }
                    }
                    "quit" => {
                        if let Some(s) = app.try_state::<KitState>() {
                            let kit = s.0.clone();
                            let _ = std::thread::spawn(move || {
                                let _ = kit.lock().stop_all();
                            })
                            .join();
                        }
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running DevKit");
}
