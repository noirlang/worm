export function androidPage({ t, icon, pageTitle, state, escapeHtml, backendReady }) {
  return `
    <section class="page">
      ${pageTitle(t("hub.android.title"), t("hub.android.desc"), "android")}
      <div class="tool-grid android-mode-grid">
// TODO: Enable physical acquisition card, implement EDL/BROM device scan dropdown and physical image trigger
        ${androidImageModeCard("physical", t("android.mode.physical.title"), t("android.mode.physical.desc"), "disk", "var(--text)", t("android.mode.soon"), icon, escapeHtml, { disabled: true })}
        ${androidImageModeCard("logical", t("android.mode.logical.title"), t("android.mode.logical.desc"), "android", "var(--text)", t("android.mode.logical.badge"), icon, escapeHtml)}
        ${androidImageModeCard("filesystem", t("android.mode.filesystem.title"), t("android.mode.filesystem.desc"), "folder", "var(--text)", t("android.mode.filesystem.badge"), icon, escapeHtml)}
        ${androidImageModeCard("ram", t("android.mode.ram.title"), t("android.mode.ram.desc"), "cpu", "var(--text)", t("android.mode.ram.badge"), icon, escapeHtml)}
        ${androidImageModeCard("remote", t("android.mode.remote.title"), t("android.mode.remote.desc"), "wifi", "var(--text)", t("android.mode.remote.badge"), icon, escapeHtml)}
      </div>
    </section>
  `;
}

