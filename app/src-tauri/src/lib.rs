use devkit_core::{watch, DevKit, StackStatus};
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

struct KitState(Mutex<DevKit>);

fn map_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
fn get_status(state: State<KitState>) -> Result<StackStatus, String> {
    state.0.lock().status().map_err(map_err)
}

#[tauri::command]
fn start_all(state: State<KitState>) -> Result<StackStatus, String> {
    state.0.lock().start_all(true).map_err(map_err)
}

#[tauri::command]
fn stop_all(state: State<KitState>) -> Result<StackStatus, String> {
    state.0.lock().stop_all().map_err(map_err)
}

#[tauri::command]
fn start_service(state: State<KitState>, id: String) -> Result<StackStatus, String> {
    let mut kit = state.0.lock();
    kit.start_service(&id, true).map_err(map_err)?;
    kit.status().map_err(map_err)
}

#[tauri::command]
fn stop_service(state: State<KitState>, id: String) -> Result<StackStatus, String> {
    let mut kit = state.0.lock();
    kit.stop_service(&id).map_err(map_err)?;
    kit.status().map_err(map_err)
}

#[tauri::command]
fn reload_sites(state: State<KitState>) -> Result<StackStatus, String> {
    let mut kit = state.0.lock();
    kit.reload_sites().map_err(map_err)?;
    kit.status().map_err(map_err)
}

#[tauri::command]
fn open_terminal(state: State<KitState>) -> Result<(), String> {
    state.0.lock().open_terminal().map_err(map_err)
}

#[tauri::command]
fn adminer_url(state: State<KitState>) -> Result<String, String> {
    Ok(state.0.lock().adminer_url())
}

#[tauri::command]
fn default_url(state: State<KitState>) -> Result<String, String> {
    Ok(state.0.lock().default_url())
}

#[tauri::command]
fn isolated_path(state: State<KitState>) -> Result<String, String> {
    Ok(state.0.lock().isolated_path())
}

#[tauri::command]
fn create_site(state: State<KitState>, kind: String, name: String) -> Result<StackStatus, String> {
    let mut kit = state.0.lock();
    kit.create_site(&kind, &name).map_err(map_err)?;
    kit.status().map_err(map_err)
}

#[tauri::command]
fn setup_stack(state: State<KitState>, full: bool, php: Option<String>) -> Result<Vec<String>, String> {
    state
        .0
        .lock()
        .setup(full, php.as_deref())
        .map_err(map_err)
}

#[tauri::command]
fn php_versions(state: State<KitState>) -> Result<Vec<String>, String> {
    let kit = state.0.lock();
    let mut v = kit.php_versions().map_err(map_err)?;
    if !v.contains(&kit.config.php_version) {
        v.push(kit.config.php_version.clone());
    }
    Ok(v)
}

#[tauri::command]
fn use_php(state: State<KitState>, version: String) -> Result<StackStatus, String> {
    let mut kit = state.0.lock();
    kit.use_php(&version).map_err(map_err)?;
    kit.status().map_err(map_err)
}

#[tauri::command]
fn enable_ssl(state: State<KitState>) -> Result<String, String> {
    state.0.lock().enable_ssl().map_err(map_err)
}

#[tauri::command]
fn pack_portable(state: State<KitState>) -> Result<String, String> {
    state
        .0
        .lock()
        .pack(None)
        .map(|p| p.display().to_string())
        .map_err(map_err)
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
        .manage(KitState(Mutex::new(ui_kit)))
        .invoke_handler(tauri::generate_handler![
            get_status,
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
                            let _ = s.0.lock().start_all(true);
                        }
                    }
                    "stop" => {
                        if let Some(s) = app.try_state::<KitState>() {
                            let _ = s.0.lock().stop_all();
                        }
                    }
                    "term" => {
                        if let Some(s) = app.try_state::<KitState>() {
                            let _ = s.0.lock().open_terminal();
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
                            let _ = s.0.lock().stop_all();
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
