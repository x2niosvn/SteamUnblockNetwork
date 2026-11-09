use crate::doh::{cached_domain_count, clear_doh_cache};
use crate::hosts::{flush_dns_cache, is_hosts_applied, remove_steam_hosts};
use crate::logger::{get_all_logs, log_info, log_error};
use crate::proxy::{
    get_proxy_stats, is_proxy_running, run_steam_connectivity_test, start_proxy_engine,
    stop_proxy_engine,
};
use crate::service::{
    install_and_start_service, query_win_service_state, uninstall_service, WinServiceState,
};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

#[derive(Serialize)]
pub struct StatusDto {
    pub proxy_running: bool,
    pub service_state: String,
    pub active_connections: usize,
    pub total_connections: u64,
    pub traffic_bytes: u64,
    pub doh_cached: usize,
    pub hosts_active: bool,
}

#[tauri::command]
pub fn get_status() -> StatusDto {
    let svc = query_win_service_state();
    let stats = get_proxy_stats();
    let svc_str = match svc {
        WinServiceState::Running => "running",
        WinServiceState::Stopped => "stopped",
        _ => "not_installed",
    };

    StatusDto {
        proxy_running: stats.running,
        service_state: svc_str.to_string(),
        active_connections: stats.active_connections,
        total_connections: stats.total_connections,
        traffic_bytes: stats.total_bytes,
        doh_cached: cached_domain_count(),
        hosts_active: is_hosts_applied(),
    }
}

#[tauri::command]
pub fn toggle_proxy() -> Result<(), String> {
    let svc = query_win_service_state();
    if svc == WinServiceState::Running {
        log_info("[DỊCH VỤ]   Proxy đang chạy dưới dạng Windows Service nền.");
        return Ok(());
    }

    if is_proxy_running() {
        stop_proxy_engine();
    } else {
        start_proxy_engine()?;
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_service() -> Result<(), String> {
    let svc = query_win_service_state();
    match svc {
        WinServiceState::NotInstalled => {
            if is_proxy_running() {
                stop_proxy_engine();
            }
            if let Err(e) = install_and_start_service() {
                log_error(format!("[DỊCH VỤ]   Lỗi cài đặt: {}", e));
                let _ = start_proxy_engine();
                return Err(e);
            }
        }
        _ => {
            if let Err(e) = uninstall_service() {
                log_error(format!("[DỊCH VỤ]   Lỗi gỡ dịch vụ: {}", e));
                return Err(e);
            }
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub struct PingResultDto {
    pub success: bool,
    pub ping_ms: u64,
    pub error: String,
}

#[tauri::command]
pub async fn test_connection() -> PingResultDto {
    match crate::proxy::run_steam_ping_test().await {
        Ok(ms) => PingResultDto {
            success: true,
            ping_ms: ms,
            error: String::new(),
        },
        Err(e) => PingResultDto {
            success: false,
            ping_ms: 0,
            error: e,
        },
    }
}

#[tauri::command]
pub fn flush_dns() {
    clear_doh_cache();
    let _ = flush_dns_cache();
}

#[tauri::command]
pub fn get_logs() -> Vec<String> {
    let text = get_all_logs();
    text.lines().map(|s| s.to_string()).collect()
}

#[tauri::command]
pub fn clear_all_logs() {
    crate::logger::clear_logs();
}

#[tauri::command]
pub fn minimize_window(app: AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.minimize();
    }
}

#[tauri::command]
pub fn minimize_to_tray(app: AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
}

#[tauri::command]
pub fn exit_app(app: AppHandle) {
    perform_clean_teardown_on_exit();
    app.exit(0);
}

fn perform_clean_teardown_on_exit() {
    let svc = query_win_service_state();
    if svc != WinServiceState::Running {
        stop_proxy_engine();
        let _ = remove_steam_hosts();
    }
}

pub fn run_gui_app() -> Result<(), String> {
    log_info("[HỆ THỐNG]  Khởi động giao diện Steam Unblock Network...");

    // Tự động bật Proxy Plug & Play
    let svc_state = query_win_service_state();
    match svc_state {
        WinServiceState::Running => {
            log_info("[DỊCH VỤ]   Dịch vụ Windows đang chạy ngầm.");
            run_steam_connectivity_test();
        }
        _ => match start_proxy_engine() {
            Ok(()) => {
                log_info("[HỆ THỐNG]  ⚡ Đã kích hoạt bảo vệ Steam.");
                run_steam_connectivity_test();
            }
            Err(e) => {
                log_error(format!("[HỆ THỐNG]  Lỗi khởi chạy: {}", e));
            }
        },
    }

    tauri::Builder::default()
        .setup(|app| {
            // Thiết lập Tray Icon hệ thống
            let show_i = MenuItem::with_id(app, "show", "Mở Steam Unblock", true, None::<&str>)?;
            let toggle_i = MenuItem::with_id(app, "toggle", "Bật / Tắt Proxy", true, None::<&str>)?;
            let test_i = MenuItem::with_id(app, "test", "Kiểm tra kết nối", true, None::<&str>)?;
            let flush_i = MenuItem::with_id(app, "flush", "Làm mới DNS", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Thoát ứng dụng", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &toggle_i, &test_i, &flush_i, &quit_i])?;

            let mut builder = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Steam Unblock Network")
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "show" => {
                            if let Some(win) = app.get_webview_window("main") {
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                        }
                        "toggle" => {
                            let _ = toggle_proxy();
                        }
                        "test" => {
                            crate::proxy::run_steam_connectivity_test();
                        }
                        "flush" => {
                            flush_dns();
                        }
                        "quit" => {
                            perform_clean_teardown_on_exit();
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                });

            if let Some(icon) = app.default_window_icon() {
                builder = builder.icon(icon.clone());
            }

            builder.build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Khi bấm [X], thu nhỏ xuống Tray để duy trì mở khóa Steam
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            toggle_proxy,
            toggle_service,
            test_connection,
            flush_dns,
            get_logs,
            clear_all_logs,
            minimize_window,
            minimize_to_tray,
            exit_app,
        ])
        .run(tauri::generate_context!())
        .map_err(|e| format!("Lỗi khởi chạy Tauri UI: {}", e))
}
