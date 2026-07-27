export function otherPage({ t, icon, state, pageTitle, pickerField, field, escapeHtml, caseSelectOptions, detailPanel }) {
  return `
    <section class="page">
      ${pageTitle(t("other.title"), t("other.desc"), "tiles")}
      <div class="other-grid">
        ${simpleCard(t("other.hash.title"), t("other.hash.desc"), "shield", "hash", icon, t)}
        ${simpleCard(t("other.evidence.title"), t("other.evidence.desc"), "scale", "evidence", icon, t)}
        ${simpleCard(t("other.reports.title"), t("other.reports.desc"), "report", "reports", icon, t)}
        ${simpleCard(t("other.history.title"), t("other.history.desc"), "clock", "history", icon, t)}
        ${simpleCard(t("other.logs.title"), t("other.logs.desc"), "clock", "logs", icon, t)}
      </div>
      <div id="other-detail" class="workflow-panel" style="margin-top:16px">${detailPanel(state.activeTab)}</div>
    </section>
  `;
}

function simpleCard(title, desc, iconName, tab, icon, t) {
  return `
    <button class="forensic-card" data-tab="${tab}">
      <span class="card-icon">${icon(iconName)}</span>
      <h3>${title}</h3>
      <p>${desc}</p>
      <span class="meta">${t("open")}</span>
    </button>
  `;
}

export function detailPanel({ tab, t, icon, state, pickerField, field, escapeHtml, caseSelectOptions, hashPanel }) {
  if (tab === "evidence") {
    return `
      <p class="section-label">${t("case.management")}</p>
      <div class="side-info">
        <span class="metric-icon">${icon("folder")}</span>
        <span><strong>${t("case.location")}</strong><small data-case-base>${escapeHtml(state.caseBaseDir || "~/Amele/Vakalar")}</small></span>
      </div>
      <p class="field-hint">${t("case.fixedLocation")}</p>
      ${field(t("case.name"), '<input id="case-name" class="input" placeholder="Case_2026_001" />')}
      <div class="button-row">
        <button class="primary-button" data-action="create-case">${icon("folder")} ${t("case.create")}</button>
        <button class="secondary-button" data-action="refresh-cases">${icon("refresh")} ${t("case.refresh")}</button>
        <button class="secondary-button" data-action="create-manifest">${icon("shield")} ${t("case.manifest.create")}</button>
      </div>
      <div class="status-badge" data-case-status>${icon("info")} ${state.activeCase ? t("case.created", { path: state.activeCase.case_dir }) : t("case.notCreated")}</div>
      <div class="status-badge" data-manifest-status>${icon("shield")} ${state.activeCase?.manifest_path ? t("case.manifest.ready", { path: escapeHtml(state.activeCase.manifest_path) }) : t("case.manifest.waiting")}</div>
      <div class="section-divider"></div>
      <p class="section-label">${t("case.files")}</p>
      ${field(t("case.folder"), `<select id="case-folder" class="select"><option value="ciktilar">${t("case.outputs")}</option><option value="disk_imajlari">${t("case.diskImages")}</option><option value="ram">${t("case.ram")}</option><option value="android">${t("case.android")}</option><option value="raporlar">${t("case.reports")}</option><option value="hash">${t("case.hash")}</option><option value="notlar">${t("case.notes")}</option><option value="gunlukler">${t("case.logs")}</option></select>`)}
      ${field(t("case.file"), `<select id="case-file-list" class="select"><option>${t("case.listFilesPlaceholder")}</option></select>`)}
      <div class="button-row">
        <button class="secondary-button" data-action="list-files">${icon("search")} ${t("case.listFiles")}</button>
      </div>
    `;
  }
  if (tab === "reports") {
    return `
      <p class="section-label">${t("report.createTitle")}</p>
      <p class="field-hint">${t("report.hint")}</p>
      ${field(t("report.case"), `<select id="report-case" class="select" data-case-select data-allow-new-case="1">${caseSelectOptions(state.activeCase?.case_name, { allowNew: true })}</select>`)}
      ${field(t("report.title"), `<input id="report-title" class="input" value="${t("report.defaultTitle")}" />`)}
      ${field(t("report.format"), '<select id="report-format" class="select"><option value="txt">TXT</option><option value="json">JSON</option></select>')}
      ${field(t("report.note"), `<textarea id="report-note" class="textarea" placeholder="${t("report.notePlaceholder")}"></textarea>`)}
      <div class="button-row">
        <button class="secondary-button" data-action="refresh-cases">${icon("refresh")} ${t("case.refresh")}</button>
        <button class="secondary-button" data-action="add-note">${icon("report")} ${t("report.addNote")}</button>
        <button class="primary-button" data-action="create-report">${icon("report")} ${t("report.createTitle")}</button>
      </div>
      <div class="status-badge" data-report-status>${icon("info")} ${t("ready")}</div>
    `;
  }
  if (tab === "history") {
    const history = Array.isArray(state.acquisitionHistory) ? state.acquisitionHistory : [];
    return `
      <p class="section-label">${t("other.history.title")}</p>
      <p class="field-hint">${t("acquisition.history.hint")}</p>
      ${field(t("report.case"), `<select id="history-case" class="select" data-case-select>${caseSelectOptions(state.activeCase?.case_name)}</select>`)}
      <div class="button-row">
        <button class="secondary-button" data-action="refresh-cases">${icon("refresh")} ${t("case.refresh")}</button>
        <button class="primary-button" data-action="load-history">${icon("clock")} ${t("acquisition.history.load")}</button>
      </div>
      <div class="acquisition-history-list">
        ${history.length ? history.map((item) => historyItem(item, t, icon, escapeHtml)).join("") : `<div class="log-box">${t("acquisition.history.empty")}</div>`}
      </div>
    `;
  }
  if (tab === "logs") {
    return `
      <p class="section-label">${t("other.logs.title")}</p>
      <p class="field-hint">${t("log.live")}</p>
      <div class="log-box">${state.lastLog.map((line) => escapeHtml(line)).join("<br />")}</div>
      <div class="button-row" style="margin-top:12px">
        <button class="secondary-button" data-action="refresh-log">${icon("refresh")} ${t("log.refreshFromFile")}</button>
      </div>
    `;
  }
  return hashPanel({ t, icon, pickerField, field });
}

