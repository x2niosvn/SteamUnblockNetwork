use crate::logger::{log_info, log_warn};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
struct CachedDnsEntry {
    ips: Vec<Ipv4Addr>,
    expires_at: Instant,
}

#[derive(Deserialize, Debug)]
struct DohResponse {
    #[serde(rename = "Status")]
    status: Option<u32>,
    #[serde(rename = "Answer")]
    answer: Option<Vec<DohAnswer>>,
}

#[derive(Deserialize, Debug)]
struct DohAnswer {
    #[serde(rename = "type")]
    record_type: u16,
    #[serde(rename = "TTL")]
    ttl: Option<u64>,
    data: String,
}

static DNS_CACHE: Lazy<RwLock<HashMap<String, CachedDnsEntry>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

static HTTP_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(4))
        .connect_timeout(Duration::from_secs(2))
        .no_proxy()
        .build()
        .expect("Failed to initialize DoH HTTP client")
});

/// Danh sách các máy chủ DoH truy vấn trực tiếp bằng địa chỉ IP.
/// Ưu tiên Google DoH (8.8.8.8 / 8.8.4.4) vì hỗ trợ EDNS Client Subnet giúp trả về
/// máy chủ CDN Akamai/Fastly nội vùng (ping 2ms - 5ms) thay vì máy chủ quốc tế ở xa.
const DOH_ENDPOINTS: &[(&str, &str)] = &[
    ("Google DoH (8.8.8.8)", "https://8.8.8.8/resolve"),
    ("Google DoH (8.8.4.4)", "https://8.8.4.4/resolve"),
    ("Cloudflare DoH (1.1.1.1)", "https://1.1.1.1/dns-query"),
    ("Cloudflare DoH (1.0.0.1)", "https://1.0.0.1/dns-query"),
];

fn is_valid_public_ip(ip: &Ipv4Addr) -> bool {
    !ip.is_loopback()
        && !ip.is_unspecified()
        && !ip.is_private()
        && !ip.is_link_local()
        && !ip.is_broadcast()
}

/// Phân giải tên miền Steam sang danh sách IP thật thông qua DoH (DNS over HTTPS)
pub async fn resolve_domain_doh(domain: &str) -> Result<Vec<Ipv4Addr>, String> {
    let key = domain.trim().to_lowercase();

    // 1. Kiểm tra Cache trong RAM
    {
        let cache = DNS_CACHE.read();
        if let Some(entry) = cache.get(&key) {
            if Instant::now() < entry.expires_at && !entry.ips.is_empty() {
                return Ok(entry.ips.clone());
            }
        }
    }

    // 2. Truy vấn lần lượt các máy chủ DoH
    for (_provider_name, endpoint) in DOH_ENDPOINTS {
        match query_single_doh(endpoint, &key).await {
            Ok((ips, ttl)) if !ips.is_empty() => {
                let cache_ttl = Duration::from_secs(ttl.clamp(180, 1800));
                {
                    let mut cache = DNS_CACHE.write();
                    cache.insert(
                        key.clone(),
                        CachedDnsEntry {
                            ips: ips.clone(),
                            expires_at: Instant::now() + cache_ttl,
                        },
                    );
                }
                log_info(format!(
                    "[DoH DNS]   {:<30} ➔  {}",
                    key, ips[0]
                ));
                return Ok(ips);
            }
            Ok(_) => continue,
            Err(_) => continue,
        }
    }

    // 3. Nếu tất cả DoH tạm thời nghẽn, kiểm tra Cache dự phòng
    {
        let cache = DNS_CACHE.read();
        if let Some(entry) = cache.get(&key) {
            if !entry.ips.is_empty() {
                log_warn(format!("[DoH DNS]   Sử dụng IP dự phòng cho {}", key));
                return Ok(entry.ips.clone());
            }
        }
    }

    Err(format!(
        "Không thể phân giải IP qua DoH cho tên miền: {}",
        key
    ))
}

async fn query_single_doh(endpoint: &str, domain: &str) -> Result<(Vec<Ipv4Addr>, u64), String> {
    let url = format!("{}?name={}&type=A", endpoint, domain);
    let resp = HTTP_CLIENT
        .get(&url)
        .header("Accept", "application/dns-json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let parsed: DohResponse = resp.json().await.map_err(|e| e.to_string())?;
    if parsed.status.unwrap_or(0) != 0 {
        return Err(format!("DNS Status {}", parsed.status.unwrap_or(99)));
    }

    let mut ips = Vec::new();
    let mut min_ttl = 300u64;

    if let Some(answers) = parsed.answer {
        for ans in answers {
            if ans.record_type == 1 {
                if let Ok(ip) = ans.data.trim().parse::<Ipv4Addr>() {
                    if is_valid_public_ip(&ip) && !ips.contains(&ip) {
                        ips.push(ip);
                        if let Some(t) = ans.ttl {
                            min_ttl = min_ttl.min(t);
                        }
                    }
                }
            }
        }
    }

    Ok((ips, min_ttl))
}

pub fn clear_doh_cache() {
    DNS_CACHE.write().clear();
    log_info("[DoH DNS]   Đã làm mới bộ nhớ đệm phân giải tên miền DoH.");
}

pub fn cached_domain_count() -> usize {
    DNS_CACHE.read().len()
}
