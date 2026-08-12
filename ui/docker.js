// Docker ve Konteyner Adli Bilişimi Arayüz Modülü
import { showToast } from "./core/toast.js";

export const dockerState = {
  mode: "local", // "local" | "remote"
  customRoot: "",
  localStatus: null,
  localContainers: [],
  localScanned: false,
  remoteStatus: null,
  remoteContainers: [],
  remoteScanned: false,
  filter: "all", // "all" | "running" | "stopped" | "high_risk"
  search: "",
  selectedContainer: null,
  selectedTab: "overview", // "overview" | "security" | "secrets" | "drift" | "logs"
  containerLogs: [],
  loadingLogs: false,
  isScanning: false,
  isAcquiring: false,
  lastAction: "",
  remote: {
    ip: "127.0.0.1",
    port: 4444,
    token: "",
    connected: false,
  },
};

export function dockerPage({ t, icon, state, pageTitle, pickerField, field, escapeHtml, backendReady }) {
  const d = dockerState;
  const isRemote = d.mode === "remote";
  const status = isRemote ? d.remoteStatus : d.localStatus;
  const rawContainers = isRemote ? d.remoteContainers : d.localContainers;
  const isScanned = isRemote ? d.remoteScanned : d.localScanned;

  const renderField = field || ((label, control) => `
    <div class="field">
      <label>${label}</label>
      ${control}
    </div>
  `);

  const renderPicker = pickerField || ((label, id, value, type) => `
    <div class="field">
      <label>${label}</label>
      <div class="input-action">
        <input id="${id}" class="input" value="${escapeHtml(value)}" data-picker-target />
        <button class="secondary-button" data-action="${type === "folder" ? "pick-folder" : "pick-file"}" data-target="#${id}">
          ${icon(type === "folder" ? "folder" : "search")} ${t("select")}
        </button>
      </div>
    </div>
  `);

  // Konteyner filtreleme
  let filtered = (rawContainers || []).filter((c) => {
    if (d.filter === "running" && !c.running) return false;
    if (d.filter === "stopped" && c.running) return false;
    if (d.filter === "high_risk" && c.risk_level !== "HIGH" && c.risk_level !== "CRITICAL") return false;
    if (d.search) {
      const q = d.search.toLowerCase();
      const matchName = (c.name || "").toLowerCase().includes(q);
      const matchId = (c.id || "").toLowerCase().includes(q);
      const matchImage = (c.image || "").toLowerCase().includes(q);
      if (!matchName && !matchId && !matchImage) return false;
    }
    return true;
  });

  return `
    <section class="page">
      ${pageTitle(t("docker.title"), t("docker.desc"), "docker", icon)}

      <!-- Mod Değiştirici: Yerel vs Uzak Agent -->
      <div class="analysis-tabs">
        <button class="analysis-tab-btn ${!isRemote ? "active" : ""}" data-docker-action="set-mode" data-mode="local">
          ${icon("monitor")} ${t("docker.localMode")}
        </button>
        <button class="analysis-tab-btn ${isRemote ? "active" : ""}" data-docker-action="set-mode" data-mode="remote">
          ${icon("network")} ${t("docker.remoteMode")}
        </button>
      </div>

      <div class="workflow-layout">
        <!-- Sol Ana Panel -->
        <div class="workflow-panel">
          ${!isRemote ? `
            <p class="section-label">${t("docker.localSettings")}</p>
            ${renderPicker(t("docker.customRoot"), "docker-custom-root", d.customRoot || "/var/lib/docker", "folder")}
            <small class="field-hint" style="margin-top:-6px;margin-bottom:12px;display:block;">${t("docker.customRootHint")}</small>
            <div class="button-row" style="margin-top:14px;">
              <button class="primary-button" data-docker-action="scan-local" ${d.isScanning ? "disabled" : ""}>
                ${icon("refresh")} ${d.isScanning ? t("docker.scanning") : t("docker.scan")}
              </button>
            </div>
          ` : `
            <p class="section-label">${t("workflow.connectionOps")}</p>
            <div class="form-grid">
              ${renderField(t("workflow.ip"), `<input class="input" id="docker-remote-ip" placeholder="${t("workflow.ipPlaceholder")}" value="${escapeHtml(d.remote.ip)}" />`)}
              ${renderField(t("workflow.port"), `<input class="input" id="docker-remote-port" type="number" value="${escapeHtml(String(d.remote.port || 4444))}" />`)}
            </div>
            ${renderField(t("workflow.token"), `<input class="input" id="docker-remote-token" type="password" placeholder="${t("workflow.tokenPlaceholder")}" value="${escapeHtml(d.remote.token)}" />`)}
            <div class="button-row" style="margin-top:14px;">
              <button class="primary-button" data-docker-action="scan-remote" ${d.isScanning ? "disabled" : ""}>
                ${icon("network")} ${d.isScanning ? t("docker.scanning") : t("docker.connectAndScan")}
              </button>
            </div>
          `}

          <div class="section-divider"></div>
          
          <!-- Konteyner Listesi Başlığı ve Filtreler -->
          <p class="section-label">${t("docker.containersTitle")} (${filtered.length})</p>
          
          <div class="field">
            <input type="text" class="input" placeholder="${t("docker.searchPlaceholder")}" value="${escapeHtml(d.search)}" data-docker-action="search" />
          </div>

          <div class="button-row" style="margin-top:8px;margin-bottom:12px;">
            <button class="secondary-button ${d.filter === "all" ? "active" : ""}" data-docker-action="set-filter" data-filter="all">${t("docker.filterAll")}</button>
            <button class="secondary-button ${d.filter === "running" ? "active" : ""}" data-docker-action="set-filter" data-filter="running">${t("docker.filterRunning")}</button>
            <button class="secondary-button ${d.filter === "stopped" ? "active" : ""}" data-docker-action="set-filter" data-filter="stopped">${t("docker.filterStopped")}</button>
            <button class="secondary-button ${d.filter === "high_risk" ? "active" : ""}" data-docker-action="set-filter" data-filter="high_risk">${t("docker.filterHighRisk")}</button>
          </div>

          ${!isScanned ? `
            <div class="log-box" style="margin-top:12px;text-align:center;padding:24px 16px;">
              • ${t("docker.notScannedPrompt")}
            </div>
          ` : (filtered.length === 0 ? `
            <div class="log-box" style="margin-top:12px;text-align:center;padding:24px 16px;">
              • ${t("docker.noContainers")}
            </div>
          ` : `
            <div class="docker-container-table-wrapper" style="margin-top:12px;">
              <table class="docker-table">
                <thead>
                  <tr>
                    <th>${t("docker.risk")}</th>
                    <th>Konteyner Adı / ID</th>
                    <th>İmaj</th>
                    <th>Durum</th>
                    <th>IP / Portlar</th>
                    <th>İşlemler</th>
                  </tr>
                </thead>
                <tbody>
                  ${filtered.map((c) => renderContainerRow(c, t, icon, escapeHtml)).join("")}
                </tbody>
              </table>
            </div>
          `)}
        </div>

        <!-- Sağ Bilgi Paneli -->
        <aside class="side-panel">
          <h3>${t("workflow.status")}</h3>
          ${sideInfo(t("docker.side.mode"), isRemote ? t("remoteAgent") : t("localOperation"), isRemote ? "network" : "monitor", "", icon)}
          ${sideInfo(t("docker.side.daemon"), status ? (status.docker_running ? t("docker.daemonRunning") : t("docker.daemonOffline")) : t("docker.daemonIdle"), "chip", "", icon)}
          ${sideInfo(t("docker.side.containers"), status ? `${status.running_count || 0} ${t("docker.running")} / ${status.containers_count || 0} ${t("docker.total")}` : "-", "docker", "", icon)}
          ${sideInfo(t("docker.side.driver"), status?.storage_driver || "-", "disk", "", icon)}
          ${sideInfo(t("workflow.lastAction"), d.lastAction || t("lastActionReady"), "clock", "", icon)}
        </aside>
      </div>

      <!-- Konteyner Detay Modal / İnceleme Paneli -->
      ${d.selectedContainer ? renderInspectorModal(d.selectedContainer, d, t, icon, escapeHtml) : ""}
    </section>
  `;
}

