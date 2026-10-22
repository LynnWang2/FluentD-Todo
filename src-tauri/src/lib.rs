#[cfg(target_os = "windows")]
fn restore_acrylic_after_pointer_release(window: tauri::WebviewWindow) {
    use std::{thread, time::Duration};
    use tauri::Emitter;
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(35));
        while unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x01) } < 0 {
            thread::sleep(Duration::from_millis(12));
        }
        let _ = window.emit("window-move-finished", ());
    });
}

#[derive(serde::Serialize)]
struct LastMarkdownFile {
    path: String,
    content: String,
}

fn remembered_path_file(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    let mut data_dir = app.path().app_local_data_dir().map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    data_dir.push("last-md-file.txt");
    Ok(data_dir)
}

fn load_remembered_path(app: &tauri::AppHandle) -> Result<Option<std::path::PathBuf>, String> {
    let record = remembered_path_file(app)?;
    match std::fs::read_to_string(record) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(std::path::PathBuf::from(value.trim()))),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// Remembered UI state (window geometry + opacity + theme mode), stored as
/// JSON next to the remembered-file record so the app reopens the way the
/// user left it. Geometry is captured by Rust itself on window events;
/// opacity/theme are reported by the frontend through save_ui_prefs.
#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
struct UiState {
    x: Option<i32>,
    y: Option<i32>,
    width: Option<u32>,
    height: Option<u32>,
    opacity: Option<f64>,
    theme: Option<String>,
    hide_completed: Option<bool>,
    quick_section: Option<String>,
    always_on_top: Option<bool>,
}

fn ui_state_file(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    let mut data_dir = app.path().app_local_data_dir().map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    data_dir.push("ui-state.json");
    Ok(data_dir)
}

fn load_ui_state(app: &tauri::AppHandle) -> UiState {
    let Ok(file) = ui_state_file(app) else { return UiState::default() };
    let Ok(text) = std::fs::read_to_string(file) else { return UiState::default() };
    serde_json::from_str(&text).unwrap_or_default()
}

fn store_ui_state(app: &tauri::AppHandle, state: &UiState) -> Result<(), String> {
    let text = serde_json::to_string(state).map_err(|error| error.to_string())?;
    std::fs::write(ui_state_file(app)?, text).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_ui_state(app: tauri::AppHandle) -> Result<UiState, String> {
    Ok(load_ui_state(&app))
}

#[tauri::command]
fn save_ui_prefs(app: tauri::AppHandle, opacity: Option<f64>, theme: Option<String>, hide_completed: Option<bool>, quick_section: Option<String>, always_on_top: Option<bool>) -> Result<(), String> {
    let mut state = load_ui_state(&app);
    if opacity.is_some() {
        state.opacity = opacity;
    }
    if theme.is_some() {
        state.theme = theme;
    }
    if hide_completed.is_some() {
        state.hide_completed = hide_completed;
    }
    if quick_section.is_some() {
        state.quick_section = quick_section;
    }
    if always_on_top.is_some() {
        state.always_on_top = always_on_top;
    }
    store_ui_state(&app, &state)
}

static LAST_GEOMETRY_SAVE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Snapshot the main window's position/size into the UI state. Move/resize
/// events fire continuously while dragging, so those are throttled; exits
/// pass force=true to always land the final resting place. Maximized and
/// minimized states are skipped so a later restore never resurrects them.
fn capture_window_geometry(window: &tauri::Window, force: bool) {
    use tauri::Manager;
    if !force {
        let Ok(mut last) = LAST_GEOMETRY_SAVE.lock() else { return };
        if let Some(saved_at) = *last {
            if saved_at.elapsed() < std::time::Duration::from_millis(400) {
                return;
            }
        }
        *last = Some(std::time::Instant::now());
    }
    if window.is_maximized().unwrap_or(false) || window.is_minimized().unwrap_or(false) {
        return;
    }
    let Ok(size) = window.outer_size() else { return };
    let Ok(position) = window.outer_position() else { return };
    if size.width < 200 || size.height < 200 || position.x < -10000 || position.y < -10000 {
        return;
    }
    let app = window.app_handle();
    let mut state = load_ui_state(app);
    state.x = Some(position.x);
    state.y = Some(position.y);
    state.width = Some(size.width);
    state.height = Some(size.height);
    let _ = store_ui_state(app, &state);
}

/// True when the saved window rect intersects at least one connected
/// monitor (guards against restoring onto a since-unplugged display).
fn rect_on_screen(window: &tauri::WebviewWindow, x: i32, y: i32, width: u32, height: u32) -> bool {
    let Ok(monitors) = window.available_monitors() else { return true };
    if monitors.is_empty() {
        return true;
    }
    monitors.iter().any(|monitor| {
        let origin = monitor.position();
        let size = monitor.size();
        let (right, bottom) = (origin.x + size.width as i32, origin.y + size.height as i32);
        x < right && x + width as i32 > origin.x && y < bottom && y + height as i32 > origin.y
    })
}

#[tauri::command]
fn remember_md_file(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let md_path = std::path::PathBuf::from(&path);
    let extension = md_path.extension().and_then(|value| value.to_str()).unwrap_or("");
    if !["md", "markdown", "txt"].iter().any(|allowed| extension.eq_ignore_ascii_case(allowed)) {
        return Err("仅支持 Markdown 文档".into());
    }
    std::fs::write(remembered_path_file(&app)?, path).map_err(|error| error.to_string())
}

#[tauri::command]
fn read_last_md_file(app: tauri::AppHandle) -> Result<Option<LastMarkdownFile>, String> {
    let Some(path) = load_remembered_path(&app)? else { return Ok(None); };
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Some(LastMarkdownFile { path: path.to_string_lossy().into_owned(), content })),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let _ = std::fs::remove_file(remembered_path_file(&app)?);
            Ok(None)
        }
        Err(error) => Err(error.to_string()),
    }
}

