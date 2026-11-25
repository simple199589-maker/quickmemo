#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod crypto;
mod keystore;
mod platform;
mod storage;
mod totp;

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use storage::{Session, Snapshot};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State, WebviewWindow, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_updater::UpdaterExt;

struct AppState {
    session: Mutex<Session>,
    start_hidden: AtomicBool,
}

fn with_session<T>(
    state: &State<'_, AppState>,
    action: impl FnOnce(&mut Session) -> Result<T, String>,
) -> Result<T, String> {
    let mut session = state
        .session
        .lock()
        .map_err(|_| "内部状态锁定失败".to_string())?;
    action(&mut session)
}

fn toggle_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }
}

fn setup_tray(app: &tauri::App) -> Result<(), String> {
    let toggle = MenuItem::with_id(app, "toggle", "显示/隐藏", true, None::<&str>)
        .map_err(|err| format!("创建托盘菜单失败：{err}"))?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)
        .map_err(|err| format!("创建托盘菜单失败：{err}"))?;
    let menu = Menu::with_items(app, &[&toggle, &quit])
        .map_err(|err| format!("创建托盘菜单失败：{err}"))?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| "缺少应用图标".to_string())?;
    let tray = TrayIconBuilder::with_id("main")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("备忘快贴")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => toggle_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        })
        .build(app)
        .map_err(|err| format!("创建托盘图标失败：{err}"))?;
    app.manage(tray);

    Ok(())
}

fn is_wayland() -> bool {
    cfg!(target_os = "linux")
        && std::env::var("XDG_SESSION_TYPE")
            .map(|value| value.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
}

fn shortcut_failure_hint(message: String) -> String {
    if is_wayland() {
        format!("{message}；Wayland 下建议在系统设置里把快捷键绑定到 `quickmemo --toggle`")
    } else {
        message
    }
}

fn register_saved_shortcut(app: &AppHandle) {
    let saved = {
        let state = app.state::<AppState>();
        let value = match state.session.lock() {
            Ok(session) => session.toggle_shortcut(),
            Err(_) => return,
        };
        value
    };

    if saved.is_empty() {
        if let Some(state) = app.try_state::<AppState>() {
            if let Ok(mut session) = state.session.lock() {
                session.set_shortcut_warning(None);
            }
        }
        return;
    }

    match Shortcut::from_str(&saved) {
        Ok(shortcut) => {
            if let Err(err) = app.global_shortcut().register(shortcut) {
                eprintln!("Quick Memo 快捷键 {saved} 注册失败：{err}");
                if let Some(state) = app.try_state::<AppState>() {
                    state.start_hidden.store(false, Ordering::Relaxed);
                    if let Ok(mut session) = state.session.lock() {
                        session.set_shortcut_warning(Some(shortcut_failure_hint(format!(
                            "快捷键 {saved} 注册失败：{err}"
                        ))));
                    }
                }
            } else if let Some(state) = app.try_state::<AppState>() {
                if let Ok(mut session) = state.session.lock() {
                    session.set_shortcut_warning(None);
                }
            }
        }
        Err(_) => {
            if let Some(state) = app.try_state::<AppState>() {
                state.start_hidden.store(false, Ordering::Relaxed);
                if let Ok(mut session) = state.session.lock() {
                    session.set_shortcut_warning(Some(shortcut_failure_hint(format!(
                        "快捷键 {saved} 格式无效"
                    ))));
                }
            }
        }
    }
}

#[tauri::command]
fn get_state(state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| session.snapshot())
}

#[derive(serde::Serialize)]
struct UpdateInfo {
    version: String,
    current_version: String,
    notes: Option<String>,
    date: Option<String>,
}

fn updater_configured(app: &AppHandle) -> bool {
    let Some(updater) = app.config().plugins.0.get("updater") else {
        return false;
    };
    let pubkey = updater
        .get("pubkey")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if pubkey.is_empty() || pubkey.contains("REPLACE_WITH") {
        return false;
    }
    let Some(endpoints) = updater.get("endpoints").and_then(|value| value.as_array()) else {
        return false;
    };
    !endpoints.is_empty()
        && endpoints.iter().all(|endpoint| {
            endpoint
                .as_str()
                .map(|value| !value.contains("OWNER/REPO"))
                .unwrap_or(false)
        })
}