function sideInfo(title, body, iconName, key = "", icon) {
  return `
    <div class="side-info" ${key ? `data-side="${key}"` : ""}>
      <span class="metric-icon">${icon(iconName)}</span>
      <span><strong>${title}</strong><small>${body}</small></span>
    </div>
  `;
}

function renderContainerRow(c, t, icon, escapeHtml) {
  const riskClass =
    c.risk_level === "CRITICAL" ? "risk-badge-critical" :
    c.risk_level === "HIGH" ? "risk-badge-high" :
    c.risk_level === "MEDIUM" ? "risk-badge-medium" : "risk-badge-low";

  const riskLabel =
    c.risk_level === "CRITICAL" ? t("docker.riskCritical") :
    c.risk_level === "HIGH" ? t("docker.riskHigh") :
    c.risk_level === "MEDIUM" ? t("docker.riskMedium") : t("docker.riskLow");

  const statusClass = c.running ? "status-badge-running" : "status-badge-stopped";

  return `
    <tr class="docker-row ${c.risk_level === "CRITICAL" || c.risk_level === "HIGH" ? "row-highlight-risk" : ""}">
      <td>
        <span class="risk-badge ${riskClass}">
          ${riskLabel}
        </span>
      </td>
      <td>
        <div class="container-name-cell">
          <strong>${escapeHtml(c.name || "unnamed")}</strong>
          <code>${escapeHtml(c.short_id || (c.id ? c.id.substring(0, 12) : "-"))}</code>
        </div>
      </td>
      <td>
        <span class="image-tag" title="${escapeHtml(c.image || "-")}">
          ${escapeHtml(c.image || "-")}
        </span>
      </td>
      <td>
        <span class="status-badge ${statusClass}">
          ${c.running ? "● " + t("docker.running") : "○ " + t("docker.stopped")}
        </span>
      </td>
      <td>
        <small class="ip-port-text">
          ${c.ip_address ? `IP: ${escapeHtml(c.ip_address)}<br/>` : ""}
          ${c.ports && c.ports.length > 0 ? c.ports.slice(0, 2).map(p => escapeHtml(p)).join(", ") : "<span class='text-muted'>-</span>"}
        </small>
      </td>
      <td>
        <div class="row-action-buttons">
          <button class="secondary-button small-button" data-docker-action="inspect" data-id="${c.id}">
            ${icon("search")} ${t("docker.inspect")}
          </button>
          <button class="primary-button small-button" data-docker-action="acquire" data-id="${c.id}" data-name="${escapeHtml(c.name || "container")}">
            ${icon("download")} ${t("docker.acquire")}
          </button>
        </div>
      </td>
    </tr>
  `;
}

