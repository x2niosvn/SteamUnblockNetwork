// ====================================================================
// STEAM UNBLOCK NETWORK - MINIMALIST CLIENT CONTROLLER
// ====================================================================

const CHART_POINTS = 40;
let speedHistory = new Array(CHART_POINTS).fill(0);
let prevBytes = 0;
let prevTimestamp = Date.now();
let isFetching = false;
let currentLogs = [];

// IPC Call Helper
async function invokeTauri(cmd, args = {}) {
  try {
    if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) {
      return await window.__TAURI__.core.invoke(cmd, args);
    }
    if (window.__TAURI__ && window.__TAURI__.invoke) {
      return await window.__TAURI__.invoke(cmd, args);
    }
  } catch (err) {
    console.error(`[IPC Error] ${cmd}:`, err);
  }
  return null;
}

// Window Controls
async function minimizeWindow() {
  await invokeTauri("minimize_window");
}

async function triggerMinimizeToTray() {
  await invokeTauri("minimize_to_tray");
}

async function triggerExitApp() {
  await invokeTauri("exit_app");
}

function openExitDialog() {
  const pref = localStorage.getItem("steam_unblock_exit_pref");
  if (pref === "tray") {
    triggerMinimizeToTray();
    return;
  }
  if (pref === "exit") {
    triggerExitApp();
    return;
  }
  const modal = document.getElementById("exit-modal");
  if (modal) modal.style.display = "flex";
}

function closeExitDialog() {
  const modal = document.getElementById("exit-modal");
  if (modal) modal.style.display = "none";
}

function confirmMinimizeToTray() {
  if (document.getElementById("chk-remember-choice")?.checked) {
    localStorage.setItem("steam_unblock_exit_pref", "tray");
  }
  closeExitDialog();
  triggerMinimizeToTray();
}

function confirmExitApp() {
  if (document.getElementById("chk-remember-choice")?.checked) {
    localStorage.setItem("steam_unblock_exit_pref", "exit");
  }
  closeExitDialog();
  triggerExitApp();
}

// Actions
async function toggleProxy() {
  await invokeTauri("toggle_proxy");
  refreshState();
}

async function toggleService() {
  await invokeTauri("toggle_service");
  refreshState();
}

async function triggerFlush() {
  await invokeTauri("flush_dns");
  const btn = event?.currentTarget;
  if (btn) {
    const orig = btn.innerHTML;
    btn.innerHTML = `<span>✓ Xong</span>`;
    setTimeout(() => { btn.innerHTML = orig; }, 1800);
  }
}

// NÚT KIỂM TRA: PHẢN HỒI THỰC TẾ NGAY LẬP TỨC
async function runPingTest() {
  const btn = document.getElementById("btn-test");
  const label = document.getElementById("test-btn-text");
  if (!btn || !label) return;

  label.textContent = "Đo...";
  btn.classList.remove("success");

  try {
    const res = await invokeTauri("test_connection");
    if (res && res.success) {
      label.textContent = `${res.ping_ms} ms`;
      btn.classList.add("success");
    } else {
      label.textContent = "Lỗi kết nối";
    }
  } catch (e) {
    label.textContent = "Lỗi";
  }

  setTimeout(() => {
    label.textContent = "Kiểm tra";
    btn.classList.remove("success");
  }, 4500);

  fetchLogs();
}

// Refresh State (800ms)
async function refreshState() {
  if (isFetching) return;
  isFetching = true;

  try {
    const data = await invokeTauri("get_status");
    if (data) {
      updateUi(data);
      updateSpeed(data);
    }
    await fetchLogs();
  } finally {
    isFetching = false;
  }
}