fn ensure_updater_configured(app: &AppHandle) -> Result<(), String> {
    if updater_configured(app) {
        Ok(())
    } else {
        Err("自动更新尚未配置：请在 tauri.conf.json 中填写更新公钥和 GitHub 仓库地址".to_string())
    }
}

#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    ensure_updater_configured(&app)?;
    let updater = app.updater().map_err(|err| format!("检查更新失败：{err}"))?;
    let update = updater
        .check()
        .await
        .map_err(|err| format!("检查更新失败：{err}"))?;
    Ok(update.map(|update| UpdateInfo {
        version: update.version,
        current_version: update.current_version,
        notes: update.body,
        date: update.date.map(|date| date.to_string()),
    }))
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    ensure_updater_configured(&app)?;
    let updater = app.updater().map_err(|err| format!("检查更新失败：{err}"))?;
    let update = updater
        .check()
        .await
        .map_err(|err| format!("检查更新失败：{err}"))?
        .ok_or_else(|| "已经是最新版本".to_string())?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|err| format!("安装更新失败：{err}"))?;
    app.restart()
}

#[tauri::command]
fn unlock(
    totp_code: Option<String>,
    data_password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.unlock(totp_code, data_password)?;
        session.snapshot()
    })
}

#[tauri::command]
fn lock(state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.lock()?;
        session.snapshot()
    })
}

#[tauri::command]
fn set_ttl_days(days: u64, state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.set_ttl_days(days)?;
        session.snapshot()
    })
}

#[tauri::command]
fn start_totp_setup(state: State<'_, AppState>) -> Result<totp::TotpSetup, String> {
    with_session(&state, |session| session.start_totp_setup())
}

#[tauri::command]
fn confirm_totp_setup(
    secret: String,
    code: String,
    state: State<'_, AppState>,
) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.confirm_totp_setup(&secret, &code)?;
        session.snapshot()
    })
}

#[tauri::command]
fn disable_totp(state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.disable_totp()?;
        session.snapshot()
    })
}

#[tauri::command]
fn set_data_password(password: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.set_data_password(&password)?;
        session.snapshot()
    })
}

#[tauri::command]
fn disable_data_password(state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.disable_data_password()?;
        session.snapshot()
    })
}

#[tauri::command]
fn create_group(name: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.create_group(&name)?;
        session.snapshot()
    })
}

#[tauri::command]
fn rename_group(id: String, name: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.rename_group(&id, &name)?;
        session.snapshot()
    })
}

#[tauri::command]
fn delete_group(id: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.delete_group(&id)?;
        session.snapshot()
    })
}

#[tauri::command]
fn create_tag(
    group_id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.create_tag(&group_id, &name)?;
        session.snapshot()
    })
}

#[tauri::command]
fn rename_tag(id: String, name: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.rename_tag(&id, &name)?;
        session.snapshot()
    })
}

#[tauri::command]
fn move_tag(
    id: String,
    group_id: String,
    state: State<'_, AppState>,
) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.move_tag(&id, &group_id)?;
        session.snapshot()
    })
}

#[tauri::command]
fn delete_tag(id: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.delete_tag(&id)?;
        session.snapshot()
    })
}

#[tauri::command]
fn create_entry(
    tag_id: String,
    title: String,
    body: String,
    state: State<'_, AppState>,
) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.create_entry(&tag_id, &title, &body)?;
        session.snapshot()
    })
}

#[tauri::command]
fn update_entry(
    id: String,
    title: String,
    body: String,
    state: State<'_, AppState>,
) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.update_entry(&id, &title, &body)?;
        session.snapshot()
    })
}

#[tauri::command]
fn delete_entry(id: String, state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.delete_entry(&id)?;
        session.snapshot()
    })
}

#[tauri::command]
fn set_autostart(enabled: bool, state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.set_autostart(enabled)?;
        session.snapshot()
    })
}

#[tauri::command]
fn set_start_hidden(enabled: bool, state: State<'_, AppState>) -> Result<Snapshot, String> {
    with_session(&state, |session| {
        session.set_start_hidden(enabled)?;
        session.snapshot()
    })
}