function renderInspectorModal(c, d, t, icon, escapeHtml) {
  const activeTab = d.selectedTab || "overview";

  return `
    <div class="modal-backdrop" data-docker-action="close-modal">
      <div class="docker-modal-content" onclick="event.stopPropagation()">
        <div class="modal-header">
          <div class="modal-title-group">
            <h2>${icon("docker")} ${escapeHtml(c.name || "container")} <code>(${c.short_id || (c.id ? c.id.substring(0, 12) : "-")})</code></h2>
            <span class="risk-badge ${c.risk_level === "CRITICAL" ? "risk-badge-critical" : c.risk_level === "HIGH" ? "risk-badge-high" : "risk-badge-low"}">
              ${c.risk_level} RISK
            </span>
          </div>
          <button class="modal-close-btn" data-docker-action="close-modal">✕</button>
        </div>

        <div class="modal-tab-bar">
          <button class="modal-tab-btn ${activeTab === "overview" ? "active" : ""}" data-docker-action="set-tab" data-tab="overview">
            ${icon("info")} ${t("docker.tabOverview")}
          </button>
          <button class="modal-tab-btn ${activeTab === "security" ? "active" : ""}" data-docker-action="set-tab" data-tab="security">
            ${icon("shield")} ${t("docker.tabSecurity")} (${c.risk_reasons?.length || 0})
          </button>
          <button class="modal-tab-btn ${activeTab === "secrets" ? "active" : ""}" data-docker-action="set-tab" data-tab="secrets">
            ${icon("key")} ${t("docker.tabSecrets")} (${c.secrets_found?.length || 0})
          </button>
          <button class="modal-tab-btn ${activeTab === "drift" ? "active" : ""}" data-docker-action="set-tab" data-tab="drift">
            ${icon("disk")} ${t("docker.tabDrift")}
          </button>
          <button class="modal-tab-btn ${activeTab === "logs" ? "active" : ""}" data-docker-action="set-tab" data-tab="logs">
            ${icon("report")} ${t("docker.tabLogs")}
          </button>
        </div>

        <div class="modal-body-scroll">
          ${activeTab === "overview" ? renderOverviewTab(c, t, escapeHtml) : ""}
          ${activeTab === "security" ? renderSecurityTab(c, t, escapeHtml) : ""}
          ${activeTab === "secrets" ? renderSecretsTab(c, t, escapeHtml) : ""}
          ${activeTab === "drift" ? renderDriftTab(c, t, escapeHtml) : ""}
          ${activeTab === "logs" ? renderLogsTab(d, t, escapeHtml) : ""}
        </div>

        <div class="modal-footer">
          <button class="secondary-button" data-docker-action="close-modal">Kapat / Close</button>
          <button class="primary-button" data-docker-action="acquire" data-id="${c.id}" data-name="${escapeHtml(c.name || "container")}">
            ${icon("download")} ${t("docker.startAcquisition")}
          </button>
        </div>
      </div>
    </div>
  `;
}

