export function iosPage({ t, icon, pageTitle, state, escapeHtml, backendReady, casePanel, field }) {
  const ios = state.ios || {};
  const backupPath = ios.backupPath || "";
  const profile = ios.profile || null;
  const job = ios.normalizeJob || null;
  const isRunning = job?.status === "running";
  const isPaused = job?.status === "paused";
  const isActive = isRunning || isPaused;
  const encrypted = Boolean(profile?.encrypted);
  const progressValue = job && job.total > 0 ? Math.round((job.done / job.total) * 100) : 0;
  const progressClass = progressValue >= 50 ? " progress-bar is-past-half" : " progress-bar";
  const startDisabled = isActive || encrypted || !backendReady() ? "disabled" : "";

  return `
    <section class="page">
      ${pageTitle(t("hub.ios.title"), t("hub.ios.desc"), "ios")}
      <div class="workflow-layout">
        <div class="workflow-panel">
          <p class="section-label">${t("ios.backup.title")}</p>
          ${field(t("ios.backup.folder"), `
            <div class="input-action">
              <input id="ios-backup-path" class="input" value="${escapeHtml(backupPath)}" placeholder="${escapeHtml(t("ios.backup.placeholder"))}" />
              <button class="secondary-button" data-action="pick-folder" data-target="#ios-backup-path">${icon("folder")} ${t("select")}</button>
            </div>
          `)}
          <div class="button-row" style="margin-top:12px">
            <button class="secondary-button" data-action="ios-load-profile">${icon("search")} ${t("ios.profile.load")}</button>
          </div>

          ${profile ? iosProfilePanel(profile, t, icon, escapeHtml) : `
            <div class="log-box" style="margin-top:12px">• ${escapeHtml(t("ios.profile.waiting"))}</div>
          `}

          <div class="section-divider"></div>
          <p class="section-label">${t("ios.case.title")}</p>
          ${casePanel("ios", t("ios.case.hint"))}

          <div class="section-divider"></div>
          <p class="section-label">${t("ios.hash.title")}</p>
          <div class="ios-hash-row">
            ${hashCheck("md5", "MD5", ios, true)}
            ${hashCheck("sha1", "SHA-1", ios, true)}
            ${hashCheck("sha256", "SHA-256", ios, true)}
          </div>

          <div class="section-divider"></div>
          <p class="section-label">${t("ios.normalize.title")}</p>
          ${encrypted ? `<div class="error-panel">${escapeHtml(t("ios.encrypted.warning"))}</div>` : ""}
          <div class="button-row">
            <button class="primary-button" data-action="ios-start-normalize" ${startDisabled}>${icon("ios")} ${t("ios.normalize.start")}</button>
            ${isRunning ? `<button class="secondary-button" data-action="ios-pause-normalize">${icon("pause")} ${t("workflow.pause")}</button>` : ""}
            ${isPaused ? `<button class="secondary-button" data-action="ios-resume-normalize">${icon("play")} ${t("workflow.resume")}</button>` : ""}
            ${isActive ? `<button class="danger-button" data-action="ios-stop-normalize">${icon("stop")} ${t("workflow.stopLabel")}</button>` : ""}
          </div>

          <div class="section-divider"></div>
          <p class="section-label">${t("ios.progress.title")}</p>
          <div class="${progressClass.trim()}" data-progress style="--value:${progressValue}%"><span></span><b>${progressValue}%</b></div>
          <div class="log-box" id="ios-log">${iosLogContent(ios, t, escapeHtml)}</div>
        </div>

        <aside class="side-panel">
          <h3>${t("android.side.status")}</h3>
          ${sideInfo(t("ios.side.backup"), backupPath || t("ios.side.noBackup"), "folder", icon, escapeHtml)}
          ${sideInfo(t("ios.side.encryption"), profile ? (encrypted ? t("ios.encrypted.yes") : t("ios.encrypted.no")) : t("unknown"), encrypted ? "key" : "shield", icon, escapeHtml)}
          ${profile ? sideInfo(t("ios.side.device"), deviceSummary(profile, t), "ios", icon, escapeHtml) : ""}
          ${job ? sideInfo(t("android.side.lastAction"), job.message || "-", "clock", icon, escapeHtml) : ""}
          ${job?.result?.output_dir ? sideInfo(t("ios.side.output"), job.result.output_dir, "folder", icon, escapeHtml) : ""}
          ${job?.result?.total_bytes ? sideInfo(t("android.side.totalBytes"), formatBytes(job.result.total_bytes), "disk", icon, escapeHtml) : ""}
        </aside>
      </div>
    </section>
  `;
}

