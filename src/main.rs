#![windows_subsystem = "windows"]

pub mod doh;
pub mod domains;
pub mod gui;
pub mod hosts;
pub mod logger;
pub mod proxy;
pub mod service;
pub mod tls;

use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
};
use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Kiểm tra xem tiến trình hiện tại đã có quyền Administrator hay chưa
fn is_running_as_admin() -> bool {
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut ret_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// Tự động yêu cầu quyền Administrator (UAC) nếu người dùng mở trực tiếp mà chưa chạy quyền Admin
fn relaunch_as_admin(args: &[String]) {
    if let Ok(exe_path) = std::env::current_exe() {
        let verb = to_wide("runas");
        let file = to_wide(&exe_path.to_string_lossy());
        let params_str = args.join(" ");
        let params = to_wide(&params_str);

        unsafe {
            ShellExecuteW(
                null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                params.as_ptr(),
                null(),
                SW_SHOWNORMAL,
            );
        }
    }
}

fn attach_parent_console() {
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn cli_print(msg: &str) {
    use std::io::Write;
    if let Ok(mut con) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
        let _ = writeln!(con, "{}", msg);
    } else {
        println!("{}", msg);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // 1. Chế độ chạy ngầm Windows Service (được gọi tự động bởi Windows SCM khi khởi động máy)
    if args.iter().any(|a| a == "--service") {
        let _ = service::run_service_entry();
        return;
    }

    // 2. Chế độ kiểm tra nhanh kết nối DoH + SNI Fragmentation qua CLI (không cần quyền Admin)
    if args.iter().any(|a| a == "--test-doh") {
        attach_parent_console();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            cli_print("\r\n=== KIỂM TRA DoH & TLS SNI FRAGMENTATION CHO STEAM ===");
            for domain in [
                "store.steampowered.com",
                "steamcommunity.com",
                "api.steampowered.com",
                "shared.akamai.steamstatic.com",
            ] {
                match proxy::test_single_domain_tls_handshake(domain).await {
                    Ok(ip) => cli_print(&format!(
                        "[OK] {} -> DoH IP: {} | TLS Fragmented Handshake ServerHello thành công!",
                        domain, ip
                    )),
                    Err(e) => cli_print(&format!("[ERR] {} -> {}", domain, e)),
                }
            }
        });
        return;
    }

    // 3. Các tác vụ quản trị hệ thống (hosts, cổng 443, Windows Service) yêu cầu quyền Administrator
    if !is_running_as_admin() {
        relaunch_as_admin(&args);
        return;
    }

    // 4. Các cờ dòng lệnh hỗ trợ cài đặt / gỡ cài đặt / dọn dẹp nhanh
    if args.iter().any(|a| a == "--install-service") {
        attach_parent_console();
        match service::install_and_start_service() {
            Ok(()) => cli_print("Đã cài đặt và khởi chạy Dịch vụ Windows (Automatic) thành công."),
            Err(e) => cli_print(&format!("Lỗi cài đặt dịch vụ: {}", e)),
        }
        return;
    }

    if args.iter().any(|a| a == "--uninstall-service") {
        attach_parent_console();
        match service::uninstall_service() {
            Ok(()) => cli_print("Đã gỡ cài đặt Dịch vụ Windows và khôi phục mạng gốc thành công."),
            Err(e) => cli_print(&format!("Lỗi gỡ dịch vụ: {}", e)),
        }
        return;
    }

    if args.iter().any(|a| a == "--cleanup") {
        attach_parent_console();
        let _ = hosts::remove_steam_hosts();
        let _ = hosts::flush_dns_cache();
        cli_print("Clean Teardown: Đã khôi phục file hosts và xóa DNS cache.");
        return;
    }

    // 5. Mặc định: Khởi chạy Giao diện người dùng (GUI) + Khay hệ thống (System Tray) + Plug & Play
    let _ = gui::run_gui_app();
}