export function androidModePage({ modeId, t, icon, pageTitle, state, escapeHtml, backendReady, casePanel, field }) {
  const mode = androidMode(modeId, t);
  const android = state.android || {};
  const status = android.adbStatus || null;
  const installed = Boolean(status?.installed);
  const devices = Array.isArray(android.devices) ? android.devices : [];
  const selectedDevice = selectedAndroidDevice(devices, android.selectedDevice);
  const selected = selectedDevice?.serial || "";
  const selectedReady = isReadyAndroidDevice(selectedDevice);
  const statusTitle = status
    ? installed ? t("android.adb.installed") : t("android.adb.missing")
    : t("android.adb.unknown");
  const statusDetail = status?.message || (backendReady() ? t("android.adb.checkHint") : t("android.appModeRequired"));
  const deviceProfile = android.deviceProfile || null;
  const capabilities = android.capabilities || null;
  const acquisitionProfile = android.logicalProfile || "full_logical";

  const isLogical = modeId === "logical";
  const isFilesystem = modeId === "filesystem";
  const isRam = modeId === "ram";
  const isRemote = modeId === "remote";
  const job = isLogical 
    ? (android.logicalJob || null) 
    : (isFilesystem ? (android.filesystemJob || null) : (android.ramJob || null));
  const isRunning = job?.status === "running";
  const isPaused = job?.status === "paused";
  const isActive = isRunning || isPaused;
  const isDone = job?.status === "completed";
  const isFailed = job?.status === "failed";
  const progressValue = job && job.total > 0 ? Math.round((job.done / job.total) * 100) : 0;
  const progressClass = progressValue >= 50 ? " progress-bar is-past-half" : " progress-bar";

  return `
    <section class="page">
      <button class="secondary-button android-back-button" data-route="android">${icon("grid")} ${t("android.back")}</button>
      ${pageTitle(mode.title, mode.desc, mode.icon)}
      <div class="workflow-layout">
        <div class="workflow-panel">
          <p class="section-label">${t("android.adb.title")}</p>
          <div class="side-info">
            <span class="metric-icon">${icon(installed ? "shield" : "android")}</span>
            <span>
              <strong>${statusTitle}</strong>
              <small>${escapeHtml(statusDetail)}</small>
            </span>
          </div>
          <div class="button-row" style="margin-top:12px">
            <button class="primary-button" data-action="android-adb-check">${icon("android")} ${t("android.adb.check")}</button>
            ${installed ? "" : `<button class="secondary-button" data-action="android-adb-install">${icon("download")} ${t("android.adb.install")}</button>`}
            ${installed ? `<button class="secondary-button" data-action="android-list-devices">${icon("refresh")} ${t("android.devices.list")}</button>` : ""}
          </div>

          <div class="section-divider"></div>
          <p class="section-label">${t("android.devices.title")}</p>
          <div class="field">
            <label>${t("android.devices.select")}</label>
            <select class="select" data-android-device-select ${devices.length ? "" : "disabled"}>
              ${deviceOptions(devices, selected, t, escapeHtml)}
            </select>
          </div>
          <div class="button-row" style="margin-top:12px">
            <button class="secondary-button" data-action="android-profile-fetch" ${selectedReady ? "" : "disabled"}>${icon("search")} ${t("android.profile.fetch")}</button>
          </div>
          ${deviceProfile ? `
            <div class="log-box" style="margin-top:12px">
              ${deviceProfileSummary(deviceProfile, t, escapeHtml)}
            </div>
          ` : ""}
          ${capabilities ? capabilitySummary(capabilities, modeId, t, escapeHtml) : ""}

          ${isLogical ? `
            <div class="section-divider"></div>
            <p class="section-label">${t("android.logical.caseTitle")}</p>
            ${casePanel("android", t("android.logical.caseHint"))}

            <div class="section-divider"></div>
            <p class="section-label">${t("android.acquisition.profileTitle")}</p>
            <div class="field">
              <label>${t("android.acquisition.profile")}</label>
              <select class="select" data-android-acquisition-profile>
                ${acquisitionProfileOptions(acquisitionProfile, t)}
              </select>
            </div>

            <div class="section-divider"></div>
            <p class="section-label">${t("android.logical.acquisitionTitle")}</p>
            <div class="button-row">
              <button class="primary-button" data-action="android-start-logical" ${isActive || !selectedReady ? "disabled" : ""}>${icon("android")} ${t("android.logical.start")}</button>
              ${isRunning ? `<button class="secondary-button" data-action="android-pause-logical">${icon("pause")} ${t("workflow.pause")}</button>` : ""}
              ${isPaused ? `<button class="secondary-button" data-action="android-resume-logical">${icon("play")} ${t("workflow.resume")}</button>` : ""}
              ${isActive ? `<button class="danger-button" data-action="android-stop-logical">${icon("stop")} ${t("android.logical.stop")}</button>` : ""}
            </div>

            <div class="section-divider"></div>
            <p class="section-label">${t("android.logical.progress")}</p>
            <div class="${progressClass.trim()}" data-progress style="--value:${progressValue}%"><span></span><b>${progressValue}%</b></div>
            <div class="log-box" id="android-log">${androidLogContent(android, t, modeId)}</div>
          ` : ""}

          ${isFilesystem ? `
            <div class="section-divider"></div>
            <p class="section-label">${t("android.filesystem.caseTitle") || "Vaka Notları"}</p>
            ${casePanel("android", t("android.filesystem.caseHint") || "Dosya sistemi imajı için vaka detayı belirtin.")}

            <div class="section-divider"></div>
            <p class="section-label">${t("android.filesystem.options") || "Seçenekler"}</p>
            <div class="field" style="flex-direction: row; align-items: center; gap: 10px;">
              <input type="checkbox" id="android-filesystem-has-root" data-android-filesystem-has-root style="width: 18px; height: 18px; cursor: pointer;" />
              <label for="android-filesystem-has-root" style="cursor: pointer; user-select: none; font-size: 0.9rem; color: #acc0e4;">${t("android.filesystem.hasRoot") || "Cihazda Root Yetkisi Var (Doğrudan imaj al)"}</label>
            </div>

            <div class="section-divider"></div>
            <p class="section-label">${t("android.filesystem.acquisitionTitle") || "Aktarım"}</p>
            <div class="button-row">
              <button class="primary-button" data-action="android-start-filesystem" ${isActive || !selectedReady ? "disabled" : ""}>${icon("folder")} ${t("android.filesystem.start") || "Dosya Sistem İmajını Al"}</button>
              ${isRunning ? `<button class="secondary-button" data-action="android-pause-filesystem">${icon("pause")} ${t("workflow.pause")}</button>` : ""}
              ${isPaused ? `<button class="secondary-button" data-action="android-resume-filesystem">${icon("play")} ${t("workflow.resume")}</button>` : ""}
              ${isActive ? `<button class="danger-button" data-action="android-stop-filesystem">${icon("stop")} ${t("android.logical.stop")}</button>` : ""}
            </div>

            <div class="section-divider"></div>
            <p class="section-label">${t("android.logical.progress")}</p>
            <div class="${progressClass.trim()}" data-progress style="--value:${progressValue}%"><span></span><b>${progressValue}%</b></div>
            <div class="log-box" id="android-log">${androidLogContent(android, t, modeId)}</div>
          ` : ""}

          ${isRam ? `
            <div class="section-divider"></div>
            <p class="section-label">${t("android.ram.caseTitle") || "Vaka Notları"}</p>
            ${casePanel("android", t("android.ram.caseHint") || "RAM imajı için vaka detayı belirtin.")}

            <div class="section-divider"></div>
            <p class="section-label">${t("android.ram.options") || "Seçenekler"}</p>
            <div class="field">
              <label>${t("android.ram.mode")}</label>
              <select class="select" data-android-ram-mode>
                ${ramModeOptions(android.ramMode || "volatile_data", t)}
              </select>
            </div>
            <div class="field" style="flex-direction: row; align-items: center; gap: 10px;">
              <input type="checkbox" id="android-ram-has-root" data-android-ram-has-root style="width: 18px; height: 18px; cursor: pointer;" />
              <label for="android-ram-has-root" style="cursor: pointer; user-select: none; font-size: 0.9rem; color: #acc0e4;">${t("android.filesystem.hasRoot") || "Cihazda Root Yetkisi Var"}</label>
            </div>

            ${(android.ramMode || "volatile_data") === "physical_memory_probe" ? lemonPreflightPanel(android.lemonPreflight, selected, t, icon, escapeHtml) : ""}

            <div class="section-divider"></div>
            <p class="section-label">${t("android.ram.acquisitionTitle") || "Aktarım"}</p>
            <div class="button-row">
              <button class="primary-button" data-action="android-start-ram" ${isActive || !selectedReady ? "disabled" : ""}>${icon("cpu")} ${t("android.ram.start") || "RAM İmajını Al"}</button>
              ${isRunning ? `<button class="secondary-button" data-action="android-pause-ram">${icon("pause")} ${t("workflow.pause")}</button>` : ""}
              ${isPaused ? `<button class="secondary-button" data-action="android-resume-ram">${icon("play")} ${t("workflow.resume")}</button>` : ""}
              ${isActive ? `<button class="danger-button" data-action="android-stop-ram">${icon("stop")} ${t("android.logical.stop")}</button>` : ""}
            </div>

            <div class="section-divider"></div>
            <p class="section-label">${t("android.logical.progress")}</p>
            <div class="${progressClass.trim()}" data-progress style="--value:${progressValue}%"><span></span><b>${progressValue}%</b></div>
            <div class="log-box" id="android-log">${androidLogContent(android, t, modeId)}</div>
          ` : ""}

          ${isRemote ? remoteAndroidPanel(android, selected, selectedReady, t, icon, escapeHtml) : ""}
        </div>

        <aside class="side-panel">
          <h3>${t("android.side.status")}</h3>
          ${sideInfo(t("android.side.adb"), statusTitle, installed ? "shield" : "android", icon)}
          ${sideInfo(t("android.side.device"), selected || t("android.devices.none"), "android", icon)}
          ${deviceProfile ? sideInfo(t("android.side.profile"), sideProfileSummary(deviceProfile, t), "info", icon) : ""}
          ${(isLogical || isFilesystem || isRam) && job ? sideInfo(t("android.side.lastAction"), job.message || "—", "clock", icon) : ""}
          ${(isLogical || isFilesystem || isRam) && isDone && job.result ? sideInfo(t("android.side.totalBytes"), formatBytes(job.result.total_bytes || 0), "disk", icon) : ""}
          ${isRemote && android.remoteConnectResult ? sideInfo(t("android.remote.status") || "Bağlantı", android.remoteConnectResult.message || "—", android.remoteConnectResult.success ? "shield" : "android", icon) : ""}
        </aside>
      </div>
    </section>
  `;
}