function historyItem(item, t, icon, escapeHtml) {
  const platform = item.platform === "ios" ? "ios" : "android";
  const statusClass = item.status === "completed" ? "ok" : "warn";
  const statusText = item.status === "completed" ? t("acquisition.history.completed") : t("acquisition.history.warnings");
  const metrics = [
    item.total_entries != null ? `${t("acquisition.history.entries")}: ${item.total_entries}` : "",
    item.success_count != null ? `${t("acquisition.history.success")}: ${item.success_count}` : "",
    item.error_count != null ? `${t("acquisition.history.errors")}: ${item.error_count}` : "",
    item.total_bytes != null ? formatHistoryBytes(Number(item.total_bytes || 0)) : ""
  ].filter(Boolean).join(" · ");
  return `
    <article class="acquisition-history-item">
      <span class="metric-icon">${icon(platform)}</span>
      <div>
        <div class="history-title-row">
          <strong>${escapeHtml(item.title || "-")}</strong>
          <span class="status-pill ${statusClass}">${escapeHtml(statusText)}</span>
        </div>
        <small>${escapeHtml(item.subtitle || "-")}</small>
        <small>${escapeHtml(item.generated_at || "-")}</small>
        <small class="path-text">${escapeHtml(item.output_dir || item.relative_output || "-")}</small>
        ${metrics ? `<small>${escapeHtml(metrics)}</small>` : ""}
      </div>
    </article>
  `;
}

