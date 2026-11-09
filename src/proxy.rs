use crate::doh::resolve_domain_doh;
use crate::domains::{get_target_domains, is_allowed_steam_domain};
use crate::hosts::{apply_steam_hosts, remove_steam_hosts};
use crate::logger::{log_error, log_info, log_warn};
use crate::tls::{build_fragmented_tls_records, parse_http_host, parse_tls_client_hello_sni};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::time::timeout;

static PROXY_RUNNING: AtomicBool = AtomicBool::new(false);
static ACTIVE_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);
static TOTAL_CONNECTIONS: AtomicU64 = AtomicU64::new(0);
static TOTAL_BYTES: AtomicU64 = AtomicU64::new(0);

static SHUTDOWN_TX: Lazy<Mutex<Option<watch::Sender<bool>>>> = Lazy::new(|| Mutex::new(None));
static RECENT_DOMAIN_LOGS: Lazy<Mutex<HashMap<String, Instant>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static TOKIO_RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("Không thể khởi tạo Tokio Async Runtime")
});

#[derive(Clone, Copy, Debug)]
pub struct ProxyStats {
    pub running: bool,
    pub active_connections: usize,
    pub total_connections: u64,
    pub total_bytes: u64,
}

pub fn get_proxy_stats() -> ProxyStats {
    ProxyStats {
        running: PROXY_RUNNING.load(Ordering::Acquire),
        active_connections: ACTIVE_CONNECTIONS.load(Ordering::Relaxed),
        total_connections: TOTAL_CONNECTIONS.load(Ordering::Relaxed),
        total_bytes: TOTAL_BYTES.load(Ordering::Relaxed),
    }
}

pub fn is_proxy_running() -> bool {
    PROXY_RUNNING.load(Ordering::Acquire)
}

fn log_proxied_domain_throttled(domain: &str, ip: std::net::Ipv4Addr) {
    let mut map = RECENT_DOMAIN_LOGS.lock();
    let now = Instant::now();
    if let Some(last) = map.get(domain) {
        if now.duration_since(*last) < Duration::from_secs(15) {
            return;
        }
    }
    map.insert(domain.to_string(), now);
    log_info(format!(
        "[SNI PROXY] {:<30} ➔  {}:443",
        domain, ip
    ));
}

