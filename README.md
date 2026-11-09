# Steam Unblock Network

DoH Resolver, SNI Proxy, and TLS Record Fragmentation Engine for Steam on Windows.

Author: x2niosvn (x2niosvn@gmail.com)  
License: MIT  

---

## Overview

Steam Unblock Network is a specialized local network utility engineered to resolve accessibility restrictions affecting Steam web services (Store, Community, Inventory, Market, Trade, Profile, Username, and Avatar CDN) implemented by internet service providers in Vietnam.

Unlike conventional VPN or TUN/TAP-based tunneling solutions, Steam Unblock Network routes exclusively HTTP/HTTPS and WebSocket traffic for explicitly targeted Steam endpoints through a local DoH resolver and RFC-compliant TLS ClientHello record fragmenter. Game server UDP matchmaking and real-time multiplayer traffic remain entirely untouched, preserving direct low-latency routing without jitter or latency penalties.

---

## Architecture & How It Works

```
[ Steam Desktop Client ]
        |
        +--- HTTPS / Web Traffic (TCP:443) 
        |       |
        |       v (via local hosts redirection)
        |    [ Local SNI Proxy: 127.0.0.1:443 ]
        |       |
        |       +---> DNS over HTTPS (DoH + EDNS Client Subnet)
        |       |     Resolves nearest regional Akamai/Fastly edge IPs (~2-5ms)
        |       |
        |       +---> RFC-Compliant TLS Fragmentation
        |             Splits ClientHello handshake record across SNI boundaries
        |             Bypasses ISP Deep Packet Inspection (DPI)
        |             No Root CA certificate installation required
        |       |
        |       v
        |    [ Steam Edge Servers & CM WebSockets ]
        |
        +--- Game Traffic / Matchmaking (UDP)
                |
                v (Direct routing - 100% untouched)
             [ Regional Game Servers (0ms Added Latency) ]
```

### 1. DNS over HTTPS (DoH) with EDNS Client Subnet
The engine queries upstream DoH resolvers (Google Primary and Cloudflare Fallback) utilizing EDNS Client Subnet (ECS) negotiation. This ensures returned IP addresses correspond to regional Akamai and Fastly cache nodes located closest to Vietnam, avoiding foreign routing loops and delivering sub-5ms round-trip times.

### 2. TLS ClientHello Record Fragmentation
Traditional Deep Packet Inspection (DPI) hardware scans the Server Name Indication (SNI) field within the initial unencrypted TLS `ClientHello` record. Steam Unblock Network intercepts this record on `127.0.0.1:443` and fragments it into multiple standard RFC 5246/8446 TLS records:
- The first fragment contains the TLS record header and the initial byte of the handshake payload.
- Subsequent fragments split across the SNI extension bytes with distinct TCP segmentation (`TCP_NODELAY` and inter-packet transmission pacing).
Because this process operates strictly at the TLS record framing layer without decrypting or terminating payload data, no custom Root CA certificate or system trust store modification is necessary.

### 3. Steam Connection Manager (CM WebSocket) & Avatar Fix
Steam desktop clients (`steamwebhelper.exe`) authenticate identity, fetch profile metadata, and retrieve avatars via Steam Connection Manager WebSocket endpoints (`cmp*.steamserver.net` on ports 27018-27036 and 443) alongside avatar storage CDNs (`avatars.steamstatic.com`, `avatars.akamai.steamstatic.com`). By mapping and tunneling these CM clusters through DoH and SNI fragmentation, the persistent missing avatar issue (placeholder `?` icon) and profile header synchronization failures are resolved.

---

## Feature Comparison

| Feature | Steam Unblock Network | VPN / WARP | GoodbyeDPI / WinDivert | Manual Hosts Editing |
| :--- | :---: | :---: | :---: | :---: |
| Steam Store & Community Unblock | Yes | Yes | Partial | Partial (IPs expire) |
| Profile Avatar & Username Fix | Yes | Yes | No | No |
| Game Matchmaking Latency Impact | 0 ms (Untouched) | +20 ms to +80 ms | 0 ms | 0 ms |
| Kernel Driver Required | No | Yes (TUN/TAP) | Yes (WinDivert.sys) | No |
| Anti-Cheat Compatibility | 100% Safe (User-space) | Mixed | Risk of False Flag | 100% Safe |
| Custom Root CA Certificate | Not Required | Often Required | Not Required | Not Required |
| Automatic Setup & Teardown | Yes | Manual / App | Manual | Manual |
| Windows Service (Silent Background) | Yes | Varies | Third-party wrapper | No |
| Memory Footprint | ~20 MB | ~100 MB - 300 MB | ~15 MB | 0 MB |

---

## Core Capabilities