function renderOverviewTab(c, t, escapeHtml) {
  return `
    <div class="inspector-section">
      <div class="inspector-grid-2">
        <div class="meta-item"><strong>Konteyner ID:</strong> <code>${escapeHtml(c.id || "-")}</code></div>
        <div class="meta-item"><strong>İmaj:</strong> <code>${escapeHtml(c.image || "-")}</code></div>
        <div class="meta-item"><strong>Oluşturulma:</strong> ${escapeHtml(c.created || "-")}</div>
        <div class="meta-item"><strong>Durum:</strong> ${c.running ? "🟢 Çalışıyor (Running)" : "⚪ Durduruldu (Exited)"}</div>
        <div class="meta-item"><strong>Host PID:</strong> ${c.pid || "-"}</div>
        <div class="meta-item"><strong>Exit Code:</strong> ${c.exit_code ?? "-"}</div>
        <div class="meta-item"><strong>IP Adresi:</strong> ${c.ip_address || "-"}</div>
        <div class="meta-item"><strong>Storage Driver:</strong> ${escapeHtml(c.driver || "overlay2")}</div>
      </div>

      <h4 class="inspector-subtitle">Mount & Dizin Eşleşmeleri (${c.mounts?.length || 0})</h4>
      ${!c.mounts || c.mounts.length === 0 ? "<p class='text-muted'>Bağlı mount bulunamadı.</p>" : `
        <table class="inspector-mini-table">
          <thead><tr><th>Host Kaynak (Source)</th><th>Konteyner Hedef (Dest)</th><th>Mod</th><th>Yazılabilir (RW)</th></tr></thead>
          <tbody>
            ${c.mounts.map(m => `
              <tr>
                <td><code>${escapeHtml(m.source || "-")}</code></td>
                <td><code>${escapeHtml(m.destination || "-")}</code></td>
                <td>${escapeHtml(m.mode || "bind")}</td>
                <td>${m.rw ? "Evet (RW)" : "Salt Okunur (RO)"}</td>
              </tr>
            `).join("")}
          </tbody>
        </table>
      `}
    </div>
  `;
}

function renderSecurityTab(c, t, escapeHtml) {
  return `
    <div class="inspector-section">
      <div class="security-score-banner ${c.risk_level === "CRITICAL" ? "bg-critical" : c.risk_level === "HIGH" ? "bg-high" : "bg-low"}">
        <h3>Güvenlik Değerlendirmesi: ${c.risk_level} RISK</h3>
        <p>Amele, konteyner kaçış (breakout) ve host ele geçirme risklerini denetledi.</p>
      </div>

      <h4 class="inspector-subtitle">Tespit Edilen Risk Faktörleri (${c.risk_reasons?.length || 0})</h4>
      ${!c.risk_reasons || c.risk_reasons.length === 0 ? `
        <div class="empty-state-card">
          <p>Kritik bir güvenlik riski veya yetki yükseltme konfigürasyonu tespit edilmedi.</p>
        </div>
      ` : `
        <ul class="risk-reasons-list">
          ${c.risk_reasons.map(r => `<li><span class="warning-bullet">•</span> ${escapeHtml(r)}</li>`).join("")}
        </ul>
      `}
    </div>
  `;
}

function renderSecretsTab(c, t, escapeHtml) {
  return `
    <div class="inspector-section">
      <p class="field-hint">${t("docker.secretsFound")}</p>
      ${!c.secrets_found || c.secrets_found.length === 0 ? `
        <div class="empty-state-card"><p>${t("docker.noSecrets")}</p></div>
      ` : `
        <table class="inspector-mini-table">
          <thead><tr><th>Değişken Adı (Key)</th><th>Tür</th><th>Değer Önizleme</th></tr></thead>
          <tbody>
            ${c.secrets_found.map(s => `
              <tr>
                <td><strong class="secret-key-text">${escapeHtml(s.key)}</strong></td>
                <td><span class="secret-kind-badge">${escapeHtml(s.secret_type)}</span></td>
                <td><code>${escapeHtml(s.value_preview)}</code></td>
              </tr>
            `).join("")}
          </tbody>
        </table>
      `}
    </div>
  `;
}