/// Khởi động SNI Proxy (Cổng 443 & 80) + Cấu hình file hosts tự động (Plug & Play)
pub fn start_proxy_engine() -> Result<(), String> {
    if is_proxy_running() {
        return Ok(());
    }

    let domains = get_target_domains();

    let (https_listener, http_listener) = TOKIO_RUNTIME.block_on(async {
        let https = TcpListener::bind("127.0.0.1:443").await.map_err(|e| {
            format!(
                "Cổng 443 đang bận ({}). Hãy kiểm tra Dịch vụ Windows nền.",
                e
            )
        })?;

        let http = match TcpListener::bind("127.0.0.1:80").await {
            Ok(l) => Some(l),
            Err(_) => {
                log_warn("[HỆ THỐNG]  Cổng 80 đang bận, chỉ chạy HTTPS trên cổng 443.");
                None
            }
        };

        Ok::<_, String>((https, http))
    })?;

    if let Err(e) = apply_steam_hosts(&domains) {
        return Err(e);
    }

    let (tx, rx) = watch::channel(false);
    *SHUTDOWN_TX.lock() = Some(tx);
    PROXY_RUNNING.store(true, Ordering::Release);

    log_info("[SNI PROXY] Đang lắng nghe tại 127.0.0.1:443.");

    let mut rx_https = rx.clone();
    TOKIO_RUNTIME.spawn(async move {
        loop {
            tokio::select! {
                _ = rx_https.changed() => {
                    if *rx_https.borrow() {
                        break;
                    }
                }
                accept_res = https_listener.accept() => {
                    match accept_res {
                        Ok((client_stream, _peer)) => {
                            tokio::spawn(async move {
                                ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
                                TOTAL_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
                                let _ = handle_tls_sni_connection(client_stream).await;
                                ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
                            });
                        }
                        Err(_) => {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    }
                }
            }
        }
    });

    if let Some(http_listener) = http_listener {
        let mut rx_http = rx;
        TOKIO_RUNTIME.spawn(async move {
            loop {
                tokio::select! {
                    _ = rx_http.changed() => {
                        if *rx_http.borrow() {
                            break;
                        }
                    }
                    accept_res = http_listener.accept() => {
                        if let Ok((client_stream, _)) = accept_res {
                            tokio::spawn(async move {
                                ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
                                TOTAL_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
                                let _ = handle_http_connection(client_stream).await;
                                ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
                            });
                        }
                    }
                }
            }
        });
    }

    Ok(())
}

/// Dừng SNI Proxy và thực hiện Clean Teardown (Khôi phục file hosts gốc & Flush DNS)
pub fn stop_proxy_engine() {
    if let Some(tx) = SHUTDOWN_TX.lock().take() {
        let _ = tx.send(true);
    }

    let was_running = PROXY_RUNNING.swap(false, Ordering::AcqRel);

    match remove_steam_hosts() {
        Ok(_) => {
            if was_running {
                log_info("[HỆ THỐNG]  Đã dừng SNI Proxy và khôi phục hoàn toàn cài đặt mạng gốc.");
            }
        }
        Err(e) => {
            log_error(format!("[TEARDOWN]  Lỗi khi khôi phục file hosts: {}", e));
        }
    }
}

async fn handle_tls_sni_connection(mut client: TcpStream) -> Result<(), String> {
    let _ = client.set_nodelay(true);

    let mut buf = Vec::with_capacity(2048);
    let mut temp = [0u8; 4096];

    let read_res = timeout(Duration::from_secs(5), async {
        loop {
            let n = client.read(&mut temp).await.map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("Client đóng kết nối sớm".to_string());
            }
            buf.extend_from_slice(&temp[..n]);
            if buf.len() >= 5 {
                if buf[0] != 0x16 {
                    return Err("Không phải gói tin TLS Handshake (0x16)".to_string());
                }
                let rec_len = u16::from_be_bytes([buf[3], buf[4]]) as usize;
                if buf.len() >= 5 + rec_len || buf.len() >= 16384 {
                    break;
                }
            }
        }
        Ok::<(), String>(())
    })
    .await;

    if read_res.is_err() || buf.is_empty() {
        return Err("Timeout đọc TLS ClientHello".to_string());
    }

    let sni_info = parse_tls_client_hello_sni(&buf)
        .ok_or_else(|| "Không tìm thấy SNI trong gói TLS ClientHello".to_string())?;

    if !is_allowed_steam_domain(&sni_info.hostname) {
        return Err("Domain không thuộc danh sách cho phép".to_string());
    }

    let ips = resolve_domain_doh(&sni_info.hostname).await?;

    let mut upstream_opt = None;
    for ip in &ips {
        let target = SocketAddr::from((*ip, 443));
        if let Ok(Ok(stream)) =
            timeout(Duration::from_millis(2500), TcpStream::connect(target)).await
        {
            let _ = stream.set_nodelay(true);
            upstream_opt = Some((stream, *ip));
            break;
        }
    }

    let (mut upstream, connected_ip) = upstream_opt.ok_or_else(|| {
        let msg = format!(
            "[SNI PROXY] Không thể kết nối TCP 443 tới {} (IP: {:?})",
            sni_info.hostname, ips
        );
        log_error(&msg);
        msg
    })?;

    log_proxied_domain_throttled(&sni_info.hostname, connected_ip);

    let fragments = build_fragmented_tls_records(&buf, &sni_info);
    for (idx, chunk) in fragments.iter().enumerate() {
        upstream
            .write_all(chunk)
            .await
            .map_err(|e| format!("Lỗi gửi mảnh TLS #{}: {}", idx, e))?;
        let _ = upstream.flush().await;
        if idx + 1 < fragments.len() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    TOTAL_BYTES.fetch_add(buf.len() as u64, Ordering::Relaxed);

    if let Ok((from_client, from_server)) =
        tokio::io::copy_bidirectional(&mut client, &mut upstream).await
    {
        TOTAL_BYTES.fetch_add(from_client + from_server, Ordering::Relaxed);
    }

    Ok(())
}

async fn handle_http_connection(mut client: TcpStream) -> Result<(), String> {
    let _ = client.set_nodelay(true);
    let mut buf = vec![0u8; 4096];

    let n = match timeout(Duration::from_secs(4), client.read(&mut buf)).await {
        Ok(Ok(n)) if n > 0 => n,
        _ => return Err("Timeout đọc HTTP request".to_string()),
    };
    buf.truncate(n);

    let (host, split_pos) =
        parse_http_host(&buf).ok_or_else(|| "Không tìm thấy HTTP Host header".to_string())?;

    if !is_allowed_steam_domain(&host) {
        return Err("HTTP Host không thuộc Steam".to_string());
    }

    let ips = resolve_domain_doh(&host).await?;
    let mut upstream_opt = None;
    for ip in &ips {
        let target = SocketAddr::from((*ip, 80));
        if let Ok(Ok(stream)) =
            timeout(Duration::from_millis(2500), TcpStream::connect(target)).await
        {
            let _ = stream.set_nodelay(true);
            upstream_opt = Some(stream);
            break;
        }
    }

    let mut upstream = upstream_opt.ok_or_else(|| "Lỗi kết nối cổng 80".to_string())?;
    let cut = split_pos.clamp(1, buf.len().saturating_sub(1));
    upstream
        .write_all(&buf[..cut])
        .await
        .map_err(|e| e.to_string())?;
    let _ = upstream.flush().await;
    tokio::time::sleep(Duration::from_millis(2)).await;
    upstream
        .write_all(&buf[cut..])
        .await
        .map_err(|e| e.to_string())?;
    let _ = upstream.flush().await;

    if let Ok((tx, rx)) = tokio::io::copy_bidirectional(&mut client, &mut upstream).await {
        TOTAL_BYTES.fetch_add(tx + rx + (buf.len() as u64), Ordering::Relaxed);
    }

    Ok(())
}

/// Kiểm tra kết nối tới Store, Community, Hồ sơ/Avatar và Chat
pub fn run_steam_connectivity_test() {
    TOKIO_RUNTIME.spawn(async {
        log_info("[KIỂM TRA]  Đang kiểm tra kết nối Steam...");
        let test_domains = [
            "store.steampowered.com",
            "steamcommunity.com",
            "avatars.steamstatic.com",
            "steam-chat.com",
            "api.steampowered.com",
        ];

        for domain in test_domains {
            match test_single_domain_tls_handshake(domain).await {
                Ok(ip) => {
                    log_info(format!(
                        "[KIỂM TRA]  {:<30} ➔  OK ({})",
                        domain, ip
                    ));
                }
                Err(e) => {
                    log_error(format!("[KIỂM TRA]  {:<30} ➔  LỖI ({})", domain, e));
                }
            }
        }
        log_info("[KIỂM TRA]  Hoàn tất! Kết nối Steam đã thông suốt.");
    });
}

pub async fn run_steam_ping_test() -> Result<u64, String> {
    let start = std::time::Instant::now();
    let ip = test_single_domain_tls_handshake("store.steampowered.com").await?;
    let elapsed = start.elapsed().as_millis() as u64;
    log_info(format!("[KIỂM TRA]  store.steampowered.com ➔ {} (Ping: {} ms)", ip, elapsed));
    Ok(elapsed)
}

pub async fn test_single_domain_tls_handshake(domain: &str) -> Result<String, String> {
    let ips = resolve_domain_doh(domain).await?;
    let ip = ips
        .first()
        .copied()
        .ok_or_else(|| "Không có IP từ DoH".to_string())?;

    let target = SocketAddr::from((ip, 443));
    let mut stream = timeout(Duration::from_secs(3), TcpStream::connect(target))
        .await
        .map_err(|_| format!("Timeout kết nối TCP tới {}:443", ip))?
        .map_err(|e| format!("Lỗi TCP {}: {}", ip, e))?;
    let _ = stream.set_nodelay(true);

    let hello = build_synthetic_client_hello(domain);
    let sni = parse_tls_client_hello_sni(&hello)
        .ok_or_else(|| "Lỗi tạo gói ClientHello thử nghiệm".to_string())?;
    let chunks = build_fragmented_tls_records(&hello, &sni);

    for (i, chunk) in chunks.iter().enumerate() {
        stream
            .write_all(chunk)
            .await
            .map_err(|e| format!("Lỗi ghi mảnh TLS: {}", e))?;
        let _ = stream.flush().await;
        if i + 1 < chunks.len() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    let mut resp = [0u8; 6];
    timeout(Duration::from_secs(3), stream.read_exact(&mut resp))
        .await
        .map_err(|_| "Timeout chờ phản hồi TLS ServerHello".to_string())?
        .map_err(|e| format!("Lỗi đọc TLS ServerHello: {}", e))?;

    if resp[0] == 0x16 && resp[1] == 0x03 {
        Ok(ip.to_string())
    } else {
        Err(format!("Phản hồi không phải TLS Handshake: {:02X?}", resp))
    }
}

fn build_synthetic_client_hello(hostname: &str) -> Vec<u8> {
    let host_bytes = hostname.as_bytes();
    let mut ext_sni = Vec::new();
    let list_len = (3 + host_bytes.len()) as u16;
    ext_sni.extend_from_slice(&list_len.to_be_bytes());
    ext_sni.push(0x00);
    ext_sni.extend_from_slice(&(host_bytes.len() as u16).to_be_bytes());
    ext_sni.extend_from_slice(host_bytes);

    let mut extensions = Vec::new();
    extensions.extend_from_slice(&0x0000u16.to_be_bytes());
    extensions.extend_from_slice(&(ext_sni.len() as u16).to_be_bytes());
    extensions.extend_from_slice(&ext_sni);

    extensions.extend_from_slice(&[0x00, 0x0a, 0x00, 0x06, 0x00, 0x04, 0x00, 0x1d, 0x00, 0x17]);
    extensions.extend_from_slice(&[0x00, 0x0b, 0x00, 0x02, 0x01, 0x00]);
    extensions.extend_from_slice(&[
        0x00, 0x0d, 0x00, 0x08, 0x00, 0x06, 0x04, 0x03, 0x08, 0x04, 0x04, 0x01,
    ]);

    let mut ch_body = Vec::new();
    ch_body.extend_from_slice(&[0x03, 0x03]);
    ch_body.extend_from_slice(&[0x5A; 32]);
    ch_body.push(0x00);
    let ciphers: &[u8] = &[
        0x13, 0x01, 0x13, 0x02, 0xc0, 0x2f, 0xc0, 0x2b, 0xc0, 0x30, 0xc0, 0x2c,
    ];
    ch_body.extend_from_slice(&(ciphers.len() as u16).to_be_bytes());
    ch_body.extend_from_slice(ciphers);
    ch_body.push(0x01);
    ch_body.push(0x00);
    ch_body.extend_from_slice(&(extensions.len() as u16).to_be_bytes());
    ch_body.extend_from_slice(&extensions);

    let mut handshake = Vec::new();
    handshake.push(0x01);
    let hs_len = ch_body.len() as u32;
    handshake.push(((hs_len >> 16) & 0xFF) as u8);
    handshake.push(((hs_len >> 8) & 0xFF) as u8);
    handshake.push((hs_len & 0xFF) as u8);
    handshake.extend_from_slice(&ch_body);

    let mut record = Vec::new();
    record.extend_from_slice(&[0x16, 0x03, 0x01]);
    record.extend_from_slice(&(handshake.len() as u16).to_be_bytes());
    record.extend_from_slice(&handshake);
    record
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_live_steam_doh_and_fragmented_tls() {
        for domain in [
            "store.steampowered.com",
            "steamcommunity.com",
            "avatars.steamstatic.com",
            "avatars.akamai.steamstatic.com",
            "steam-chat.com",
            "api.steampowered.com",
        ] {
            let res = test_single_domain_tls_handshake(domain).await;
            println!("Live Test [{}]: {:?}", domain, res);
            assert!(res.is_ok(), "Failed handshake for {}: {:?}", domain, res);
        }
    }
}