fn verify_remembered_file(app: &tauri::AppHandle, path: &str) -> Result<(), String> {
    let remembered = load_remembered_path(app)?.ok_or("尚未选择 Markdown 文件")?;
    if remembered != std::path::PathBuf::from(path) {
        return Err("文件路径与上次选择的文档不一致".into());
    }
    Ok(())
}

#[tauri::command]
fn read_md_file(app: tauri::AppHandle, path: String) -> Result<String, String> {
    verify_remembered_file(&app, &path)?;
    std::fs::read_to_string(path).map_err(|error| error.to_string())
}

#[tauri::command]
fn write_md_file(app: tauri::AppHandle, path: String, content: String) -> Result<(), String> {
    verify_remembered_file(&app, &path)?;
    std::fs::write(path, content).map_err(|error| error.to_string())
}

#[tauri::command]
fn begin_window_drag(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|error| error.to_string())?;
    #[cfg(target_os = "windows")]
    restore_acrylic_after_pointer_release(window);
    Ok(())
}

#[tauri::command]
fn begin_window_resize(window: tauri::WebviewWindow, direction: String) -> Result<(), String> {
    use tauri::webview::Webview;
    let direction = match direction.as_str() {
        "North" => tauri_runtime::ResizeDirection::North,
        "South" => tauri_runtime::ResizeDirection::South,
        "East" => tauri_runtime::ResizeDirection::East,
        "West" => tauri_runtime::ResizeDirection::West,
        "NorthEast" => tauri_runtime::ResizeDirection::NorthEast,
        "NorthWest" => tauri_runtime::ResizeDirection::NorthWest,
        "SouthEast" => tauri_runtime::ResizeDirection::SouthEast,
        "SouthWest" => tauri_runtime::ResizeDirection::SouthWest,
        _ => return Err("Invalid resize direction".into()),
    };
    let webview: Webview = window.as_ref().clone();
    webview.window().start_resize_dragging(direction).map_err(|error| error.to_string())?;
    #[cfg(target_os = "windows")]
    restore_acrylic_after_pointer_release(window);
    Ok(())
}

#[tauri::command]
fn get_autostart_state(_app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    return windows_autostart::is_enabled();
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_autostart::ManagerExt;
        _app.autolaunch().is_enabled().map_err(|error| error.to_string())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    Ok(false)
}