- Plug & Play Automation: Automatically binds the local proxy engine and synchronizes the Windows `hosts` file upon startup.
- Clean Teardown Guarantee: Restores original `hosts` file contents and executes a full Windows DNS cache flush (`ipconfig /flushdns`) upon application shutdown, system power-off, or service uninstallation.
- Windows Background Service: Native integration with the Windows Service Control Manager (`SteamUnblockService` with `Automatic` startup type). Runs silently at boot without requiring an open window or user login session.
- Realtime Network Telemetry: Dynamic canvas waveform telemetry rendering live transfer throughput (KB/s, MB/s) and cumulative byte counters.
- Single-Click Handshake Diagnostics: Real-time TLS handshake latency verification against active Steam endpoints with sub-second visual confirmation.
- Memory-Safe Ring Buffer Logging: High-performance in-memory ring buffer strictly capped at the latest 50 entries to eliminate memory growth and rendering lag during long running sessions.
- Modern Frameless User Interface: Custom window chrome with integrated drag physics, system tray minimizing, and dark colorway.

---

## Installation & Usage

### Method 1: Graphical Application (Recommended)

1. Download the latest `steam-unblock-network.exe` from the GitHub Releases page.
2. Right-click `steam-unblock-network.exe` and select "Run as administrator" (elevated privileges are required to bind TCP port 443 and write to the system `hosts` file).
3. The application starts the proxy engine and applies system routes automatically.
4. Restart the Steam client completely (`Steam -> Exit` then re-launch) to reload all connection pools.

### Method 2: Windows Background Service (Silent Autostart)

To maintain unblocking across system reboots without keeping the desktop interface open:

From the GUI:
- Click the "Cai dat" button inside the "Dich vu Windows" panel.

From Command Prompt or PowerShell (Run as Administrator):
```powershell
# Install and start the background service
.\steam-unblock-network.exe --install-service

# Query service status
sc.exe query SteamUnblockService

# Stop and uninstall the service (restores original hosts)
.\steam-unblock-network.exe --uninstall-service
```

---

## Command Line Interface (CLI)

The executable supports command-line switches for headless administration, automation scripts, and troubleshooting:

```
USAGE:
    steam-unblock-network.exe [OPTIONS]

OPTIONS:
    --install-service      Registers and starts SteamUnblockService in Windows SCM
    --uninstall-service    Stops, deregisters the service, and restores network state
    --run-service          Internal dispatcher flag invoked by Windows Service Manager
    --test-doh             Executes diagnostic DNS-over-HTTPS resolution test
    --cleanup              Emergency teardown: strips hosts markers and flushes DNS
    --help, -h             Prints command line usage instructions
```

---

## Custom Domain Extension

The proxy engine bundles 42+ official Steam web and infrastructure domains by default. To append custom domains without recompiling, create a plaintext file named `steam-domains.txt` in the same directory as the executable:

```text
# steam-domains.txt: One domain per line
my-custom-steam-cache.example.com
partner.steampowered.com
```

The application detects and merges these rules dynamically at startup.

---

## Building from Source

### Prerequisites
- Windows 10 or Windows 11 (x64)
- Rust toolchain (1.78 or newer): `https://rustup.rs/`
- Visual Studio C++ Build Tools with Windows SDK
- WebView2 Runtime (installed by default on Windows 10/11)

### Compilation

```powershell
# Clone the repository
git clone https://github.com/x2niosvn/SteamUnblockNetwork.git
cd SteamUnblockNetwork

# Build optimized release binary
cargo build --release

# Run internal integration tests
cargo test
```

The optimized, standalone executable will be located at:
```
target/release/steam-unblock-network.exe
```

---

## Security & Integrity Considerations

1. Zero Decryption: Steam Unblock Network is an SNI record proxy, not an intercepting HTTPS proxy. It does not perform TLS termination, cannot inspect encrypted payload data, and requires no custom CA certificates.
2. User-space Operation: The utility operates entirely in user space through standard Winsock APIs without installing third-party kernel drivers (e.g., WinDivert or NDIS filters), ensuring zero risk of anti-cheat conflict (Valve Anti-Cheat, Vanguard, Easy Anti-Cheat, BattlEye).
3. System State Safety: All modifications to `C:\Windows\System32\drivers\etc\hosts` are bounded by explicit markers (`# >>> STEAM-UNBLOCK-NETWORK START >>>`) and backed up to `hosts.steam_unblock.bak` before any mutation. The teardown routine guarantees atomic restoration upon exit.

---

## Author

- Developer: x2niosvn
- Contact: x2niosvn@gmail.com
- Repository: https://github.com/x2niosvn/SteamUnblockNetwork

---

## License

This project is licensed under the terms of the MIT License. See the [LICENSE](LICENSE) file for full text.