export async function handleIosAction(button, deps) {
  const action = button.dataset.action;
  if (action === "ios-load-profile") {
    await loadIosProfile(button, deps);
    return true;
  }
  if (action === "ios-start-normalize") {
    await startIosNormalize(button, deps);
    return true;
  }
  if (action === "ios-pause-normalize") {
    await controlIosNormalize(button, "pause", deps);
    return true;
  }
  if (action === "ios-resume-normalize") {
    await controlIosNormalize(button, "resume", deps);
    return true;
  }
  if (action === "ios-stop-normalize") {
    await controlIosNormalize(button, "stop", deps);
    return true;
  }
  return false;
}

export function syncIosBackupPathInput(input, state) {
  if (!state.ios) state.ios = {};
  state.ios.backupPath = input.value.trim();
  state.ios.profile = null;
}

async function loadIosProfile(button, { apiRequest, backendReady, state, t, showToast, render }) {
  if (!backendReady()) {
    showToast(t("workflow.appModeRequired"), "warning");
    return;
  }
  const backupPath = currentBackupPath();
  if (!backupPath) {
    showToast(t("ios.backup.required"), "warning");
    return;
  }

  button.disabled = true;
  try {
    const result = await apiRequest("/api/ios-backup-profile", {
      method: "POST",
      body: JSON.stringify({ backup_path: backupPath })
    });
    if (!state.ios) state.ios = {};
    state.ios.backupPath = backupPath;
    state.ios.profile = result.info || null;
    state.ios.normalizeLog = [t("ios.profile.loaded")];
    render();
    showToast(t("ios.profile.loaded"), "success");
  } catch (error) {
    showToast(t("ios.profile.failed", { message: error.message }), "error");
  } finally {
    button.disabled = false;
  }
}

async function startIosNormalize(button, { apiRequest, backendReady, state, t, showToast, render, resolveCase }) {
  if (!backendReady()) {
    showToast(t("workflow.appModeRequired"), "warning");
    return;
  }
  const backupPath = currentBackupPath();
  if (!backupPath) {
    showToast(t("ios.backup.required"), "warning");
    return;
  }
  if (state.ios?.profile?.encrypted) {
    showToast(t("ios.encrypted.warning"), "warning");
    return;
  }

  const hashAlgorithms = selectedHashes();
  const caseName = resolveCase?.() || null;
  if (!state.ios) state.ios = {};
  state.ios.backupPath = backupPath;
  state.ios.hashAlgorithms = hashAlgorithms;
  state.ios.normalizeLog = [t("ios.normalize.starting")];
  state.ios.normalizeJob = null;
  render();

  button.disabled = true;
  try {
    const body = { backup_path: backupPath, hash_algorithms: hashAlgorithms };
    if (caseName) body.case_name = caseName;
    const result = await apiRequest("/api/ios-backup-normalize", {
      method: "POST",
      body: JSON.stringify(body)
    });
    if (!result.job_id) throw new Error(t("workflow.jobIdMissing"));
    state.ios.normalizeJob = {
      job_id: result.job_id,
      status: "running",
      done: 0,
      total: 0,
      message: t("ios.normalize.starting")
    };
    render();
    pollIosJob(result.job_id, { apiRequest, state, t, showToast, render });
  } catch (error) {
    state.ios.normalizeLog.push(`! ${error.message}`);
    showToast(t("ios.normalize.failed", { message: error.message }), "error");
    render();
  } finally {
    button.disabled = false;
  }
}

async function controlIosNormalize(button, action, { apiRequest, state, t, showToast, render }) {
  const job = state.ios?.normalizeJob;
  if (!job?.job_id) return;

  button.disabled = true;
  try {
    const result = await apiRequest("/api/acquisition-control", {
      method: "POST",
      body: JSON.stringify({ job_id: job.job_id, action })
    });
    showToast(result.message || t("android.control.sent"), "success");
    render();
  } catch (error) {
    showToast(error.message, "error");
  } finally {
    button.disabled = false;
  }
}