#[tauri::command]
fn set_autostart_state(_app: tauri::AppHandle, enabled: bool) -> Result<(), String> {    #[cfg(target_os = "windows")]
    return windows_autostart::set_enabled(enabled);
    #[cfg(target_os = "macos")]
    {
        use tauri_plugin_autostart::ManagerExt;
        if enabled { _app.autolaunch().enable().map_err(|error| error.to_string()) }
        else { _app.autolaunch().disable().map_err(|error| error.to_string()) }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    { let _ = enabled; Ok(()) }
}

/// Clip the whole native window (webview + vibrancy backdrop) to a corner
/// radius on macOS. A CSS radius on the page only rounds the web surface;
/// the square frosted layer behind it still shows at the corners, so the
/// window keeps looking square. Clipping the NSWindow content view's layer
/// rounds every layer at once. No-op elsewhere (radius 0 restores square,
/// used while maximized, like native windows).
///
/// Commands run on a Tauri worker thread, but AppKit must run on the main
/// thread (same lesson as the v2.0.16 Apple Silicon crash), so the native
/// calls are dispatched with run_on_main_thread.
#[tauri::command]
fn set_window_corner_radius(app: tauri::AppHandle, window: tauri::WebviewWindow, radius: f64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        app.run_on_main_thread(move || {
            let _ = window
                .with_webview(move |webview| {
                    use objc2::msg_send;
                    use objc2::runtime::AnyObject;
                    unsafe {
                        let ns_window = webview.ns_window() as *mut AnyObject;
                        if ns_window.is_null() {
                            return;
                        }
                        let content_view: *mut AnyObject = msg_send![ns_window, contentView];
                        if content_view.is_null() {
                            return;
                        }
                        let _: () = msg_send![content_view, setWantsLayer: true];
                        let layer: *mut AnyObject = msg_send![content_view, layer];
                        if layer.is_null() {
                            return;
                        }
                        let _: () = msg_send![layer, setCornerRadius: radius];
                        let _: () = msg_send![layer, setMasksToBounds: true];
                    }
                })
                .map_err(|error| error.to_string());
        })
        .map_err(|error| error.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, window, radius);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
mod windows_autostart {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::{HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW},
    };
    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const VALUE_NAME: &str = "Fluent MD Todo";
    fn wide(value: &str) -> Vec<u16> { OsStr::new(value).encode_wide().chain(Some(0)).collect() }

    pub fn is_enabled() -> Result<bool, String> {
        unsafe {
            let (key_name, value_name) = (wide(RUN_KEY), wide(VALUE_NAME));
            let mut key: HKEY = ptr::null_mut();
            let result = RegOpenKeyExW(HKEY_CURRENT_USER, key_name.as_ptr(), 0, KEY_QUERY_VALUE, &mut key);
            if result != ERROR_SUCCESS { return Ok(false); }
            let mut value_type = 0;
            let mut size = 0;
            let result = RegQueryValueExW(key, value_name.as_ptr(), ptr::null(), &mut value_type, ptr::null_mut(), &mut size);
            RegCloseKey(key);
            Ok(result == ERROR_SUCCESS && value_type == REG_SZ)
        }
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        unsafe {
            let (key_name, value_name) = (wide(RUN_KEY), wide(VALUE_NAME));
            if enabled {
                let mut key: HKEY = ptr::null_mut();
                let result = RegCreateKeyExW(HKEY_CURRENT_USER, key_name.as_ptr(), 0, ptr::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, ptr::null(), &mut key, ptr::null_mut());
                if result != ERROR_SUCCESS { return Err(format!("Registry key error: {result}")); }
                let executable = std::env::current_exe().map_err(|error| error.to_string())?;
                let command = wide(&format!("\"{}\" --autostart", executable.display()));
                let result = RegSetValueExW(key, value_name.as_ptr(), 0, REG_SZ, command.as_ptr().cast(), (command.len() * 2) as u32);
                RegCloseKey(key);
                if result != ERROR_SUCCESS { return Err(format!("Registry write error: {result}")); }
            } else {
                let mut key: HKEY = ptr::null_mut();
                let result = RegOpenKeyExW(HKEY_CURRENT_USER, key_name.as_ptr(), 0, KEY_SET_VALUE, &mut key);
                if result == ERROR_FILE_NOT_FOUND { return Ok(()); }
                if result != ERROR_SUCCESS { return Err(format!("Registry key error: {result}")); }
                let result = RegDeleteValueW(key, value_name.as_ptr());
                RegCloseKey(key);
                if result != ERROR_SUCCESS && result != ERROR_FILE_NOT_FOUND { return Err(format!("Registry delete error: {result}")); }
            }
            Ok(())
        }
    }
}

/// Set once a real quit starts (tray 退出 / Cmd+Q), so the close-to-tray
/// interception in on_window_event lets the final window closes through.
static QUITTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Show the main window and focus it (tray click / Dock reopen / tray menu).
#[cfg(desktop)]
fn show_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// macOS: after the right-click menu pops, detach it again right away.
/// tray-icon 0.24 keeps a menu pinned to the NSStatusItem and macOS 27's
/// AppKit then pops that menu on ANY click and swallows the left click
/// before TrayIconEvent::Click fires; upstream fixed this in tray-icon
/// 0.25.1 (mount only for the pop) but tauri 2.11.x pins tray-icon ^0.24,
/// so replicate the fix at the app layer (same approach as DuckColorPicker).
#[cfg(target_os = "macos")]
fn detach_tray_menu<R: tauri::Runtime>(tray: &tauri::tray::TrayIcon<R>) {
    let _ = tray.set_menu(None::<tauri::menu::Menu<R>>);
}

/// System tray (Windows) / menu-bar extra (macOS): closing the window only
/// hides it (see on_window_event in run()); the app keeps living in the
/// tray, and the tray icon brings the window back. A single LEFT click
/// opens the window directly; the menu （打开 / 退出） appears on RIGHT
/// click only. The mac icon is a monochrome template image; Windows uses
/// the app icon.
#[cfg(desktop)]
fn build_tray(app: &mut tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show_item = MenuItem::with_id(app, "tray_show", "打开 Fluent MD Todo", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "tray_quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("Fluent MD Todo")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "tray_show" => show_main_window(app),
            "tray_quit" => {
                QUITTING.store(true, std::sync::atomic::Ordering::Relaxed);
                app.exit(0);
            }
            _ => {}
        });

    // Windows: the menu stays attached and the OS shows it on right-click;
    // show_menu_on_left_click(false) routes left clicks to the event below.
    #[cfg(not(target_os = "macos"))]
    {
        builder = builder
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    show_main_window(tray.app_handle());
                }
            });
    }

    // macOS: NO menu attached by default (see detach_tray_menu) — left click
    // reaches the handler and opens the window; on right-button DOWN the
    // menu is attached just long enough to pop, then detached again.
    #[cfg(target_os = "macos")]
    {
        let menu_for_event = menu.clone();
        builder = builder
            .show_menu_on_left_click(false)
            .on_tray_icon_event(move |tray, event| {
                if let TrayIconEvent::Click {
                    button,
                    button_state,
                    ..
                } = event
                {
                    match (button, button_state) {
                        (MouseButton::Left, MouseButtonState::Up) => {
                            show_main_window(tray.app_handle());
                        }
                        (MouseButton::Right, MouseButtonState::Down) => {
                            let _ = tray.set_menu(Some(menu_for_event.clone()));
                            let _ = tray.with_inner_tray_icon(|inner| inner.show_menu());
                            detach_tray_menu(tray);
                        }
                        _ => {}
                    }
                }
            });
    }

    #[cfg(target_os = "macos")]
    {
        let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-mac.png"))?;
        builder = builder.icon(icon).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(icon) = app.default_window_icon() {
            builder = builder.icon(icon.clone());
        }
    }
    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init());
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent,
        Some(vec!["--autostart"]),
    ));
    let app = builder
        .on_window_event(|window, event| {
            match event {
                // Remember where the user left the window (throttled while
                // dragging; the final position is forced on close/exit).
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                    capture_window_geometry(window, false);
                }
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    capture_window_geometry(window, true);
                    // Closing the window hides it to the tray instead of quitting;
                    // the app exits only from the tray menu （退出） or Cmd+Q.
                    if !QUITTING.load(std::sync::atomic::Ordering::Relaxed) {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
                _ => {}
            }
        })
        .setup(|app| {
            #[cfg(desktop)]
            {
                use tauri::Manager;
                use tauri::window::EffectsBuilder;
                if let Some(window) = app.get_webview_window("main") {
                    // Restore the window geometry the user left last time
                    // (position only when it still intersects a monitor).
                    let saved = load_ui_state(app.handle());
                    if let (Some(width), Some(height)) = (saved.width, saved.height) {
                        let _ = window.set_size(tauri::PhysicalSize::new(width, height));
                        if let (Some(x), Some(y)) = (saved.x, saved.y) {
                            if rect_on_screen(&window, x, y, width, height) {
                                let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
                            }
                        }
                    }
                    // Re-apply the pinned (always-on-top) state the user left.
                    if saved.always_on_top == Some(true) {
                        let _ = window.set_always_on_top(true);
                    }
                    #[cfg(target_os = "windows")]
                    let effects = EffectsBuilder::new().effect(tauri::window::Effect::Blur).build();
                    #[cfg(target_os = "macos")]
                    let effects = EffectsBuilder::new().effect(tauri::window::Effect::Sidebar).build();
                    #[cfg(any(target_os = "windows", target_os = "macos"))]
                    let _ = window.set_effects(effects);
                    #[cfg(target_os = "windows")]
                    let _ = window.set_shadow(false);
                }
                build_tray(app)?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            begin_window_drag,
            begin_window_resize,
            remember_md_file,
            read_last_md_file,
            read_md_file,
            write_md_file,
            get_autostart_state,
            set_autostart_state,
            set_window_corner_radius,
            get_ui_state,
            save_ui_prefs
        ])
        .build(tauri::generate_context!())
        .expect("failed to run Fluent MD Todo");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            QUITTING.store(true, std::sync::atomic::Ordering::Relaxed);
            // Tray quit / Cmd+Q never fires CloseRequested on the hidden
            // window, so land the final geometry here.
            use tauri::Manager;
            if let Some(webview_window) = app_handle.get_webview_window("main") {
                let webview: tauri::webview::Webview = webview_window.as_ref().clone();
                capture_window_geometry(&webview.window(), true);
            }
        }
        // Clicking the Dock icon while the window is hidden in the tray
        // brings it back (macOS sends Reopen, Tauri does not show by itself).
        #[cfg(target_os = "macos")]
        {
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main_window(app_handle);
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = app_handle;
    });
}