function formatHistoryBytes(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / Math.pow(1024, index)).toFixed(index > 0 ? 1 : 0)} ${units[index]}`;
}

export function hashPanel({ t, icon, pickerField, field }) {
  return `
    <p class="section-label">${t("hash.calculator")}</p>
    ${pickerField(t("hash.file"), "hash-file", t("hash.selectFile"), "file")}
    <div class="button-row">
      <button class="primary-button" data-action="hash">${icon("shield")} ${t("hash.calculate")}</button>
    </div>
    <div class="hash-grid">
      ${hashResult("MD5", "md5")}
      ${hashResult("SHA1", "sha1")}
      ${hashResult("SHA256", "sha256")}
      ${hashResult("SHA512", "sha512")}
    </div>
    <div class="section-divider"></div>
    <p class="section-label">${t("hash.compare")}</p>
    ${field(t("hash.value"), `<input class="input" data-hash-expected placeholder="${t("hash.placeholder")}" />`)}
    <div class="button-row">
      <button class="secondary-button" data-action="compare">${icon("search")} ${t("hash.compare")}</button>
    </div>
    <div class="side-info" data-hash-compare-result>
      <span class="metric-icon">${icon("info")}</span>
      <span><strong>${t("hash.result")}</strong><small>${t("hash.waiting")}</small></span>
    </div>
  `;
}

function hashResult(label, key) {
  return `
    <div class="hash-result" data-hash-result="${key}">
      <small>${label}</small>
      <strong>-</strong>
    </div>
  `;
}

export function settingsPage({ t, icon, state, platformLabel, APP_VERSION, escapeHtml }) {
  const updateTarget = state.latestUpdate?.update_target || state.updateTarget || {};
  const updateAsset = state.latestUpdate?.platform_asset || {};
  const packageLabel = escapeHtml(updateTarget.asset_package_label || updateTarget.package_label || t("settings.packageAuto"));
  const assetName = escapeHtml(updateAsset.name || t("settings.assetAuto"));
  const detectedBy = updateTarget.detected_by ? `<span>${t("settings.detectedBy")}: ${escapeHtml(updateTarget.detected_by)}</span>` : "";
  return `
    <section class="page">
      <div class="settings-header">
        <h1>${t("settings.title")}</h1>
      </div>
      <div class="settings-layout">
        <article class="settings-card settings-primary">
          <span class="settings-kicker">${t("settings.appearance")}</span>
          <h3>${t("settings.appSettings")}</h3>
          <div class="settings-row">
            <span>
              <strong>${t("settings.darkTheme")}</strong>
            </span>
            <button class="switch ${state.theme === "dark" ? "on" : ""}" data-action="theme-toggle" aria-label="${t("settings.darkTheme")}"></button>
          </div>
          <div class="settings-row">
            <span>
              <strong>${t("settings.language")}</strong>
            </span>
            <select class="select compact-select" data-action="language-select" aria-label="${t("settings.language")}">
              <option value="tr" ${state.language === "tr" ? "selected" : ""}>Türkçe</option>
              <option value="en" ${state.language === "en" ? "selected" : ""}>English</option>
            </select>
          </div>
          <div class="settings-row">
            <span>
              <strong>${t("settings.detectedSystem")}</strong>
            </span>
            <span class="status-badge">${icon(state.platform === "windows" ? "windows" : state.platform === "linux" ? "linux" : "monitor")} ${platformLabel(state.platform)}</span>
          </div>
          <div class="button-row">
            <button class="primary-button" data-action="save-settings">${t("settings.save")}</button>
          </div>
          <div class="status-badge" data-settings-status>${icon("info")} ${t("ready")}</div>
        </article>

        <article class="settings-card settings-update">
          <span class="settings-kicker">${t("settings.version")}</span>
          <h3>${t("settings.update")}</h3>
          <div class="settings-meta">
            <span>${t("settings.installed")}: ${APP_VERSION}</span>
            <span>${t("settings.package")}: ${packageLabel}</span>
            ${detectedBy}
            <span>Asset: ${assetName}</span>
          </div>
          <div class="progress-bar" data-update-progress style="--value:0%"><span></span><b>0%</b></div>
          <div class="button-row">
            <button class="primary-button" data-action="check-update">${icon("refresh")} ${t("settings.checkUpdate")}</button>
            <button class="secondary-button" data-action="download-update">${icon("download")} ${t("settings.downloadInstall")}</button>
          </div>
          <div class="status-badge" data-update-status>${icon("info")} ${t("ready")}</div>
          <div class="log-box compact-log" data-update-log>${t("settings.releaseNotes")}</div>
        </article>
      </div>
    </section>
  `;
}

export function aboutPage({ t, icon, APP_VERSION, assetPath, theme }) {
  const logoFile = theme === "light" ? "logo-siyah.png" : "logo.png";
  return `
    <section class="page">
      <div class="about-hero">
        <span class="about-logo"><img src="${assetPath}/logo/${logoFile}" alt="Amele logo" /></span>
        <div>
          <h1>Amele Forensic Tool</h1>
          <span class="status-badge">${t("about.version", { version: APP_VERSION })}</span>
          <p>${t("about.desc")}</p>
        </div>
      </div>

      <h2 class="section-heading">${t("about.capabilities")}</h2>
      <div class="capability-grid">
        ${capabilityCard(t("about.collect.title"), t("about.collect.desc"), "disk", "var(--text)", icon)}
        ${capabilityCard(t("about.prove.title"), t("about.prove.desc"), "shield", "var(--text)", icon)}
        ${capabilityCard(t("about.package.title"), t("about.package.desc"), "report", "var(--text)", icon)}
      </div>

      <h2 class="section-heading">${t("about.maintainers")}</h2>
      <div class="contributor-grid">
        ${contributorCard("ME", "Melih Emik", t("about.role.lead"), "melih-emik.jpg", [
          ["GitHub", "https://github.com/favilances"],
          ["LinkedIn", "https://www.linkedin.com/in/melihemik/"],
          ["Website", "https://melihemik.com.tr"]
        ], assetPath, icon)}
        ${contributorCard("YT", "Yusuf Tuncel", t("about.role.windows"), "yusuf-tuncel.jpg", [
          ["GitHub", "https://github.com/yetece1"],
          ["LinkedIn", "https://www.linkedin.com/in/yusuf-tuncel/"],
          ["Website", "https://yusuftuncel.tr"]
        ], assetPath, icon)}
        ${contributorCard("MG", "Muhammet Ali Güner", t("about.role.linux"), "muhammet-ali-guner.jpg", [
          ["GitHub", "https://github.com/kafkaskrtl"],
          ["LinkedIn", "https://www.linkedin.com/in/muhammetali-g%C3%BCner/"]
        ], assetPath, icon)}
        ${contributorCard("AA", "Abdulhalim Altuntaş", t("about.role.android"), "abdulhalim.jpg", [
          ["GitHub", "https://github.com/abdulhalimaltuntas"],
          ["LinkedIn", "https://www.linkedin.com/in/abdulhalim-altunta%C5%9F-7992672b5/"]
        ], assetPath, icon)}
      </div>

      <div class="company-logo-card">
        <img src="${assetPath}/logo/sirket.png" alt="Şirket logosu" />
      </div>
    </section>
  `;
}

function capabilityCard(title, desc, iconName, accent, icon) {
  return `
    <article class="forensic-card" style="--accent:${accent};cursor:default">
      <span class="card-icon">${icon(iconName)}</span>
      <h3>${title}</h3>
      <p>${desc}</p>
    </article>
  `;
}

function contributorCard(initials, name, role, photo, links, assetPath, icon) {
  return `
    <article class="contributor-card">
      <img class="avatar" src="${assetPath}/contributors/${photo}" alt="${name}" />
      <h3>${name}</h3>
      <p>${role}</p>
      <div class="social-row" aria-label="${name} bağlantıları">
        ${links.map(([label, url]) => socialLink(label, url, icon)).join("")}
      </div>
    </article>
  `;
}

function socialLink(label, url, icon) {
  const key = label === "LinkedIn" ? "linkedin" : label === "Website" ? "website" : "github";
  return `<a class="social-button" href="${url}" target="_blank" rel="noopener noreferrer" aria-label="${label}">${icon(key)}</a>`;
}
