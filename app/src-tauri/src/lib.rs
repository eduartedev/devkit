use devkit_core::{
    build_mcp_release, mcp_exe_info, terminal, watch, AppConfig, AppSettings, AutoStartMode,
    DbCredentials, DevKit, McpExeInfo, Paths, PhpExtensionInfo, PortsSettings, StackStatus,
};
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

/// Snapshot para lecturas / acciones ligeras sin esperar el mutex del kit
/// mientras corre un trabajo pesado (start/stop/setup/…).
#[derive(Clone)]
struct LightSnap {
    paths: Paths,
    config: AppConfig,
    status: Option<StackStatus>,
}

impl LightSnap {
    fn from_kit(kit: &DevKit) -> Self {
        Self {
            paths: kit.paths.clone(),
            config: kit.config.clone(),
            status: kit.status_light().ok(),
        }
    }

    fn adminer_url(&self) -> String {
        format!(
            "http://{}:{}/adminer/",
            self.config.bind, self.config.http_port
        )
    }

    fn default_url(&self) -> String {
        format!("http://{}:{}/", self.config.bind, self.config.http_port)
    }

    fn mailpit_url(&self) -> String {
        format!("http://{}:{}/", self.config.bind, self.config.mailpit_ui)
    }
}

struct KitState {
    kit: Arc<Mutex<DevKit>>,
    snap: Arc<Mutex<LightSnap>>,
    /// Etiqueta de la operación pesada en curso (`None` = libre).
    heavy: Arc<Mutex<Option<String>>>,
}

fn map_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn publish_snap(kit: &DevKit, snap: &Mutex<LightSnap>) {
    *snap.lock() = LightSnap::from_kit(kit);
}

fn try_begin_heavy(heavy: &Mutex<Option<String>>, label: &str) -> Result<(), String> {
    let mut guard = heavy.lock();
    if let Some(cur) = guard.as_ref() {
        return Err(format!(
            "ya hay una operación en curso ({cur}); espera a que termine"
        ));
    }
    *guard = Some(label.to_string());
    Ok(())
}

fn end_heavy(heavy: &Mutex<Option<String>>) {
    *heavy.lock() = None;
}

/// Lectura/escritura corta del kit (actualiza snapshot al terminar).
async fn with_kit<T, F>(state: &KitState, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&mut DevKit) -> Result<T, String> + Send + 'static,
{
    let kit = state.kit.clone();
    let snap = state.snap.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = kit.lock();
        let out = f(&mut guard);
        publish_snap(&guard, &snap);
        out
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Trabajo pesado: como máximo uno a la vez; publica snapshot al entrar y al salir.
async fn with_heavy<T, F>(state: &KitState, label: &str, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&mut DevKit) -> Result<T, String> + Send + 'static,
{
    try_begin_heavy(&state.heavy, label)?;
    let kit = state.kit.clone();
    let snap = state.snap.clone();
    let heavy = state.heavy.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut guard = kit.lock();
        publish_snap(&guard, &snap);
        let out = f(&mut guard);
        publish_snap(&guard, &snap);
        out
    })
    .await
    .map_err(|e| e.to_string());
    end_heavy(&heavy);
    result?
}

