use crate::hosts::remove_steam_hosts;
use crate::logger::{enable_file_logging, log_error, log_info};
use crate::proxy::{start_proxy_engine, stop_proxy_engine};
use std::ffi::OsString;
use std::sync::mpsc;
use std::time::Duration;
use windows_service::define_windows_service;
use windows_service::service::{
    ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
    ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

pub const SERVICE_NAME: &str = "SteamUnblockService";
pub const SERVICE_DISPLAY_NAME: &str = "Steam Unblock Network (DoH & SNI Proxy)";
pub const SERVICE_DESCRIPTION: &str =
    "Định tuyến lưu lượng Steam qua DoH và SNI Proxy (TLS Fragmentation) giúp mở khóa Steam Store, Cộng đồng, Kho hàng, Hồ sơ & Avatar trên các nhà mạng tại Việt Nam mà không ảnh hưởng Ping khi chơi Steam Games.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinServiceState {
    NotInstalled,
    Stopped,
    Running,
    Pending,
}

pub fn query_win_service_state() -> WinServiceState {
    let manager = match ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
    {
        Ok(m) => m,
        Err(_) => return WinServiceState::NotInstalled,
    };

    let service = match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
        Ok(s) => s,
        Err(_) => return WinServiceState::NotInstalled,
    };

    match service.query_status() {
        Ok(status) => match status.current_state {
            ServiceState::Running => WinServiceState::Running,
            ServiceState::Stopped => WinServiceState::Stopped,
            _ => WinServiceState::Pending,
        },
        Err(_) => WinServiceState::NotInstalled,
    }
}

pub fn install_and_start_service() -> Result<(), String> {
    let exe_path = std::env::current_exe()
        .map_err(|e| format!("Không lấy được đường dẫn file thực thi: {}", e))?;

    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .map_err(|e| format!("Không thể mở Windows Service Manager (Cần quyền Admin): {}", e))?;

    let service_info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe_path,
        launch_arguments: vec![OsString::from("--service")],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };

    let service = match manager.open_service(
        SERVICE_NAME,
        ServiceAccess::QUERY_STATUS | ServiceAccess::START | ServiceAccess::CHANGE_CONFIG,
    ) {
        Ok(existing) => {
            let _ = existing.change_config(&service_info);
            existing
        }
        Err(_) => manager
            .create_service(
                &service_info,
                ServiceAccess::CHANGE_CONFIG
                    | ServiceAccess::QUERY_STATUS
                    | ServiceAccess::START,
            )
            .map_err(|e| format!("Không thể tạo Windows Service: {}", e))?,
    };

    let _ = service.set_description(SERVICE_DESCRIPTION);

    if let Ok(status) = service.query_status() {
        if status.current_state != ServiceState::Running {
            service
                .start(&[] as &[&std::ffi::OsStr])
                .map_err(|e| format!("Không thể khởi chạy Windows Service: {}", e))?;
        }
    }

    log_info("[DỊCH VỤ]   Đã cài đặt và khởi chạy Dịch vụ Windows nền (Chế độ: Automatic khi bật máy).");
    Ok(())
}

pub fn uninstall_service() -> Result<(), String> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| format!("Không thể mở Windows Service Manager: {}", e))?;

    if let Ok(service) = manager.open_service(
        SERVICE_NAME,
        ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
    ) {
        if let Ok(status) = service.query_status() {
            if status.current_state != ServiceState::Stopped {
                let _ = service.stop();
                for _ in 0..25 {
                    std::thread::sleep(Duration::from_millis(200));
                    if let Ok(st) = service.query_status() {
                        if st.current_state == ServiceState::Stopped {
                            break;
                        }
                    }
                }
            }
        }

        service
            .delete()
            .map_err(|e| format!("Không thể xóa Windows Service: {}", e))?;
    }

    let _ = remove_steam_hosts();
    log_info("[DỊCH VỤ]   Đã gỡ cài đặt Dịch vụ Windows và khôi phục hoàn toàn mạng gốc.");
    Ok(())
}

define_windows_service!(ffi_service_main, service_main);

pub fn run_service_entry() -> Result<(), String> {
    enable_file_logging(true);
    windows_service::service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .map_err(|e| format!("Lỗi khởi chạy Service Dispatcher: {}", e))
}

fn service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service_worker() {
        log_error(format!("[DỊCH VỤ]   Lỗi nghiêm trọng trong Windows Service: {}", e));
    }
}

fn run_service_worker() -> Result<(), String> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop | ServiceControl::Shutdown | ServiceControl::Preshutdown => {
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)
        .map_err(|e| format!("Không thể đăng ký Service Control Handler: {}", e))?;

    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 1,
        wait_hint: Duration::from_secs(5),
        process_id: None,
    });

    log_info("[DỊCH VỤ]   Windows Service SteamUnblockService đang khởi chạy...");
    if let Err(e) = start_proxy_engine() {
        log_error(format!("[DỊCH VỤ]   Không thể khởi chạy Proxy Engine: {}", e));
        let _ = status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(1),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        });
        return Err(e);
    }

    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP
            | ServiceControlAccept::SHUTDOWN
            | ServiceControlAccept::PRESHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    });

    let _ = shutdown_rx.recv();

    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StopPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 1,
        wait_hint: Duration::from_secs(5),
        process_id: None,
    });

    stop_proxy_engine();

    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    });

    Ok(())
}