function renderDriftTab(c, t, escapeHtml) {
  return `
    <div class="inspector-section">
      <div class="drift-info-card">
        <h4>Overlay2 Katman Yapısı</h4>
        <p><strong>UpperDir (Diff):</strong> <code>${escapeHtml(c.upper_dir || "Bulunamadı")}</code></p>
        <p><strong>MergedDir:</strong> <code>${escapeHtml(c.merged_dir || "-")}</code></p>
        <p><strong>WorkDir:</strong> <code>${escapeHtml(c.work_dir || "-")}</code></p>
        <small class="field-hint">UpperDir katmanı, konteyner çalışmaya başladıktan sonra saldırgan veya sistem tarafından değiştirilen / silinen / eklenen tüm dosyaları barındırır.</small>
      </div>
    </div>
  `;
}

function renderLogsTab(d, t, escapeHtml) {
  return `
    <div class="inspector-section">
      <div class="log-viewer-box">
        ${d.loadingLogs ? "<p>Loglar yükleniyor...</p>" : (
          !d.containerLogs || d.containerLogs.length === 0 ?
          "<p class='text-muted'>Konteyner için kaydedilmiş günlük bulunamadı.</p>" :
          d.containerLogs.map(l => `<div class="log-line"><code>${escapeHtml(typeof l === "string" ? l : JSON.stringify(l))}</code></div>`).join("")
        )}
      </div>
    </div>
  `;
}

export async function handleDockerAction(e, { apiRequest, setRoute, render }) {
  const target = e.target.closest("[data-docker-action]");
  if (!target) return;

  const action = target.dataset.dockerAction;

  if (action === "set-mode") {
    dockerState.mode = target.dataset.mode || "local";
    render();
  } else if (action === "set-filter") {
    dockerState.filter = target.dataset.filter || "all";
    render();
  } else if (action === "search") {
    dockerState.search = target.value || "";
    render();
  } else if (action === "scan-local") {
    await scanLocalDocker({ apiRequest, render });
  } else if (action === "scan-remote") {
    await scanRemoteDocker({ apiRequest, render });
  } else if (action === "inspect") {
    const cid = target.dataset.id;
    const containers = dockerState.mode === "remote" ? dockerState.remoteContainers : dockerState.localContainers;
    const found = containers.find(c => c.id === cid);
    if (found) {
      dockerState.selectedContainer = found;
      dockerState.selectedTab = "overview";
      dockerState.containerLogs = [];
      render();
      // Arka planda logları yükle
      loadContainerLogs(cid, apiRequest, render);
    }
  } else if (action === "set-tab") {
    dockerState.selectedTab = target.dataset.tab || "overview";
    render();
  } else if (action === "close-modal") {
    dockerState.selectedContainer = null;
    render();
  } else if (action === "acquire") {
    const cid = target.dataset.id;
    const cname = target.dataset.name || "container";
    await startDockerAcquisition(cid, cname, { apiRequest, setRoute, render });
  }
}

async function scanLocalDocker({ apiRequest, render }) {
  const rootInput = document.querySelector("#docker-custom-root");
  if (rootInput) dockerState.customRoot = rootInput.value.trim();

  dockerState.isScanning = true;
  dockerState.lastAction = "Yerel tarama başlatıldı...";
  render();

  try {
    const statusRes = await apiRequest("/api/docker-status", "POST", {
      custom_docker_root: dockerState.customRoot || null,
    });
    if (statusRes?.status) {
      dockerState.localStatus = statusRes.status;
    }

    const containersRes = await apiRequest("/api/docker-containers", "POST", {
      custom_docker_root: dockerState.customRoot || null,
    });

    dockerState.localScanned = true;
    if (containersRes?.containers) {
      dockerState.localContainers = containersRes.containers;
      dockerState.lastAction = `Tarandı: ${containersRes.containers.length} konteyner bulundu`;
      showToast(`Docker taraması tamamlandı: ${containersRes.containers.length} konteyner bulundu.`, "success");
    } else {
      dockerState.localContainers = [];
      dockerState.lastAction = "Konteyner bulunamadı";
      showToast(containersRes?.error || "Docker konteynerleri okunamadı.", "warning");
    }
  } catch (err) {
    dockerState.lastAction = "Tarama hatası";
    showToast(`Docker tarama hatası: ${err.message || err}`, "error");
  } finally {
    dockerState.isScanning = false;
    render();
  }
}