function acquisitionProfileOptions(selected, t) {
  const options = [
    ["quick_logical", t("android.acquisition.quick")],
    ["full_logical", t("android.acquisition.full")],
    ["root_logical", t("android.acquisition.root")],
    ["volatile", t("android.acquisition.volatile")]
  ];
  return options
    .map(([value, label]) => `<option value="${value}"${value === selected ? " selected" : ""}>${label}</option>`)
    .join("");
}

function deviceProfileSummary(profile, t, escapeHtml) {
  const rows = [
    [t("android.profile.model"), profile.model || profile.device || "—"],
    [t("android.profile.api"), profile.api_level || "—"],
    [t("android.profile.selinux"), profile.selinux || "—"],
    [t("android.profile.encryption"), profile.encryption || "—"],
    [t("android.profile.root"), profile.is_rooted ? t("android.profile.rooted") : t("android.profile.notRooted")]
  ];
  return rows
    .map(([label, value]) => `<div class="tree-node"><strong>${escapeHtml(label)}</strong><span>${escapeHtml(value)}</span></div>`)
    .join("");
}

function capabilitySummary(report, modeId, t, escapeHtml) {
  const entries = capabilityEntriesForMode(report, modeId, t);
  if (!entries.length) return "";
  return `
    <p class="section-label" style="margin-top:12px">${t("android.preflight.title")}</p>
    <div class="log-box">
      ${entries.map(([label, check]) => capabilityRow(label, check, t, escapeHtml)).join("")}
    </div>
  `;
}

function capabilityEntriesForMode(report, modeId, t) {
  const common = [[t("android.capability.adb"), report.adb_authorized]];
  if (modeId === "filesystem") {
    return common.concat([
      [t("android.capability.filesystemNonRoot"), report.filesystem_non_root],
      [t("android.capability.filesystemRoot"), report.filesystem_root],
    ]).filter(([, check]) => Boolean(check));
  }
  if (modeId === "ram") {
    return common.concat([
      [t("android.capability.volatileMemory"), report.volatile_memory],
      [t("android.capability.processMemory"), report.process_memory_root],
      [t("android.capability.lemonMemory"), report.lemon_physical_memory],
    ]).filter(([, check]) => Boolean(check));
  }
  return common.concat([
    [t("android.capability.logical"), report.logical_acquisition],
    [t("android.capability.sharedStorage"), report.shared_storage],
    [t("android.capability.bugreport"), report.bugreport],
    [t("android.capability.backup"), report.adb_backup],
  ]).filter(([, check]) => Boolean(check));
}

function capabilityRow(label, check, t, escapeHtml) {
  const level = String(check?.level || "unsupported");
  const pillClass = level === "supported" ? "ok" : (level === "partial" ? "warn" : "danger");
  const pillText = level === "supported"
    ? t("android.capability.supported")
    : (level === "partial" ? t("android.capability.partial") : t("android.capability.unsupported"));
  const detail = check?.recommendation || check?.reason || "";
  return `
    <div class="tree-node">
      <strong>${escapeHtml(label)}</strong>
      <span>
        <span class="status-pill ${pillClass}">${escapeHtml(pillText)}</span>
        ${detail ? `<small>${escapeHtml(detail)}</small>` : ""}
      </span>
    </div>
  `;
}

function sideProfileSummary(profile, t) {
  const model = profile.model || profile.device || t("unknown");
  const root = profile.is_rooted ? t("android.profile.rooted") : t("android.profile.notRooted");
  return `${model} · API ${profile.api_level || "?"} · ${root}`;
}

function sideInfo(title, body, iconName, icon) {
  return `
    <div class="side-info">
      <span class="metric-icon">${icon(iconName)}</span>
      <span><strong>${title}</strong><small>${body}</small></span>
    </div>
  `;
}

function androidLogContent(android, t, modeId) {
  const log = modeId === "logical" 
    ? (android.logicalLog || []) 
    : (modeId === "filesystem" ? (android.filesystemLog || []) : (android.ramLog || []));
  if (!log.length) return `• ${t("android.logical.waiting")}`;
  return log.map((line) => `• ${line}`).join("<br />");
}

