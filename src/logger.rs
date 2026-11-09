use chrono::Local;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_LOG_LINES: usize = 50;

#[derive(Clone, Debug)]
pub struct TableLogRow {
    pub title: String,
    pub time: String,
    pub detail: String,
    pub status: String,
    pub is_ok: bool,
}

static LOG_BUFFER: Lazy<Mutex<VecDeque<String>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES)));
static TABLE_ROWS: Lazy<Mutex<VecDeque<TableLogRow>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(12)));
static LOG_DIRTY: AtomicBool = AtomicBool::new(true);
static FILE_LOGGING_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn enable_file_logging(enable: bool) {
    FILE_LOGGING_ENABLED.store(enable, Ordering::Relaxed);
}

pub fn push_table_row(
    title: impl Into<String>,
    detail: impl Into<String>,
    status: impl Into<String>,
    is_ok: bool,
) {
    let time = Local::now().format("%H:%M:%S").to_string();
    let title_s = title.into();
    let detail_s = detail.into();
    let status_s = status.into();

    {
        let mut rows = TABLE_ROWS.lock();
        if let Some(last) = rows.back_mut() {
            if last.title == title_s {
                last.time = time;
                last.detail = detail_s;
                last.status = status_s;
                last.is_ok = is_ok;
                LOG_DIRTY.store(true, Ordering::Release);
                return;
            }
        }
        if rows.len() >= 5 {
            rows.pop_front();
        }
        rows.push_back(TableLogRow {
            title: title_s,
            time,
            detail: detail_s,
            status: status_s,
            is_ok,
        });
    }
    LOG_DIRTY.store(true, Ordering::Release);
}

pub fn get_table_rows() -> Vec<TableLogRow> {
    TABLE_ROWS.lock().iter().cloned().collect()
}

pub fn log_info(msg: impl AsRef<str>) {
    let s = msg.as_ref();
    append_log("OK", s, true);
}

pub fn log_warn(msg: impl AsRef<str>) {
    let s = msg.as_ref();
    append_log("WARN", s, true);
}

pub fn log_error(msg: impl AsRef<str>) {
    let s = msg.as_ref();
    append_log("ERR", s, false);
}

fn append_log(level: &str, msg: &str, is_ok: bool) {
    let timestamp = Local::now().format("%H:%M:%S").to_string();
    let line = format!("[{}] [{}] {}", timestamp, level, msg);

    {
        let mut buf = LOG_BUFFER.lock();
        while buf.len() >= MAX_LOG_LINES {
            buf.pop_front();
        }
        buf.push_back(line.clone());
    }

    let (title, detail, status) = parse_for_table(msg, is_ok);
    push_table_row(title, detail, status, is_ok);

    LOG_DIRTY.store(true, Ordering::Release);

    if FILE_LOGGING_ENABLED.load(Ordering::Relaxed) {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(dir) = exe_path.parent() {
                let log_path = dir.join("steam-unblock.log");
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
                    let _ = writeln!(file, "{}", line);
                }
            }
        }
    }
}

fn parse_for_table(msg: &str, is_ok: bool) -> (String, String, String) {
    if let Some(idx) = msg.find('➔') {
        let left = msg[..idx]
            .replace("[DoH DNS]", "")
            .replace("[SNI PROXY]", "")
            .replace("[KIỂM TRA]", "")
            .trim()
            .to_string();
        let right = msg[idx + '➔'.len_utf8()..]
            .replace("OK (", "")
            .replace(')', "")
            .trim()
            .to_string();
        let status = if is_ok { "+ Online" } else { "- Error" };
        return (left, right, status.to_string());
    }

    let clean = msg
        .replace("[HỆ THỐNG]", "")
        .replace("[SNI PROXY]", "")
        .replace("[KIỂM TRA]", "")
        .replace("[DỊCH VỤ]", "")
        .replace("[TEARDOWN]", "")
        .replace("[DoH DNS]", "")
        .trim()
        .to_string();

    let short_title = if clean.chars().count() > 24 {
        let s: String = clean.chars().take(24).collect();
        format!("{}…", s)
    } else {
        clean
    };

    let status = if is_ok { "+ Active" } else { "- Failed" };
    (short_title, "127.0.0.1".to_string(), status.to_string())
}

pub fn clear_logs() {
    LOG_BUFFER.lock().clear();
    TABLE_ROWS.lock().clear();
    LOG_DIRTY.store(true, Ordering::Release);
}

pub fn check_and_clear_dirty() -> bool {
    LOG_DIRTY.swap(false, Ordering::AcqRel)
}

pub fn get_all_logs() -> String {
    LOG_DIRTY.store(false, Ordering::Release);
    let buf = LOG_BUFFER.lock();
    let mut out = String::with_capacity(buf.len() * 64);
    for line in buf.iter() {
        out.push_str(line);
        out.push_str("\r\n");
    }
    out
}

pub fn take_logs_if_dirty() -> Option<String> {
    if !LOG_DIRTY.swap(false, Ordering::AcqRel) {
        return None;
    }
    let buf = LOG_BUFFER.lock();
    let mut out = String::with_capacity(buf.len() * 64);
    for line in buf.iter() {
        out.push_str(line);
        out.push_str("\r\n");
    }
    Some(out)
}