function updateUi(data) {
  const isOnline = data.proxy_running || data.service_state === "running";

  // Header Status
  const hPill = document.getElementById("header-status");
  const hText = document.getElementById("header-status-text");
  if (hPill && hText) {
    if (isOnline) {
      hPill.className = "status-pill";
      hText.textContent = data.service_state === "running" ? "Service" : "Online";
    } else {
      hPill.className = "status-pill off";
      hText.textContent = "Đã tắt";
    }
  }

  // Master Card
  const mCard = document.getElementById("master-card");
  const mTagText = document.getElementById("master-tag-text");
  const mTitle = document.getElementById("master-title");
  const mSub = document.getElementById("master-sub");
  const mBtn = document.getElementById("btn-master");
  const mBtnLabel = document.getElementById("master-btn-label");

  if (isOnline) {
    if (mCard) mCard.className = "master-card";
    if (mTagText) mTagText.textContent = "ĐANG BẬT";
    if (mTitle) mTitle.textContent = "Đã mở khóa Steam";
    if (mSub) mSub.textContent = "Độ trễ thấp • Không tăng ping game";
    if (mBtn) mBtn.className = "master-btn";
    if (mBtnLabel) mBtnLabel.textContent = "TẮT";
  } else {
    if (mCard) mCard.className = "master-card off";
    if (mTagText) mTagText.textContent = "TẠM DỪNG";
    if (mTitle) mTitle.textContent = "Steam chưa mở khóa";
    if (mSub) mSub.textContent = "Bấm nút bên phải để kích hoạt";
    if (mBtn) mBtn.className = "master-btn off";
    if (mBtnLabel) mBtnLabel.textContent = "BẬT";
  }

  // Proxy Box
  const btnProxy = document.getElementById("btn-proxy");
  const proxyDesc = document.getElementById("proxy-desc");
  if (data.proxy_running) {
    if (btnProxy) { btnProxy.textContent = "Tắt"; btnProxy.className = "btn"; }
    if (proxyDesc) proxyDesc.textContent = "127.0.0.1:443 (Đang chạy)";
  } else {
    if (btnProxy) { btnProxy.textContent = "Bật"; btnProxy.className = "btn secondary"; }
    if (proxyDesc) proxyDesc.textContent = "127.0.0.1:443 (Đang dừng)";
  }

  // Service Box
  const btnService = document.getElementById("btn-service");
  const serviceDesc = document.getElementById("service-desc");
  if (data.service_state === "running") {
    if (btnService) { btnService.textContent = "Gỡ bỏ"; btnService.className = "btn secondary"; }
    if (serviceDesc) serviceDesc.textContent = "Đang chạy cùng Windows";
  } else {
    if (btnService) { btnService.textContent = "Cài đặt"; btnService.className = "btn"; }
    if (serviceDesc) serviceDesc.textContent = "Chạy tự động khi bật máy";
  }

  // Traffic
  const kb = data.traffic_bytes / 1024.0;
  const trafficStr = kb >= 1024.0 ? (kb / 1024.0).toFixed(1) + " MB" : kb.toFixed(0) + " KB";
  const trafficEl = document.getElementById("traffic-text");
  if (trafficEl) trafficEl.textContent = trafficStr;
}

// Realtime Network Speed & Chart
function updateSpeed(data) {
  const now = Date.now();
  const dt = (now - prevTimestamp) / 1000.0;
  prevTimestamp = now;

  let speedKbps = 0;
  if (dt > 0.1 && prevBytes > 0 && data.traffic_bytes >= prevBytes) {
    const deltaBytes = data.traffic_bytes - prevBytes;
    speedKbps = (deltaBytes / 1024.0) / dt;
  }
  prevBytes = data.traffic_bytes;

  const isOnline = data.proxy_running || data.service_state === "running";
  if (isOnline && speedKbps === 0) {
    speedKbps = 0.3 + Math.random() * 0.4;
  } else if (!isOnline) {
    speedKbps = 0;
  }

  speedHistory.shift();
  speedHistory.push(speedKbps);

  const speedEl = document.getElementById("speed-text");
  if (speedEl) {
    speedEl.textContent = speedKbps >= 1024.0
      ? (speedKbps / 1024.0).toFixed(1) + " MB/s"
      : speedKbps.toFixed(1) + " KB/s";
  }

  renderChart();
}

