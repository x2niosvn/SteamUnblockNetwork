# Steam Unblock Network 🚀

> **DoH Resolver • SNI Proxy • TLS Fragmenter • Realtime Network Monitor**  
> *Tác giả:* **x2niosvn** (`x2niosvn@gmail.com`)

Ứng dụng chuyên biệt mở khóa toàn diện **Steam Store, Cộng đồng (Community), Kho đồ (Inventory), Chợ (Market), Bạn bè & Hồ sơ (Username / Avatar)** bị chặn tại Việt Nam trên hệ điều hành Windows — **tuyệt đối không làm tăng Ping hay ảnh hưởng kết nối khi chơi game trên Steam**.

---

## 🌟 Tính Năng Nổi Bật

- ⚡ **Plug & Play**: Mở ứng dụng là tự động kích hoạt Proxy và điều hướng `hosts` — không cần cấu hình thủ công.
- 👤 **Khắc phục triệt để lỗi Username & Avatar**: Tự động giải mã và kết nối các máy chủ **Steam Connection Manager (CM WebSocket)** (`*.steamserver.net`) và CDN Avatar, xử lý dứt điểm lỗi icon `?`.
- 📊 **Biểu đồ mạng Realtime**: Giám sát tốc độ truyền tải tức thời (KB/s - MB/s) và tổng dung lượng dữ liệu bằng đồ thị sóng thời gian thực.
- ⚡ **Kiểm tra Ping 1 chạm**: Nút kiểm tra phản hồi tức thì độ trễ handshake thực tế tới cụm máy chủ Steam (`✓ 2 ms`).
- 🧹 **Clean Teardown**: Tự động khôi phục nguyên vẹn cài đặt mạng gốc (`hosts` & DNS cache) khi đóng app hoặc gỡ dịch vụ.
- 🛠️ **Windows Background Service**: Tùy chọn cài đặt thành dịch vụ Windows chạy ngầm im lặng cùng hệ thống (`Automatic`) mà không cần mở giao diện.
- 🎯 **Không ảnh hưởng Ping Game**: Chỉ can thiệp lưu lượng HTTPS web của Steam Client; toàn bộ gói tin UDP Direct Matchmaking của game giữ nguyên kết nối trực tiếp với độ trễ thấp nhất.
- 🔒 **An toàn tuyệt đối**: Không cần cài đặt chứng chỉ Root CA giả mạo nhờ kỹ thuật phân mảnh bản ghi TLS ClientHello chuẩn RFC.

---

## 🎨 Giao Diện Tối Giản (Minimalist UI)

- Giao diện **Frameless** hiện đại, không dùng thanh viền thô mặc định của Windows.
- Nút thu nhỏ và nút đóng tích hợp trực tiếp góc trên ứng dụng.
- Tùy chọn chạy ngầm ở Khay Hệ Thống (System Tray) hoặc thoát hoàn toàn.
- Hệ thống log tự động dọn dẹp, chỉ giữ tối đa 50 dòng mới nhất nhằm chống giật lag.

---

## 🚀 Hướng Dẫn Sử Dụng

### Cách 1: Chạy trực tiếp giao diện (Khuyên dùng)
1. Tải bản mới nhất từ mục **[Releases](https://github.com/x2niosvn/SteamUnblockNetwork/releases)**.
2. Nhấp chuột phải vào `steam-unblock-network.exe` chọn **Run as administrator** (cần thiết để lắng nghe cổng `443` cục bộ và cập nhật file `hosts`).
3. Ứng dụng tự động kích hoạt chế độ bảo vệ. Thoát hoàn toàn Steam Client (`Steam > Exit`) rồi mở lại để cảm nhận sự khác biệt.

### Cách 2: Chạy nền vĩnh viễn cùng Windows (Windows Service)
- Trên giao diện app, tại thẻ **Dịch Vụ Windows**, bấm **Cài đặt**.
- Hoặc chạy qua dòng lệnh PowerShell (Run as Administrator):
  ```powershell
  .\steam-unblock-network.exe --install-service
  ```
- Dịch vụ `SteamUnblockService` sẽ tự động chạy ngầm mỗi khi bật máy.
- Để gỡ bỏ dịch vụ:
  ```powershell
  .\steam-unblock-network.exe --uninstall-service
  ```

---

## 🛠️ Biên Dịch Từ Mã Nguồn (Build from Source)

Yêu cầu máy đã cài **Rust** (khuyên dùng bản stable mới nhất) và **Visual Studio C++ Build Tools**:

```bash
# Clone repository
git clone https://github.com/x2niosvn/SteamUnblockNetwork.git
cd SteamUnblockNetwork

# Build bản Release tối ưu hóa
cargo build --release
```

File thực thi sau khi biên dịch nằm tại: `target/release/steam-unblock-network.exe`.

---

## 📄 Bản Quyền (License)

Phát hành theo giấy phép **MIT License** — Tự do sử dụng và tùy biến.  
Phát triển bởi **x2niosvn**.