function pollIosJob(jobId, { apiRequest, state, t, showToast, render }) {
  const interval = setInterval(async () => {
    try {
      const result = await apiRequest("/api/acquisition-status", {
        method: "POST",
        body: JSON.stringify({ job_id: jobId })
      });
      if (!state.ios) state.ios = {};
      state.ios.normalizeJob = {
        job_id: jobId,
        status: result.status,
        done: result.done || 0,
        total: result.total || 0,
        message: result.message || "",
        result: result.result || null,
        error: result.error || null
      };
      state.ios.normalizeLog = Array.isArray(result.logs) && result.logs.length
        ? result.logs.slice(-120)
        : (state.ios.normalizeLog || []);

      if (result.status === "completed") {
        clearInterval(interval);
        state.ios.normalizeLog.push(t("ios.normalize.done"));
        showToast(t("ios.normalize.done"), "success");
      } else if (result.status === "failed") {
        clearInterval(interval);
        state.ios.normalizeLog.push(result.error || t("ios.normalize.failed", { message: "" }));
        showToast(t("ios.normalize.failed", { message: result.error || "" }), "error");
      }
      render();
    } catch {
      // Polling transient hatalarını sessiz tekrar dene.
    }
  }, 1200);
}

function iosProfilePanel(profile, t, icon, escapeHtml) {
  const encrypted = profile.encrypted
    ? `<span class="status-pill danger">${escapeHtml(t("ios.encrypted.yes"))}</span>`
    : `<span class="status-pill ok">${escapeHtml(t("ios.encrypted.no"))}</span>`;
  const rows = [
    [t("ios.profile.device"), profile.device_name || "-"],
    [t("ios.profile.model"), profile.model || profile.product_type || "-"],
    [t("ios.profile.version"), profile.ios_version || "-"],
    [t("ios.profile.serial"), profile.serial_number || "-"],
    [t("ios.profile.udid"), profile.unique_device_id || "-"],
    [t("ios.profile.files"), profile.file_count == null ? "-" : String(profile.file_count)],
    [t("ios.profile.apps"), String(profile.installed_apps_count || 0)]
  ];
  return `
    <div class="analysis-summary ios-profile-summary">
      <p class="section-label">${icon(profile.encrypted ? "key" : "shield")} ${t("ios.profile.title")}</p>
      <div class="summary-grid">
        <div><strong>${escapeHtml(t("ios.profile.encrypted"))}</strong><span>${encrypted}</span></div>
        ${rows.map(([label, value]) => `<div><strong>${escapeHtml(label)}</strong><span>${escapeHtml(value)}</span></div>`).join("")}
      </div>
    </div>
  `;
}

function hashCheck(value, label, ios, fallback) {
  const selected = Array.isArray(ios.hashAlgorithms)
    ? ios.hashAlgorithms.includes(value)
    : fallback;
  return `
    <label class="check-row ios-hash-option">
      <input type="checkbox" data-ios-hash="${value}" ${selected ? "checked" : ""} />
      <span>${label}</span>
    </label>
  `;
}

function selectedHashes() {
  const values = [...document.querySelectorAll("[data-ios-hash]")]
    .filter((input) => input.checked)
    .map((input) => input.dataset.iosHash)
    .filter(Boolean);
  return values.length ? values : ["sha256"];
}

function currentBackupPath() {
  return document.querySelector("#ios-backup-path")?.value.trim() || "";
}

function iosLogContent(ios, t, escapeHtml) {
  const log = Array.isArray(ios.normalizeLog) ? ios.normalizeLog : [];
  if (!log.length) return `• ${escapeHtml(t("ios.normalize.waiting"))}`;
  return log.map((line) => `• ${escapeHtml(line)}`).join("<br />");
}

function deviceSummary(profile, t) {
  const model = profile.model || profile.product_type || t("unknown");
  const version = profile.ios_version ? `iOS ${profile.ios_version}` : "iOS";
  return `${model} · ${version}`;
}

function sideInfo(title, body, iconName, icon, escapeHtml) {
  return `
    <div class="side-info">
      <span class="metric-icon">${icon(iconName)}</span>
      <span><strong>${escapeHtml(title)}</strong><small>${escapeHtml(String(body || "-"))}</small></span>
    </div>
  `;
}

function formatBytes(bytes) {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0)} ${units[i]}`;
}