#[tauri::command]
async fn get_status(state: State<'_, KitState>) -> Result<StackStatus, String> {
    let kit = state.kit.clone();
    let snap = state.snap.clone();
    tauri::async_runtime::spawn_blocking(move || match kit.try_lock() {
        Some(guard) => {
            let st = guard.status_light().map_err(map_err)?;
            publish_snap(&guard, &snap);
            Ok(st)
        }
        None => snap
            .lock()
            .status
            .clone()
            .ok_or_else(|| "operación en curso; estado aún no disponible".into()),
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn check_ports(state: State<'_, KitState>) -> Result<StackStatus, String> {
    with_heavy(&state, "ports", |kit| kit.status().map_err(map_err)).await
}

#[tauri::command]
async fn start_all(state: State<'_, KitState>) -> Result<StackStatus, String> {
    with_heavy(&state, "start", |kit| kit.start_all(true).map_err(map_err)).await
}

#[tauri::command]
async fn stop_all(state: State<'_, KitState>) -> Result<StackStatus, String> {
    with_heavy(&state, "stop", |kit| kit.stop_all().map_err(map_err)).await
}

#[tauri::command]
async fn start_service(state: State<'_, KitState>, id: String) -> Result<StackStatus, String> {
    with_heavy(&state, &format!("start:{id}"), move |kit| {
        kit.start_service(&id, true).map_err(map_err)?;
        kit.status_light().map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn stop_service(state: State<'_, KitState>, id: String) -> Result<StackStatus, String> {
    with_heavy(&state, &format!("stop:{id}"), move |kit| {
        kit.stop_service(&id).map_err(map_err)?;
        kit.status_light().map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn reload_sites(state: State<'_, KitState>) -> Result<StackStatus, String> {
    with_heavy(&state, "reload", |kit| {
        kit.reload_sites().map_err(map_err)?;
        kit.status_light().map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn open_terminal(state: State<'_, KitState>) -> Result<(), String> {
    // Paralelo acotado: usa snapshot, no espera al mutex del kit.
    let snap = state.snap.lock().clone();
    tauri::async_runtime::spawn_blocking(move || {
        terminal::open_terminal(&snap.paths, &snap.config).map_err(map_err)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn adminer_url(state: State<'_, KitState>) -> Result<String, String> {
    Ok(state.snap.lock().adminer_url())
}

#[tauri::command]
async fn default_url(state: State<'_, KitState>) -> Result<String, String> {
    Ok(state.snap.lock().default_url())
}

#[tauri::command]
async fn mailpit_url(state: State<'_, KitState>) -> Result<String, String> {
    Ok(state.snap.lock().mailpit_url())
}

#[tauri::command]
async fn isolated_path(state: State<'_, KitState>) -> Result<String, String> {
    let snap = state.snap.lock().clone();
    Ok(terminal::isolated_path(&snap.paths, &snap.config))
}

#[tauri::command]
async fn create_site(
    state: State<'_, KitState>,
    kind: String,
    name: String,
) -> Result<StackStatus, String> {
    with_heavy(&state, "create", move |kit| {
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
    with_heavy(&state, "setup", move |kit| {
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
    with_heavy(&state, "php", move |kit| {
        kit.use_php(&version).map_err(map_err)?;
        kit.status_light().map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn enable_ssl(state: State<'_, KitState>) -> Result<String, String> {
    with_heavy(&state, "ssl", |kit| kit.enable_ssl().map_err(map_err)).await
}

#[tauri::command]
async fn pack_portable(state: State<'_, KitState>) -> Result<String, String> {
    with_heavy(&state, "pack", |kit| {
        kit.pack(None)
            .map(|p| p.display().to_string())
            .map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn get_ports_config(state: State<'_, KitState>) -> Result<PortsSettings, String> {
    Ok(PortsSettings::from_config(&state.snap.lock().config))
}

#[tauri::command]
async fn update_ports(
    state: State<'_, KitState>,
    ports: PortsSettings,
) -> Result<StackStatus, String> {
    with_heavy(&state, "ports-save", move |kit| {
        kit.update_ports(ports).map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn ensure_hosts(state: State<'_, KitState>) -> Result<String, String> {
    with_heavy(&state, "hosts", |kit| kit.ensure_hosts().map_err(map_err)).await
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
    with_heavy(&state, "db", move |kit| {
        kit.update_db_credentials(creds).map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn get_app_settings(state: State<'_, KitState>) -> Result<AppSettings, String> {
    Ok(AppSettings::from_config(&state.snap.lock().config))
}

#[tauri::command]
async fn update_app_settings(
    app: AppHandle,
    state: State<'_, KitState>,
    settings: AppSettings,
) -> Result<String, String> {
    let launch_at_login = settings.launch_at_login;
    let msg = with_heavy(&state, "app-settings", move |kit| {
        kit.update_app_settings(settings).map_err(map_err)
    })
    .await?;
    sync_launch_at_login(&app, launch_at_login)?;
    Ok(msg)
}

#[tauri::command]
async fn regenerate_vhosts(state: State<'_, KitState>) -> Result<String, String> {
    with_heavy(&state, "vhosts", |kit| {
        kit.regenerate_vhosts().map_err(map_err)
    })
    .await
}

#[tauri::command]
async fn open_site_vhost(state: State<'_, KitState>, name: String) -> Result<String, String> {
    // Corto: abrir Notepad; si el kit está ocupado, falla claro.
    let kit = state.kit.clone();
    tauri::async_runtime::spawn_blocking(move || match kit.try_lock() {
        Some(guard) => guard.open_site_vhost(&name).map_err(map_err),
        None => Err("operación en curso; reintenta abrir el vhost en un momento".into()),
    })
    .await
    .map_err(|e| e.to_string())?
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
    with_heavy(&state, "php-ext", move |kit| {
        kit.set_php_extensions(extensions).map_err(map_err)
    })
    .await
}

#[tauri::command]
fn mcp_exe_status(state: State<'_, KitState>) -> McpExeInfo {
    let root = state.snap.lock().paths.root.clone();
    mcp_exe_info(&root)
}

#[tauri::command]
async fn build_mcp(state: State<'_, KitState>) -> Result<String, String> {
    try_begin_heavy(&state.heavy, "mcp")?;
    let root = state.snap.lock().paths.root.clone();
    let heavy = state.heavy.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        build_mcp_release(&root).map_err(map_err)
    })
    .await
    .map_err(|e| e.to_string());
    end_heavy(&heavy);
    result?
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

#[cfg(desktop)]
fn sync_launch_at_login(app: &AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch.enable().map_err(map_err)?;
    } else {
        autolaunch.disable().map_err(map_err)?;
    }
    Ok(())
}

#[cfg(not(desktop))]
fn sync_launch_at_login(_app: &AppHandle, _enabled: bool) -> Result<(), String> {
    Ok(())
}

fn apply_startup_on_launch(app: &AppHandle, state: &KitState) {
    let settings = AppSettings::from_config(&state.snap.lock().config);
    if settings.start_hidden {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.hide();
        }
    }
    #[cfg(desktop)]
    if let Err(e) = sync_launch_at_login(app, settings.launch_at_login) {
        eprintln!("DevKit autostart: {e}");
    }
    match settings.auto_start_mode {
        AutoStartMode::Off => {}
        AutoStartMode::All => {
            tray_run_heavy(state, "start", |kit| {
                let _ = kit.start_all(true);
            });
        }
        AutoStartMode::Selected => {
            let ids = settings.auto_start_services;
            if ids.is_empty() {
                return;
            }
            tray_run_heavy(state, "start", move |kit| {
                let _ = kit.start_services(&ids, true);
            });
        }
    }
}

fn tray_run_heavy(state: &KitState, label: &str, f: impl FnOnce(&mut DevKit) + Send + 'static) {
    let heavy = state.heavy.clone();
    if try_begin_heavy(&heavy, label).is_err() {
        return;
    }
    let kit = state.kit.clone();
    let snap = state.snap.clone();
    std::thread::spawn(move || {
        {
            let mut guard = kit.lock();
            publish_snap(&guard, &snap);
            f(&mut guard);
            publish_snap(&guard, &snap);
        }
        end_heavy(&heavy);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let kit = DevKit::load().unwrap_or_else(|e| {
        eprintln!("DevKit load: {e}");
        panic!("{e}");
    });
    let shared = Arc::new(std::sync::Mutex::new(kit));
    let _ = watch::spawn_www_watcher(shared.clone());

    // parking_lot mutex for tauri commands; watcher uses its own instance.
    let ui_kit = DevKit::load().expect("DevKit");
    let snap = LightSnap::from_kit(&ui_kit);
    let state = KitState {
        kit: Arc::new(Mutex::new(ui_kit)),
        snap: Arc::new(Mutex::new(snap)),
        heavy: Arc::new(Mutex::new(None)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_status,
            check_ports,
            get_ports_config,
            update_ports,
            ensure_hosts,
            get_db_credentials,
            update_db_credentials,
            get_app_settings,
            update_app_settings,
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
            mailpit_url,
            mcp_exe_status,
            build_mcp,
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
            #[cfg(desktop)]
            {
                use tauri_plugin_autostart::MacosLauncher;
                app.handle()
                    .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
                    .map_err(|e| e.to_string())?;
            }

            let show = MenuItem::with_id(app, "show", "Abrir panel", true, None::<&str>)?;
            let start = MenuItem::with_id(app, "start", "Start All", true, None::<&str>)?;
            let stop = MenuItem::with_id(app, "stop", "Stop All", true, None::<&str>)?;
            let term = MenuItem::with_id(app, "term", "Terminal DevKit", true, None::<&str>)?;
            let adminer = MenuItem::with_id(app, "adminer", "Adminer", true, None::<&str>)?;
            let mailpit = MenuItem::with_id(app, "mailpit", "Mailpit", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[&show, &sep, &start, &stop, &term, &adminer, &mailpit, &sep, &quit],
            )?;

            let handle = app.handle().clone();
            TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("DevKit")
                .icon(app.default_window_icon().unwrap().clone())
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "start" => {
                        if let Some(s) = app.try_state::<KitState>() {
                            tray_run_heavy(&*s, "start", |kit| {
                                let _ = kit.start_all(true);
                            });
                        }
                    }
                    "stop" => {
                        if let Some(s) = app.try_state::<KitState>() {
                            tray_run_heavy(&*s, "stop", |kit| {
                                let _ = kit.stop_all();
                            });
                        }
                    }
                    "term" => {
                        if let Some(s) = app.try_state::<KitState>() {
                            let snap = s.snap.lock().clone();
                            std::thread::spawn(move || {
                                let _ = terminal::open_terminal(&snap.paths, &snap.config);
                            });
                        }
                    }
                    "adminer" => {
                        if let Some(s) = app.try_state::<KitState>() {
                            let url = s.snap.lock().adminer_url();
                            let _ = handle.opener().open_url(&url, None::<&str>);
                        }
                    }
                    "mailpit" => {
                        if let Some(s) = app.try_state::<KitState>() {
                            let url = s.snap.lock().mailpit_url();
                            let _ = handle.opener().open_url(&url, None::<&str>);
                        }
                    }
                    "quit" => {
                        if let Some(s) = app.try_state::<KitState>() {
                            let kit = s.kit.clone();
                            let heavy = s.heavy.clone();
                            let snap = s.snap.clone();
                            let _ = std::thread::spawn(move || {
                                let _ = try_begin_heavy(&heavy, "stop");
                                {
                                    let mut g = kit.lock();
                                    let _ = g.stop_all();
                                    publish_snap(&g, &snap);
                                }
                                end_heavy(&heavy);
                            })
                            .join();
                        }
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;
            if let Some(s) = app.try_state::<KitState>() {
                apply_startup_on_launch(app.handle(), &*s);
            }
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
