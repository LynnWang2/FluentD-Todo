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
            // Closing the window hides it to the tray instead of quitting;
            // the app exits only from the tray menu （退出） or Cmd+Q.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if !QUITTING.load(std::sync::atomic::Ordering::Relaxed) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            #[cfg(desktop)]
            {
                use tauri::Manager;
                use tauri::window::EffectsBuilder;
                if let Some(window) = app.get_webview_window("main") {
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
            set_window_corner_radius
        ])
        .build(tauri::generate_context!())
        .expect("failed to run Fluent MD Todo");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            QUITTING.store(true, std::sync::atomic::Ordering::Relaxed);
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