#[tauri::command]
fn set_toggle_shortcut(
    shortcut: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Snapshot, String> {
    let shortcut = shortcut.trim().to_string();
    let old = with_session(&state, |session| Ok(session.toggle_shortcut()))?;

    if old == shortcut {
        return with_session(&state, |session| session.snapshot());
    }

    let new_shortcut = if shortcut.is_empty() {
        None
    } else {
        Some(Shortcut::from_str(&shortcut).map_err(|_| "快捷键格式无效".to_string())?)
    };

    if !old.is_empty() {
        if let Ok(parsed) = Shortcut::from_str(&old) {
            let _ = app.global_shortcut().unregister(parsed);
        }
    }

    if let Some(parsed) = new_shortcut {
        if let Err(err) = app.global_shortcut().register(parsed) {
            if !old.is_empty() {
                if let Ok(old_shortcut) = Shortcut::from_str(&old) {
                    let _ = app.global_shortcut().register(old_shortcut);
                }
            }
            return Err(format!("快捷键注册失败：{err}"));
        }
    }

    with_session(&state, |session| {
        session.set_toggle_shortcut(&shortcut)?;
        session.set_shortcut_warning(None);
        session.snapshot()
    })
}

#[tauri::command]
fn suspend_shortcut(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let shortcut = with_session(&state, |session| Ok(session.toggle_shortcut()))?;
    if !shortcut.is_empty() {
        if let Ok(parsed) = Shortcut::from_str(&shortcut) {
            let _ = app.global_shortcut().unregister(parsed);
        }
    }
    Ok(())
}

#[tauri::command]
fn resume_shortcut(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let shortcut = with_session(&state, |session| Ok(session.toggle_shortcut()))?;
    if shortcut.is_empty() {
        return Ok(());
    }
    let parsed = Shortcut::from_str(&shortcut).map_err(|_| "快捷键格式无效".to_string())?;
    app.global_shortcut()
        .register(parsed)
        .map_err(|err| format!("快捷键注册失败：{err}"))
}

#[tauri::command]
fn open_data_dir(state: State<'_, AppState>) -> Result<(), String> {
    with_session(&state, |session| session.open_data_dir())
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    platform::copy_to_clipboard(&text)
}

#[tauri::command]
fn hide_window(window: WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|err| err.to_string())
}

#[tauri::command]
fn frontend_ready(window: WebviewWindow, state: State<'_, AppState>) {
    if !state.start_hidden.load(Ordering::Relaxed) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn main() {
    let start_hidden = std::env::args().any(|arg| arg == "--hidden");
    let session = Session::load().unwrap_or_else(|err| {
        eprintln!("Quick Memo 启动失败：{err}");
        std::process::exit(1);
    });

    tauri::Builder::default()
        .manage(AppState {
            session: Mutex::new(session),
            start_hidden: AtomicBool::new(start_hidden),
        })
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|arg| arg == "--toggle") {
                toggle_main_window(app);
            } else if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_main_window(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_state,
            check_update,
            install_update,
            unlock,
            lock,
            set_ttl_days,
            start_totp_setup,
            confirm_totp_setup,
            disable_totp,
            set_data_password,
            disable_data_password,
            create_group,
            rename_group,
            delete_group,
            create_tag,
            rename_tag,
            move_tag,
            delete_tag,
            create_entry,
            update_entry,
            delete_entry,
            set_autostart,
            set_start_hidden,
            set_toggle_shortcut,
            suspend_shortcut,
            resume_shortcut,
            open_data_dir,
            copy_text,
            hide_window,
            frontend_ready
        ])
        .setup(|app| {
            if let Err(err) = setup_tray(app) {
                eprintln!("Quick Memo 托盘创建失败：{err}");
                if let Some(state) = app.try_state::<AppState>() {
                    state.start_hidden.store(false, Ordering::Relaxed);
                    if let Ok(mut session) = state.session.lock() {
                        session.set_warning(format!(
                            "托盘图标创建失败：{err}；启动时会直接显示窗口"
                        ));
                    }
                }
            }
            register_saved_shortcut(app.handle());

            // 点窗口关闭按钮时隐藏到托盘，只有托盘菜单的「退出」才真正结束进程。
            if let Some(window) = app.get_webview_window("main") {
                let handle = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        if handle.app_handle().tray_by_id("main").is_some() {
                            api.prevent_close();
                            let _ = handle.hide();
                        }
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Quick Memo");
}