async function scanRemoteDocker({ apiRequest, render }) {
  const ipInput = document.querySelector("#docker-remote-ip");
  const portInput = document.querySelector("#docker-remote-port");
  const tokenInput = document.querySelector("#docker-remote-token");

  if (ipInput) dockerState.remote.ip = ipInput.value.trim();
  if (portInput) dockerState.remote.port = parseInt(portInput.value) || 4444;
  if (tokenInput) dockerState.remote.token = tokenInput.value;

  if (!dockerState.remote.ip) {
    showToast("Lütfen uzak sunucu IP adresini girin.", "warning");
    return;
  }

  dockerState.isScanning = true;
  dockerState.lastAction = `Uzak Agent (${dockerState.remote.ip}) bağlanılıyor...`;
  render();

  try {
    const payload = {
      ip: dockerState.remote.ip,
      port: dockerState.remote.port,
      token: dockerState.remote.token || null,
    };

    const statusRes = await apiRequest("/api/docker-remote-status", "POST", payload);
    if (statusRes?.status) {
      dockerState.remoteStatus = statusRes.status;
    }

    const containersRes = await apiRequest("/api/docker-remote-containers", "POST", payload);
    dockerState.remoteScanned = true;
    dockerState.remote.connected = true;

    if (containersRes?.containers) {
      dockerState.remoteContainers = containersRes.containers;
      dockerState.lastAction = `Uzak tarama başarılı: ${containersRes.containers.length} konteyner`;
      showToast(`Uzak Docker taraması başarılı: ${containersRes.containers.length} konteyner bulundu.`, "success");
    } else {
      dockerState.remoteContainers = [];
      dockerState.lastAction = "Uzak konteyner bulunamadı";
      showToast(containersRes?.error || "Uzak Docker konteynerleri okunamadı.", "warning");
    }
  } catch (err) {
    dockerState.remote.connected = false;
    dockerState.lastAction = "Uzak bağlantı hatası";
    showToast(`Uzak Docker bağlantı hatası: ${err.message || err}`, "error");
  } finally {
    dockerState.isScanning = false;
    render();
  }
}

async function loadContainerLogs(containerId, apiRequest, render) {
  dockerState.loadingLogs = true;
  try {
    const isRemote = dockerState.mode === "remote";
    const endpoint = isRemote ? "/api/docker-remote-logs" : "/api/docker-logs";
    const payload = isRemote ? {
      ip: dockerState.remote.ip,
      port: dockerState.remote.port,
      token: dockerState.remote.token || null,
      container_id: containerId,
      tail: 200,
    } : {
      container_id: containerId,
      custom_docker_root: dockerState.customRoot || null,
      tail: 200,
    };

    const res = await apiRequest(endpoint, "POST", payload);
    if (res?.logs) {
      dockerState.containerLogs = res.logs;
    }
  } catch (e) {
    console.error("Log error:", e);
  } finally {
    dockerState.loadingLogs = false;
    render();
  }
}

async function startDockerAcquisition(containerId, containerName, { apiRequest, setRoute, render }) {
  dockerState.isAcquiring = true;
  dockerState.lastAction = `Edinim başlatılıyor: ${containerName}`;
  try {
    const isRemote = dockerState.mode === "remote";
    const endpoint = isRemote ? "/api/docker-remote-acquire" : "/api/docker-acquire-local";
    const payload = isRemote ? {
      ip: dockerState.remote.ip,
      port: dockerState.remote.port,
      token: dockerState.remote.token || null,
      container_id: containerId,
      container_name: containerName,
      acquire_diff: true,
      acquire_logs: true,
      acquire_config: true,
    } : {
      container_id: containerId,
      acquire_diff: true,
      acquire_logs: true,
      acquire_config: true,
      custom_docker_root: dockerState.customRoot || null,
    };

    const res = await apiRequest(endpoint, "POST", payload);
    if (res?.durum === "ok" || res?.is_id) {
      dockerState.lastAction = `Delil edinildi: ${containerName}`;
      showToast(`Docker delil edinimi tamamlandı/başlatıldı (İş ID: ${res.is_id})`, "success");
      dockerState.selectedContainer = null;
      render();
    } else {
      dockerState.lastAction = "Edinim başarısız";
      showToast(res?.error || "Docker edinimi başlatılamadı.", "error");
    }
  } catch (err) {
    dockerState.lastAction = "Edinim hatası";
    showToast(`Edinim hatası: ${err.message || err}`, "error");
  } finally {
    dockerState.isAcquiring = false;
    render();
  }
}