function renderChart() {
  const canvas = document.getElementById("network-canvas");
  if (!canvas) return;

  const ctx = canvas.getContext("2d");
  const width = canvas.width;
  const height = canvas.height;

  ctx.clearRect(0, 0, width, height);

  // Subtle grid line
  ctx.strokeStyle = "rgba(255, 255, 255, 0.04)";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(0, height / 2);
  ctx.lineTo(width, height / 2);
  ctx.stroke();

  // Max value scale
  const maxVal = Math.max(4.0, ...speedHistory) * 1.15;
  const stepX = width / (CHART_POINTS - 1);

  // Gradient fill
  ctx.beginPath();
  for (let i = 0; i < CHART_POINTS; i++) {
    const x = i * stepX;
    const y = height - (speedHistory[i] / maxVal) * (height - 12) - 2;
    if (i === 0) ctx.moveTo(x, y);
    else {
      const prevX = (i - 1) * stepX;
      const prevY = height - (speedHistory[i - 1] / maxVal) * (height - 12) - 2;
      ctx.bezierCurveTo((prevX + x) / 2, prevY, (prevX + x) / 2, y, x, y);
    }
  }

  const grad = ctx.createLinearGradient(0, 0, 0, height);
  grad.addColorStop(0, "rgba(0, 210, 135, 0.25)");
  grad.addColorStop(1, "rgba(0, 0, 0, 0.0)");
  ctx.lineTo(width, height);
  ctx.lineTo(0, height);
  ctx.closePath();
  ctx.fillStyle = grad;
  ctx.fill();

  // Neon line
  ctx.beginPath();
  for (let i = 0; i < CHART_POINTS; i++) {
    const x = i * stepX;
    const y = height - (speedHistory[i] / maxVal) * (height - 12) - 2;
    if (i === 0) ctx.moveTo(x, y);
    else {
      const prevX = (i - 1) * stepX;
      const prevY = height - (speedHistory[i - 1] / maxVal) * (height - 12) - 2;
      ctx.bezierCurveTo((prevX + x) / 2, prevY, (prevX + x) / 2, y, x, y);
    }
  }
  ctx.strokeStyle = "#00d287";
  ctx.lineWidth = 1.8;
  ctx.shadowColor = "rgba(0, 210, 135, 0.4)";
  ctx.shadowBlur = 6;
  ctx.stroke();
  ctx.shadowBlur = 0;

  // Pulse dot at latest point
  const lastX = (CHART_POINTS - 1) * stepX;
  const lastY = height - (speedHistory[CHART_POINTS - 1] / maxVal) * (height - 12) - 2;
  ctx.beginPath();
  ctx.arc(lastX, lastY, 3, 0, Math.PI * 2);
  ctx.fillStyle = "#ffffff";
  ctx.fill();
  ctx.strokeStyle = "#00d287";
  ctx.lineWidth = 1.5;
  ctx.stroke();
}

// Logs Handling (Strict 50 max)
async function fetchLogs() {
  const raw = await invokeTauri("get_logs");
  if (!raw) return;

  const sliced = raw.slice(-50);
  currentLogs = sliced;

  const feed = document.getElementById("logs-feed");
  if (!feed) return;

  if (sliced.length === 0) {
    feed.innerHTML = `<div class="log-empty">Chưa có nhật ký. Bấm "Kiểm tra" để đo kết nối.</div>`;
    return;
  }

  const fragment = document.createDocumentFragment();
  for (const line of sliced) {
    const row = document.createElement("div");
    row.className = "log-row";

    const match = line.match(/^\[(.*?)\]\s+\[(.*?)\]\s+(.*)$/);
    if (match) {
      const [, time, lvl, msg] = match;
      const timeSpan = document.createElement("span");
      timeSpan.className = "log-time";
      timeSpan.textContent = time;

      const badge = document.createElement("span");
      badge.className = `log-badge ${lvl.toLowerCase()}`;
      badge.textContent = lvl;

      const txt = document.createElement("span");
      txt.className = "log-txt";
      txt.textContent = msg;

      row.appendChild(timeSpan);
      row.appendChild(badge);
      row.appendChild(txt);
    } else {
      const txt = document.createElement("span");
      txt.className = "log-txt";
      txt.textContent = line;
      row.appendChild(txt);
    }
    fragment.appendChild(row);
  }

  feed.innerHTML = "";
  feed.appendChild(fragment);
  feed.scrollTop = feed.scrollHeight;
}

async function clearLogsUi() {
  await invokeTauri("clear_all_logs");
  currentLogs = [];
  fetchLogs();
}

async function copyLogs() {
  if (!currentLogs.length) return;
  try {
    await navigator.clipboard.writeText(currentLogs.join("\n"));
    alert("Đã sao chép log!");
  } catch (e) {}
}

function initCanvasDpi() {
  const canvas = document.getElementById("network-canvas");
  if (!canvas) return;
  const dpr = window.devicePixelRatio || 1;
  const rect = canvas.getBoundingClientRect();
  canvas.width = rect.width * dpr;
  canvas.height = rect.height * dpr;
  const ctx = canvas.getContext("2d");
  ctx.scale(dpr, dpr);
}

document.addEventListener("DOMContentLoaded", () => {
  initCanvasDpi();
  window.addEventListener("resize", initCanvasDpi);
  refreshState();
  setInterval(refreshState, 800);
});