function formatBytes(bytes) {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0)} ${units[i]}`;
}

function selectedAndroidDevice(devices, selectedSerial) {
  const list = Array.isArray(devices) ? devices : [];
  const selected = list.find((device) => device.serial === selectedSerial);
  return selected || list.find(isReadyAndroidDevice) || list[0] || null;
}

function isReadyAndroidDevice(device) {
  return String(device?.state || "").toLowerCase() === "device";
}

function requireReadyAndroidDevice(state, t, showToast) {
  const device = selectedAndroidDevice(state.android?.devices || [], state.android?.selectedDevice);
  if (!device?.serial) {
    showToast(t("android.logical.deviceRequired"), "warning");
    return null;
  }
  if (!isReadyAndroidDevice(device)) {
    showToast(t("android.devices.notReady", { state: device.state || t("unknown") }), "warning");
    return null;
  }
  if (!state.android) state.android = {};
  state.android.selectedDevice = device.serial;
  return device;
}

function androidMode(modeId, t) {
  const modes = {
    physical: {
      title: t("android.mode.physical.title"),
      desc: t("android.mode.physical.desc"),
      icon: "disk"
    },
    logical: {
      title: t("android.mode.logical.title"),
      desc: t("android.mode.logical.desc"),
      icon: "android"
    },
    filesystem: {
      title: t("android.mode.filesystem.title"),
      desc: t("android.mode.filesystem.desc"),
      icon: "folder"
    },
    ram: {
      title: t("android.mode.ram.title"),
      desc: t("android.mode.ram.desc"),
      icon: "cpu"
    },
    remote: {
      title: t("android.mode.remote.title") || "Remote Android / MESH",
      desc: t("android.mode.remote.desc") || "TCP/IP ADB veya MESH üzerinden uzak cihaza bağlan",
      icon: "wifi"
    }
  };
  return modes[modeId] || modes.logical;
}

function ramModeOptions(selected, t) {
  const options = [
    ["volatile_data", t("android.ram.mode.volatile")],
    ["root_process_memory", t("android.ram.mode.rootProcess")],
    ["physical_memory_probe", t("android.ram.mode.physicalProbe")]
  ];
  return options
    .map(([value, label]) => `<option value="${value}"${value === selected ? " selected" : ""}>${label}</option>`)
    .join("");
}

function androidImageModeCard(modeId, title, desc, iconName, accent, badge, icon, escapeHtml, options = {}) {
  const disabled = options.disabled ? " disabled aria-disabled=\"true\"" : "";
  const route = options.disabled ? "" : ` data-route="android:${modeId}"`;
  const disabledClass = options.disabled ? " is-disabled" : "";
  return `
    <button class="forensic-card${disabledClass}"${route}${disabled} style="--accent:${accent}">
      <span class="card-icon">${icon(iconName)}</span>
      <h3>${escapeHtml(title)}</h3>
      <p>${escapeHtml(desc)}</p>
      <span class="meta">${escapeHtml(badge)}</span>
    </button>
  `;
}

export async function handleAndroidAction(button, deps) {
  const action = button.dataset.action;
  if (action === "android-adb-check") {
    await checkAdb(button, deps);
    return true;
  }
  if (action === "android-adb-install") {
    await installAdb(button, deps);
    return true;
  }
  if (action === "android-list-devices") {
    await listDevices(button, deps);
    return true;
  }
  if (action === "android-profile-fetch") {
    await fetchDeviceProfile(button, deps);
    return true;
  }
  if (action === "android-start-logical") {
    await startLogicalAcquisition(button, deps);
    return true;
  }
  if (action === "android-stop-logical") {
    await stopLogicalAcquisition(button, deps);
    return true;
  }
  if (action === "android-pause-logical") {
    await controlAndroidAcquisition(button, "logical", "pause", deps);
    return true;
  }
  if (action === "android-resume-logical") {
    await controlAndroidAcquisition(button, "logical", "resume", deps);
    return true;
  }
  if (action === "android-start-filesystem") {
    await startFilesystemAcquisition(button, deps);
    return true;
  }
  if (action === "android-stop-filesystem") {
    await stopFilesystemAcquisition(button, deps);
    return true;
  }
  if (action === "android-pause-filesystem") {
    await controlAndroidAcquisition(button, "filesystem", "pause", deps);
    return true;
  }
  if (action === "android-resume-filesystem") {
    await controlAndroidAcquisition(button, "filesystem", "resume", deps);
    return true;
  }
  if (action === "android-start-ram") {
    await startRamAcquisition(button, deps);
    return true;
  }
  if (action === "android-stop-ram") {
    await stopRamAcquisition(button, deps);
    return true;
  }
  if (action === "android-pause-ram") {
    await controlAndroidAcquisition(button, "ram", "pause", deps);
    return true;
  }
  if (action === "android-resume-ram") {
    await controlAndroidAcquisition(button, "ram", "resume", deps);
    return true;
  }
  if (action === "android-remote-connect") {
    await connectRemoteAndroid(button, deps);
    return true;
  }
  if (action === "android-remote-disconnect") {
    await disconnectRemoteAndroid(button, deps);
    return true;
  }
  if (action === "android-lemon-preflight") {
    await runLemonPreflight(button, deps);
    return true;
  }
  return false;
}

export function syncAndroidDeviceSelection(select, { state, t, showToast }) {
  if (!state.android) state.android = {};
  state.android.selectedDevice = select.value;
  state.android.deviceProfile = null;
  state.android.capabilities = null;
  state.android.session = null;
  if (select.value) {
    const device = selectedAndroidDevice(state.android.devices || [], select.value);
    const type = isReadyAndroidDevice(device) ? "success" : "warning";
    showToast(t("android.devices.selected", { serial: select.value }), type);
  }
}

async function checkAdb(button, { apiRequest, backendReady, state, t, showToast, render }) {
  if (!backendReady()) {
    showToast(t("android.appModeRequired"), "warning");
    return;
  }

  button.disabled = true;
  try {
    const status = await apiRequest("/api/android-adb-status");
    if (!state.android) state.android = {};
    state.android.adbStatus = status;
    if (!status.installed) {
      state.android.devices = [];
      state.android.selectedDevice = "";
    }
    render();
    showToast(status.installed ? t("android.adb.installed") : t("android.adb.missing"), status.installed ? "success" : "error");
  } catch (error) {
    showToast(t("android.adb.checkFailed", { message: error.message }), "error");
  } finally {
    button.disabled = false;
  }
}

async function installAdb(button, { apiRequest, backendReady, state, t, showToast, render }) {
  if (!backendReady()) {
    showToast(t("android.appModeRequired"), "warning");
    return;
  }

  button.disabled = true;
  try {
    showToast(t("android.adb.installing"), "info");
    const result = await apiRequest("/api/android-adb-install", { method: "POST", body: "{}" });
    if (!state.android) state.android = {};
    state.android.adbStatus = result.status || {
      installed: Boolean(result.installed),
      message: result.message || t("android.adb.installed")
    };
    if (!state.android.adbStatus.installed) {
      state.android.devices = [];
      state.android.selectedDevice = "";
    }
    render();
    showToast(t("android.adb.installDone"), "success");
  } catch (error) {
    showToast(t("android.adb.installFailed", { message: error.message }), "error");
  } finally {
    button.disabled = false;
  }
}

async function listDevices(button, { apiRequest, backendReady, state, t, showToast, render }) {
  if (!backendReady()) {
    showToast(t("android.appModeRequired"), "warning");
    return;
  }
  if (!state.android?.adbStatus?.installed) {
    showToast(t("android.adb.checkFirst"), "warning");
    return;
  }

  button.disabled = true;
  try {
    const result = await apiRequest("/api/android-devices");
    const devices = Array.isArray(result.devices) ? result.devices : [];
    if (!state.android) state.android = {};
    state.android.devices = devices;
    state.android.selectedDevice = selectedAndroidDevice(devices, state.android.selectedDevice)?.serial || "";
    state.android.deviceProfile = null;
    state.android.capabilities = null;
    state.android.session = null;
    render();
    showToast(devices.length
      ? t("android.devices.listed", { count: String(devices.length) })
      : t("android.devices.none"),
      devices.length ? "success" : "warning"
    );
  } catch (error) {
    showToast(t("android.devices.listFailed", { message: error.message }), "error");
  } finally {
    button.disabled = false;
  }
}

async function fetchDeviceProfile(button, { apiRequest, backendReady, state, t, showToast, render }) {
  if (!backendReady()) {
    showToast(t("android.appModeRequired"), "warning");
    return;
  }
  const device = requireReadyAndroidDevice(state, t, showToast);
  if (!device) {
    return;
  }

  button.disabled = true;
  try {
    const result = await apiRequest("/api/android-device-profile", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ serial: device.serial }),
    });
    if (!state.android) state.android = {};
    state.android.deviceProfile = result.profile || null;
    state.android.capabilities = result.capabilities || null;
    state.android.session = result.session || null;
    render();
    showToast(t("android.profile.loaded"), "success");
  } catch (error) {
    showToast(t("android.profile.failed", { message: error.message }), "error");
  } finally {
    button.disabled = false;
  }
}

async function startLogicalAcquisition(button, { apiRequest, backendReady, state, t, showToast, render, resolveCase }) {
  if (!backendReady()) {
    showToast(t("android.appModeRequired"), "warning");
    return;
  }
  const device = requireReadyAndroidDevice(state, t, showToast);
  if (!device) {
    return;
  }

  const caseName = resolveCase?.() || null;
  const profileSelect = document.querySelector("[data-android-acquisition-profile]");
  const profile = profileSelect?.value || "full_logical";
  if (!state.android) state.android = {};
  state.android.logicalProfile = profile;
  state.android.logicalLog = [t("android.logical.starting")];
  state.android.logicalJob = null;
  render();

  button.disabled = true;
  try {
    const body = { serial: device.serial, profile };
    if (caseName) body.case_name = caseName;
    const result = await apiRequest("/api/android-profile-acquisition", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (result.job_id) {
      state.android.logicalJob = { job_id: result.job_id, status: "running", done: 0, total: 0, message: t("android.logical.starting") };
      render();
      pollLogicalJob(result.job_id, { apiRequest, state, t, showToast, render });
    }
  } catch (error) {
    state.android.logicalLog.push(`❌ ${error.message}`);
    showToast(t("android.logical.failed", { message: error.message }), "error");
    render();
  } finally {
    button.disabled = false;
  }
}

async function stopLogicalAcquisition(button, { apiRequest, backendReady, state, t, showToast, render }) {
  await controlAndroidAcquisition(button, "logical", "stop", { apiRequest, backendReady, state, t, showToast, render });
}

function pollLogicalJob(jobId, { apiRequest, state, t, showToast, render }) {
  const interval = setInterval(async () => {
    try {
      const result = await apiRequest("/api/acquisition-status", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ job_id: jobId }),
      });
      if (!state.android) state.android = {};

      state.android.logicalJob = {
        job_id: jobId,
        status: result.status,
        done: result.done || 0,
        total: result.total || 0,
        message: result.message || "",
        result: result.result || null,
        error: result.error || null,
      };

      // Update log
      if (!state.android.logicalLog) state.android.logicalLog = [];
      const lastMsg = state.android.logicalLog[state.android.logicalLog.length - 1];
      if (result.message && result.message !== lastMsg) {
        state.android.logicalLog.push(result.message);
      }

      if (result.status === "completed") {
        clearInterval(interval);
        state.android.logicalLog.push(`✅ ${t("android.logical.done")}`);
        showToast(t("android.logical.done"), "success");
      } else if (result.status === "failed") {
        clearInterval(interval);
        state.android.logicalLog.push(`❌ ${result.error || t("android.logical.failed", { message: "" })}`);
        showToast(t("android.logical.failed", { message: result.error || "" }), "error");
      }

      render();
    } catch {
      // Silently retry on network hiccup
    }
  }, 1500);
}

async function startFilesystemAcquisition(button, { apiRequest, backendReady, state, t, showToast, render, resolveCase }) {
  if (!backendReady()) {
    showToast(t("android.appModeRequired"), "warning");
    return;
  }
  const device = requireReadyAndroidDevice(state, t, showToast);
  if (!device) {
    return;
  }

  const hasRootCheckbox = document.querySelector("[data-android-filesystem-has-root]");
  const hasRoot = hasRootCheckbox ? Boolean(hasRootCheckbox.checked) : false;

  const caseName = resolveCase?.() || null;
  if (!state.android) state.android = {};
  state.android.filesystemLog = [t("android.filesystem.starting") || "Dosya sistemi aktarımı başlatılıyor..."];
  state.android.filesystemJob = null;
  render();

  button.disabled = true;
  try {
    const body = { 
      serial: device.serial,
      has_root: hasRoot
    };
    if (caseName) body.case_name = caseName;
    const result = await apiRequest("/api/android-filesystem-image", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (result.job_id) {
      state.android.filesystemJob = { job_id: result.job_id, status: "running", done: 0, total: 3, message: "Başlatılıyor..." };
      render();
      pollFilesystemJob(result.job_id, { apiRequest, state, t, showToast, render });
    }
  } catch (error) {
    state.android.filesystemLog.push(`❌ ${error.message}`);
    showToast(error.message, "error");
    render();
  } finally {
    button.disabled = false;
  }
}

async function stopFilesystemAcquisition(button, { apiRequest, backendReady, state, t, showToast, render }) {
  await controlAndroidAcquisition(button, "filesystem", "stop", { apiRequest, backendReady, state, t, showToast, render });
}

function pollFilesystemJob(jobId, { apiRequest, state, t, showToast, render }) {
  const interval = setInterval(async () => {
    try {
      const result = await apiRequest("/api/acquisition-status", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ job_id: jobId }),
      });
      if (!state.android) state.android = {};

      state.android.filesystemJob = {
        job_id: jobId,
        status: result.status,
        done: result.done || 0,
        total: result.total || 0,
        message: result.message || "",
        result: result.result || null,
        error: result.error || null,
      };

      // Update log
      if (!state.android.filesystemLog) state.android.filesystemLog = [];
      const lastMsg = state.android.filesystemLog[state.android.filesystemLog.length - 1];
      if (result.message && result.message !== lastMsg) {
        state.android.filesystemLog.push(result.message);
      }

      if (result.status === "completed") {
        clearInterval(interval);
        state.android.filesystemLog.push(`✅ ${t("android.logical.done")}`);
        showToast(t("android.logical.done"), "success");
      } else if (result.status === "failed") {
        clearInterval(interval);
        state.android.filesystemLog.push(`❌ ${result.error || "Aktarım başarısız"}`);
        showToast(result.error || "Aktarım başarısız", "error");
      }

      render();
    } catch {
      // Silently retry
    }
  }, 1500);
}

async function startRamAcquisition(button, { apiRequest, backendReady, state, t, showToast, render, resolveCase }) {
  if (!backendReady()) {
    showToast(t("android.appModeRequired"), "warning");
    return;
  }
  const device = requireReadyAndroidDevice(state, t, showToast);
  if (!device) {
    return;
  }

  const hasRootCheckbox = document.querySelector("[data-android-ram-has-root]");
  const hasRoot = hasRootCheckbox ? Boolean(hasRootCheckbox.checked) : false;
  const modeSelect = document.querySelector("[data-android-ram-mode]");
  const mode = modeSelect?.value || "volatile_data";

  const caseName = resolveCase?.() || null;
  if (!state.android) state.android = {};
  state.android.ramMode = mode;
  state.android.ramLog = [t("android.ram.starting") || "RAM aktarımı başlatılıyor..."];
  state.android.ramJob = null;
  render();

  button.disabled = true;
  try {
    const body = { 
      serial: device.serial,
      has_root: hasRoot,
      mode
    };
    if (caseName) body.case_name = caseName;
    const result = await apiRequest("/api/android-ram-image", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (result.job_id) {
      state.android.ramJob = { job_id: result.job_id, status: "running", done: 0, total: 3, message: "Başlatılıyor..." };
      render();
      pollRamJob(result.job_id, { apiRequest, state, t, showToast, render });
    }
  } catch (error) {
    state.android.ramLog.push(`❌ ${error.message}`);
    showToast(error.message, "error");
    render();
  } finally {
    button.disabled = false;
  }
}

async function stopRamAcquisition(button, { apiRequest, backendReady, state, t, showToast, render }) {
  await controlAndroidAcquisition(button, "ram", "stop", { apiRequest, backendReady, state, t, showToast, render });
}

async function controlAndroidAcquisition(button, kind, action, { apiRequest, state, t, showToast, render }) {
  const job = state.android?.[`${kind}Job`];
  if (!job?.job_id) return;

  button.disabled = true;
  try {
    const result = await apiRequest("/api/acquisition-control", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ job_id: job.job_id, action }),
    });
    showToast(result.message || t("android.control.sent"), "success");
    render();
  } catch (error) {
    showToast(error.message, "error");
  } finally {
    button.disabled = false;
  }
}

function pollRamJob(jobId, { apiRequest, state, t, showToast, render }) {
  const interval = setInterval(async () => {
    try {
      const result = await apiRequest("/api/acquisition-status", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ job_id: jobId }),
      });
      if (!state.android) state.android = {};

      state.android.ramJob = {
        job_id: jobId,
        status: result.status,
        done: result.done || 0,
        total: result.total || 0,
        message: result.message || "",
        result: result.result || null,
        error: result.error || null,
      };

      // Update log
      if (!state.android.ramLog) state.android.ramLog = [];
      const lastMsg = state.android.ramLog[state.android.ramLog.length - 1];
      if (result.message && result.message !== lastMsg) {
        state.android.ramLog.push(result.message);
      }

      if (result.status === "completed") {
        clearInterval(interval);
        state.android.ramLog.push(`✅ ${t("android.ram.done") || "RAM imajı başarıyla tamamlandı."}`);
        showToast(t("android.ram.done") || "RAM imajı başarıyla tamamlandı.", "success");
      } else if (result.status === "failed") {
        clearInterval(interval);
        state.android.ramLog.push(`❌ ${result.error || "Aktarım başarısız"}`);
        showToast(result.error || "Aktarım başarısız", "error");
      }

      render();
    } catch {
      // Silently retry
    }
  }, 1500);
}

function deviceOptions(devices, selected, t, escapeHtml) {
  if (!devices.length) {
    return `<option value="">${t("android.devices.none")}</option>`;
  }

  return devices
    .map((device) => {
      const serial = device.serial || "";
      const isSelected = serial === selected ? " selected" : "";
      const details = [device.state, device.model, device.product]
        .filter(Boolean)
        .join(" · ");
      const label = details ? `${serial} · ${details}` : serial;
      return `<option value="${escapeHtml(serial)}"${isSelected}>${escapeHtml(label)}</option>`;
    })
    .join("");
}

// ---------------------------------------------------------------------------
// Remote Android / MESH panel bileşenleri
// ---------------------------------------------------------------------------

function remoteAndroidPanel(android, selected, selectedReady, t, icon, escapeHtml) {
  const connectResult = android.remoteConnectResult || null;
  return `
    <div class="section-divider"></div>
    <p class="section-label">${t("android.remote.connectTitle") || "Uzak Cihaz Bağlantısı"}</p>
    <div class="log-box" style="margin-bottom:12px">
      <div class="tree-node">
        <span>${icon("info")}</span>
        <span style="font-size:0.85rem;color:#acc0e4">${escapeHtml(t("android.remote.hint") || "USB cihaz yoksa TCP/IP veya MESH endpoint girerek uzak Android cihaza bağlanabilirsin. Bağlandıktan sonra cihaz otomatik listelenir.")}</span>
      </div>
    </div>
    <div class="field">
      <label>${t("android.remote.host") || "Host / IP"}</label>
      <input class="input" type="text" id="android-remote-host" placeholder="192.168.1.50 veya mesh-peer" style="width:100%;padding:8px;background:var(--input-bg,#1a2540);border:1px solid var(--border,#2a3a5a);border-radius:6px;color:var(--text);" />
    </div>
    <div class="field">
      <label>${t("android.remote.port") || "Port"}</label>
      <input class="input" type="number" id="android-remote-port" value="5555" min="1" max="65535" style="width:120px;padding:8px;background:var(--input-bg,#1a2540);border:1px solid var(--border,#2a3a5a);border-radius:6px;color:var(--text);" />
    </div>
    <div class="field">
      <label>${t("android.remote.kind") || "Tip"}</label>
      <select class="select" id="android-remote-kind">
        <option value="tcp_adb">${t("android.remote.kind.tcp") || "TCP/IP ADB (adb connect)"}</option>
        <option value="mesh_relay">${t("android.remote.kind.mesh") || "MESH Relay Endpoint"}</option>
      </select>
    </div>
    <div class="button-row" style="margin-top:12px">
      <button class="primary-button" data-action="android-remote-connect">${icon("wifi")} ${t("android.remote.connect") || "Bağlan"}</button>
      ${selected && selected.includes(":") ? `<button class="danger-button" data-action="android-remote-disconnect">${icon("stop")} ${t("android.remote.disconnect") || "Bağlantıyı Kes"}</button>` : ""}
    </div>
    ${connectResult ? `
      <div class="log-box" style="margin-top:12px">
        <div class="tree-node">
          <span class="status-pill ${connectResult.success ? 'ok' : 'danger'}">${escapeHtml(connectResult.success ? (t("android.remote.connected") || "Bağlandı") : (t("android.remote.failed") || "Başarısız"))}</span>
          <span style="font-size:0.85rem">${escapeHtml(connectResult.message || "")}</span>
        </div>
      </div>
    ` : ""}
  `;
}

function lemonPreflightPanel(preflight, serial, t, icon, escapeHtml) {
  if (!preflight) {
    return `
      <div class="section-divider"></div>
      <p class="section-label">${t("android.lemon.preflightTitle") || "Lemon Fiziksel RAM Ön Kontrol"}</p>
      <div class="button-row">
        <button class="secondary-button" data-action="android-lemon-preflight" ${serial ? "" : "disabled"}>${icon("search")} ${t("android.lemon.runPreflight") || "Ön Kontrolü Çalıştır"}</button>
      </div>
      <div class="log-box" style="margin-top:8px">
        <div class="tree-node"><span style="font-size:0.85rem;color:#acc0e4">${escapeHtml(t("android.lemon.preflightHint") || "Lemon eBPF tabanlı fiziksel RAM aracıdır. Önce cihaz uygunluğunu kontrol et.")}</span></div>
      </div>
    `;
  }

  const statusPill = preflight.ready
    ? `<span class="status-pill ok">${t("android.lemon.ready") || "Lemon Hazır"}</span>`
    : `<span class="status-pill danger">${t("android.lemon.notReady") || "Uygun Değil"}</span>`;

  const rows = [
    [t("android.lemon.abi") || "Mimari", preflight.abi || "—", preflight.abi_supported ? "ok" : "danger"],
    [t("android.lemon.root") || "Root", preflight.root_available ? (t("android.profile.rooted") || "Var") : (t("android.profile.notRooted") || "Yok"), preflight.root_available ? "ok" : "danger"],
    [t("android.lemon.ebpf") || "eBPF/BTF", preflight.ebpf_btf_available ? (t("common.yes") || "Evet") : (t("common.no") || "Hayır"), preflight.ebpf_btf_available ? "ok" : "warn"],
    [t("android.lemon.kcore") || "/proc/kcore", preflight.kcore_available ? (t("common.yes") || "Evet") : (t("common.no") || "Hayır"), preflight.kcore_available ? "ok" : "warn"],
    [t("android.lemon.storage") || "Depolama", preflight.storage_ok ? (t("common.ok") || "Yeterli") : (t("android.lemon.storageInsuff") || "Yetersiz"), preflight.storage_ok ? "ok" : "danger"],
    [t("android.lemon.ram") || "RAM", preflight.ram_mb ? `${preflight.ram_mb} MB` : "—", ""]
  ];

  return `
    <div class="section-divider"></div>
    <p class="section-label">${t("android.lemon.preflightTitle") || "Lemon Fiziksel RAM Ön Kontrol"}</p>
    <div class="log-box">
      <div class="tree-node" style="margin-bottom:8px">${statusPill}</div>
      ${rows.map(([label, val, pill]) => `
        <div class="tree-node">
          <strong>${escapeHtml(label)}</strong>
          <span>${pill ? `<span class="status-pill ${pill}">${escapeHtml(val)}</span>` : escapeHtml(val)}</span>
        </div>
      `).join("")}
      ${preflight.kernel_version ? `<div class="tree-node"><strong>${t("android.lemon.kernel") || "Kernel"}</strong><span style="font-size:0.8rem">${escapeHtml(preflight.kernel_version)}</span></div>` : ""}
      ${preflight.soc_warning ? `<div class="tree-node"><span class="status-pill warn">⚠ SoC</span><span style="font-size:0.82rem">${escapeHtml(preflight.soc_warning)}</span></div>` : ""}
      ${!preflight.ready && preflight.reason ? `<div class="tree-node" style="margin-top:6px"><span style="color:#f87171;font-size:0.85rem">${escapeHtml(preflight.reason)}</span></div>` : ""}
    </div>
    <div class="button-row" style="margin-top:8px">
      <button class="secondary-button" data-action="android-lemon-preflight" ${serial ? "" : "disabled"}>${icon("search")} ${t("android.lemon.recheck") || "Yeniden Kontrol Et"}</button>
    </div>
  `;
}

// ---------------------------------------------------------------------------
// Remote + Lemon action handler'ları
// ---------------------------------------------------------------------------

async function connectRemoteAndroid(button, { apiRequest, backendReady, state, t, showToast, render }) {
  if (!backendReady()) { showToast(t("android.appModeRequired"), "warning"); return; }
  const hostInput = document.getElementById("android-remote-host");
  const portInput = document.getElementById("android-remote-port");
  const kindSelect = document.getElementById("android-remote-kind");
  const host = hostInput?.value?.trim();
  if (!host) { showToast(t("android.remote.hostRequired") || "Host gerekli", "warning"); return; }
  const port = parseInt(portInput?.value || "5555", 10);
  const kind = kindSelect?.value || "tcp_adb";

  button.disabled = true;
  try {
    const result = await apiRequest("/api/android-remote-connect", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ host, port, kind }),
    });
    if (!state.android) state.android = {};
    state.android.remoteConnectResult = result;
    if (result.success) {
      // Cihazı listeye yenile
      try {
        const devResult = await apiRequest("/api/android-devices");
        state.android.devices = Array.isArray(devResult.devices) ? devResult.devices : [];
        state.android.selectedDevice = result.serial;
      } catch { /* ignore */ }
      showToast(`${t("android.remote.connected") || "Bağlandı"}: ${result.serial}`, "success");
    } else {
      showToast(result.message || t("android.remote.failed") || "Bağlantı başarısız", "error");
    }
    render();
  } catch (error) {
    showToast(error.message, "error");
  } finally {
    button.disabled = false;
  }
}

async function disconnectRemoteAndroid(button, { apiRequest, backendReady, state, t, showToast, render }) {
  if (!backendReady()) { showToast(t("android.appModeRequired"), "warning"); return; }
  const serial = state.android?.selectedDevice;
  if (!serial) return;

  button.disabled = true;
  try {
    await apiRequest("/api/android-remote-disconnect", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ serial }),
    });
    if (!state.android) state.android = {};
    state.android.remoteConnectResult = null;
    state.android.selectedDevice = "";
    state.android.deviceProfile = null;
    state.android.capabilities = null;
    showToast(t("android.remote.disconnected") || "Bağlantı kesildi", "success");
    render();
  } catch (error) {
    showToast(error.message, "error");
  } finally {
    button.disabled = false;
  }
}

async function runLemonPreflight(button, { apiRequest, backendReady, state, t, showToast, render }) {
  if (!backendReady()) { showToast(t("android.appModeRequired"), "warning"); return; }
  const device = selectedAndroidDevice(state.android?.devices || [], state.android?.selectedDevice);
  if (!device?.serial) { showToast(t("android.logical.deviceRequired"), "warning"); return; }

  button.disabled = true;
  try {
    const result = await apiRequest("/api/android-lemon-preflight", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ serial: device.serial }),
    });
    if (!state.android) state.android = {};
    state.android.lemonPreflight = result;
    render();
    showToast(result.ready
      ? (t("android.lemon.readyToast") || "Lemon bu cihazda çalışabilir")
      : (t("android.lemon.notReadyToast") || "Bu cihaz Lemon için uygun değil"),
      result.ready ? "success" : "warning");
  } catch (error) {
    showToast(error.message, "error");
  } finally {
    button.disabled = false;
  }
}
