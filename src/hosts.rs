use crate::domains::STEAM_CM_DIRECT_HOSTS;
use crate::logger::{log_error, log_info, log_warn};
use parking_lot::Mutex;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

const HOSTS_PATH: &str = r"C:\Windows\System32\drivers\etc\hosts";
const MARKER_START: &str = "# >>> STEAM-UNBLOCK-NETWORK START >>>";
const MARKER_END: &str = "# <<< STEAM-UNBLOCK-NETWORK END <<<";
const CREATE_NO_WINDOW: u32 = 0x08000000;

static HOSTS_LOCK: Mutex<()> = Mutex::new(());

fn hosts_file_path() -> PathBuf {
    PathBuf::from(HOSTS_PATH)
}

pub fn is_hosts_applied() -> bool {
    if let Ok(content) = fs::read_to_string(hosts_file_path()) {
        content.contains(MARKER_START) && content.contains(MARKER_END)
    } else {
        false
    }
}

fn strip_managed_block(content: &str) -> (String, bool) {
    let mut output_lines = Vec::new();
    let mut inside_block = false;
    let mut removed = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == MARKER_START {
            inside_block = true;
            removed = true;
            continue;
        }
        if trimmed == MARKER_END {
            inside_block = false;
            continue;
        }
        if !inside_block {
            output_lines.push(line);
        }
    }

    while output_lines
        .last()
        .map(|l| l.trim().is_empty())
        .unwrap_or(false)
    {
        output_lines.pop();
    }

    let mut result = output_lines.join("\r\n");
    if !result.is_empty() {
        result.push_str("\r\n");
    }
    (result, removed)
}

/// Ghi cấu hình định tuyến Plug & Play cho các tên miền Steam (SNI Proxy + CM Servers) vào file hosts
pub fn apply_steam_hosts(domains: &[String]) -> Result<(), String> {
    let _guard = HOSTS_LOCK.lock();
    let path = hosts_file_path();

    let original = fs::read_to_string(&path).map_err(|e| {
        format!(
            "Không thể đọc file hosts ({}): {}. Hãy chạy với quyền Administrator!",
            HOSTS_PATH, e
        )
    })?;

    if !original.contains(MARKER_START) {
        let backup_path = path.with_extension("steam_unblock.bak");
        if !backup_path.exists() {
            let _ = fs::write(&backup_path, &original);
        }
    }

    let (mut clean_base, _) = strip_managed_block(&original);
    if !clean_base.is_empty() && !clean_base.ends_with("\r\n\r\n") {
        clean_base.push_str("\r\n");
    }

    let total_entries = domains.len() + STEAM_CM_DIRECT_HOSTS.len();
    let mut block = String::with_capacity(total_entries * 48 + 256);
    block.push_str(MARKER_START);
    block.push_str("\r\n");
    block.push_str("# Quản lý tự động bởi Steam Unblock Network - Tự khôi phục khi đóng ứng dụng\r\n");

    // 1. Định tuyến qua Local SNI Proxy (127.0.0.1)
    for domain in domains {
        block.push_str(&format!("127.0.0.1 {}\r\n", domain));
    }

    // 2. Định tuyến trực tiếp IP thật cho các máy chủ Steam CM WebSocket (Hồ sơ, Username, Bạn bè, Avatar)
    for (ip, cm_host) in STEAM_CM_DIRECT_HOSTS {
        block.push_str(&format!("{} {}\r\n", ip, cm_host));
    }

    block.push_str(MARKER_END);
    block.push_str("\r\n");

    let new_content = format!("{}{}", clean_base, block);

    if let Ok(metadata) = fs::metadata(&path) {
        let mut perms = metadata.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            let _ = fs::set_permissions(&path, perms);
        }
    }

    fs::write(&path, new_content).map_err(|e| {
        format!(
            "Không thể ghi vào file hosts: {}. Vui lòng kiểm tra quyền Administrator.",
            e
        )
    })?;

    log_info(format!(
        "[HỆ THỐNG]  Đã cấu hình {} tên miền Steam trong hosts.",
        total_entries
    ));

    if let Err(e) = flush_dns_cache() {
        log_warn(format!("[HỆ THỐNG]  Lỗi làm mới DNS: {}", e));
    }

    Ok(())
}

/// Clean Teardown: Khôi phục hoàn toàn file hosts hệ thống về nguyên bản và xóa DNS Cache
pub fn remove_steam_hosts() -> Result<bool, String> {
    let _guard = HOSTS_LOCK.lock();
    let path = hosts_file_path();

    let current = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            return Err(format!("Không thể đọc file hosts: {}", e));
        }
    };

    let (cleaned, removed) = strip_managed_block(&current);
    if removed {
        fs::write(&path, cleaned)
            .map_err(|e| format!("Không thể khôi phục file hosts: {}", e))?;
        log_info("[TEARDOWN]  Đã khôi phục file hosts gốc.");
        let _ = flush_dns_cache();
    }

    Ok(removed)
}

pub fn flush_dns_cache() -> Result<(), String> {
    let output = Command::new("ipconfig")
        .arg("/flushdns")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Lỗi ipconfig /flushdns: {}", e))?;

    if output.status.success() {
        log_info("[HỆ THỐNG]  Đã xóa DNS Cache.");
        Ok(())
    } else {
        let err_msg = String::from_utf8_lossy(&output.stderr).to_string();
        log_error(format!("[HỆ THỐNG]  Lỗi xóa DNS: {}", err_msg));
        Err(err_msg)
    }
}
