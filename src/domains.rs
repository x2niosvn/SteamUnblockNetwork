use std::collections::BTreeSet;
use std::fs;

/// Danh sách các tên miền Steam Web / Store / Community / Kho hàng / Hồ sơ / Avatar / API / Static CDN
/// được định tuyến qua Local SNI Proxy (127.0.0.1:443) + DoH + TLS Fragmentation.
///
/// LƯU Ý QUAN TRỌNG:
/// Không định tuyến các máy chủ trận đấu (Game Servers UDP / Steam Datagram Relay)
/// hoặc máy chủ tải game tốc độ cao (*.steamcontent.com) qua proxy này,
/// giúp việc chơi các tựa game trên Steam giữ nguyên 100% Ping gốc và độ ổn định tối đa.
pub const DEFAULT_STEAM_DOMAINS: &[&str] = &[
    // 1. Steam Store, Checkout, Auth & API
    "steampowered.com",
    "www.steampowered.com",
    "store.steampowered.com",
    "checkout.steampowered.com",
    "login.steampowered.com",
    "help.steampowered.com",
    "api.steampowered.com",
    "partner.steampowered.com",
    "store.akamai.steampowered.com",
    "media.steampowered.com",
    // 2. Steam Community, Kho hàng (Inventory), Chợ (Market), Hồ sơ & Avatar
    "steamcommunity.com",
    "www.steamcommunity.com",
    "steamcommunity-a.akamaihd.net",
    "steamuserimages-a.akamaihd.net",
    "steamcdn-a.akamaihd.net",
    "steamusercontent-a.akamaihd.net",
    "images.akamai.steamusercontent.com",
    "steamusercontent.com",
    // 3. Steam Chat & Friends WebUI
    "steam-chat.com",
    "www.steam-chat.com",
    "chat.steampowered.com",
    // 4. Steam Static & Avatar CDN (Giao diện Store, Ảnh đại diện, Khung Avatar, Icon Kho hàng)
    "store.steamstatic.com",
    "cdn.steamstatic.com",
    "shared.steamstatic.com",
    "community.steamstatic.com",
    "avatars.steamstatic.com",
    "clan.steamstatic.com",
    // Akamai SteamStatic
    "store.akamai.steamstatic.com",
    "cdn.akamai.steamstatic.com",
    "shared.akamai.steamstatic.com",
    "community.akamai.steamstatic.com",
    "avatars.akamai.steamstatic.com",
    "clan.akamai.steamstatic.com",
    "video.akamai.steamstatic.com",
    // Fastly SteamStatic
    "store.fastly.steamstatic.com",
    "cdn.fastly.steamstatic.com",
    "shared.fastly.steamstatic.com",
    "community.fastly.steamstatic.com",
    "avatars.fastly.steamstatic.com",
    "clan.fastly.steamstatic.com",
    "video.fastly.steamstatic.com",
    // Cloudflare SteamStatic
    "store.cloudflare.steamstatic.com",
    "cdn.cloudflare.steamstatic.com",
    "shared.cloudflare.steamstatic.com",
    "community.cloudflare.steamstatic.com",
    "avatars.cloudflare.steamstatic.com",
    "clan.cloudflare.steamstatic.com",
    "video.cloudflare.steamstatic.com",
    // 5. Steam shortlinks & media
    "s.team",
    "steam.tv",
];

/// Danh sách các máy chủ Steam Connection Manager (CM WebSocket) dùng cổng 27018-27036 & 443.
/// Các máy chủ này chịu trách nhiệm đăng nhập phiên Steam Client, tải Username, trạng thái Bạn bè và Avatar
/// ở góc trên bên phải ứng dụng Steam. Do sử dụng đa cổng (27018-27036) và chỉ bị chặn DNS,
/// các máy chủ này được ánh xạ trực tiếp tới IP thật trong file hosts.
pub const STEAM_CM_DIRECT_HOSTS: &[(&str, &str)] = &[
    // Hong Kong (Ping thấp nhất từ Việt Nam)
    ("103.28.54.100", "cmp1-hkg1.steamserver.net"),
    ("103.28.54.101", "cmp2-hkg1.steamserver.net"),
    ("103.28.54.102", "cmp3-hkg1.steamserver.net"),
    // Singapore
    ("103.10.124.4", "cmp1-sgp1.steamserver.net"),
    ("103.10.124.5", "cmp2-sgp1.steamserver.net"),
    // Tokyo
    ("45.121.184.100", "cmp1-tyo3.steamserver.net"),
    ("45.121.184.101", "cmp2-tyo3.steamserver.net"),
    // Asia / Global Backup CMs
    ("155.133.225.20", "ext1-maa2.steamserver.net"),
    ("155.133.225.21", "ext2-maa2.steamserver.net"),
    ("162.254.195.69", "cmp1-lax1.steamserver.net"),
    ("162.254.195.75", "cmp2-lax1.steamserver.net"),
    ("205.196.6.132", "cmp1-sea1.steamserver.net"),
    ("205.196.6.133", "cmp2-sea1.steamserver.net"),
    ("162.254.193.103", "cmp1-ord1.steamserver.net"),
    ("162.254.193.75", "cmp2-ord1.steamserver.net"),
    ("162.254.194.56", "cmp1-dfw2.steamserver.net"),
    ("162.254.194.57", "cmp2-dfw2.steamserver.net"),
    // Connectivity Check Host
    ("113.171.10.224", "steamconnecttest.com"),
];

/// Đọc danh sách domain mặc định kết hợp với file `steam-domains.txt` (nếu có) cạnh file .exe
pub fn get_target_domains() -> Vec<String> {
    let mut set: BTreeSet<String> = DEFAULT_STEAM_DOMAINS
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            let custom_file = dir.join("steam-domains.txt");
            if let Ok(content) = fs::read_to_string(&custom_file) {
                for line in content.lines() {
                    let trimmed = line.trim().to_lowercase();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.contains(' ') {
                        set.insert(trimmed);
                    }
                }
            }
        }
    }

    set.into_iter().collect()
}

/// Kiểm tra một hostname SNI có thuộc phạm vi cho phép của Steam Proxy hay không
pub fn is_allowed_steam_domain(host: &str) -> bool {
    let h = host.trim().to_lowercase();
    if h.is_empty() {
        return false;
    }
    let suffixes = [
        "steampowered.com",
        "steamcommunity.com",
        "steamstatic.com",
        "steamusercontent.com",
        "akamaihd.net",
        "steam-chat.com",
        "s.team",
        "steam.tv",
        "steamserver.net",
        "steamconnecttest.com",
    ];
    if suffixes
        .iter()
        .any(|suf| h == *suf || h.ends_with(&format!(".{}", suf)))
    {
        return true;
    }
    get_target_domains().iter().any(|d| d == &h)
}
