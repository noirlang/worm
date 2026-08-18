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
      ${field(t("report.signHash"), '<label class="checkbox-row"><input id="report-sign-hash" type="checkbox" checked /><span>' + t("report.signHashDesc") + "</span></label>")}
      <div class="button-row">
        <button class="primary-button" data-action="create-report">${icon("report")} ${t("report.generate")}</button>
        <button class="secondary-button" data-action="list-reports">${icon("refresh")} ${t("report.refresh")}</button>
      </div>
      <div class="status-badge" data-report-status>${icon("info")} ${t("ready")}</div>
      <div class="log-box" data-report-output>${t("report.outputWaiting")}</div>
    `;
  }
  if (tab === "history") {
    return `
      <p class="section-label">${t("history.title")}</p>
      <p class="field-hint">${t("history.hint")}</p>
      <div class="side-info">
        <span class="metric-icon">${icon("clock")}</span>
        <span><strong>${t("history.scope")}</strong><small>${escapeHtml(t("history.scopeAll"))}</small></span>
      </div>
      <div class="button-row">
        <button class="secondary-button" data-action="refresh-history">${icon("refresh")} ${t("history.refresh")}</button>
      </div>
      <div class="history-list" data-history-list>
        <div class="log-box">${t("history.loading")}</div>
      </div>
    `;
  }
  if (tab === "logs") {
    return `
      <p class="section-label">${t("logs.title")}</p>
      <p class="field-hint">${t("logs.hint")}</p>
      <div class="side-info">
        <span class="metric-icon">${icon("clock")}</span>
        <span><strong>${t("logs.scope")}</strong><small>${escapeHtml(state.activeCase?.case_name ? `${state.activeCase.case_name}/gunlukler` : t("logs.activeCaseOnly"))}</small></span>
      </div>
      <div class="button-row">
        <button class="secondary-button" data-action="refresh-logs">${icon("refresh")} ${t("logs.refresh")}</button>
      </div>
      <div class="log-box" data-logs-output>${t("logs.outputWaiting")}</div>
    `;
  }
  return hashPanel(pickerField, field);
}

export function hashPanel(pickerField, field, state, t, icon) {
  const method = state?.hashMethod || "sha256";
  const path = state?.hashTargetInput || "";
  const result = state?.hashResult || null;
  const status = state?.hashStatus || t("ready");

  return `
    <p class="section-label">${t("hash.title")}</p>
    <p class="field-hint">${t("hash.hint")}</p>
    ${pickerField(
      t("hash.targetFile"),
      `<input id="hash-target-path" class="input" placeholder="/path/to/evidence.raw" value="${path}" />`,
      "pick-hash-target"
    )}
    ${field(
      t("hash.algorithm"),
      `<select id="hash-algorithm" class="select">
        <option value="sha256" ${method === "sha256" ? "selected" : ""}>SHA-256 (Önerilen)</option>
        <option value="md5" ${method === "md5" ? "selected" : ""}>MD5</option>
        <option value="both" ${method === "both" ? "selected" : ""}>SHA-256 + MD5</option>
      </select>`
    )}
    <div class="button-row">
      <button class="primary-button" data-action="run-hash">${icon("shield")} ${t("hash.calculate")}</button>
    </div>
    <div class="status-badge" data-hash-status>${icon("info")} ${status}</div>
    <div class="log-box" data-hash-output>
      ${result ? renderHashResult(result, t) : t("hash.outputWaiting")}
    </div>
  `;
}

function renderHashResult(res, t) {
  let out = `<strong>Dosya:</strong> ${res.path}<br/><strong>Boyut:</strong> ${res.file_size_formatted || res.file_size + " B"}<br/>`;
  if (res.sha256) out += `<strong>SHA-256:</strong> <code style="word-break:break-all">${res.sha256}</code><br/>`;
  if (res.md5) out += `<strong>MD5:</strong> <code style="word-break:break-all">${res.md5}</code><br/>`;
  return out;
}

export function settingsPage({ t, icon, state, platformLabel, APP_VERSION }) {
  const isDark = state.theme !== "light";
  const packageLabel = state.updateCheck?.package_type?.toUpperCase() || (state.platform === "windows" ? "MSI" : "APPIMAGE");
  const assetName = state.updateCheck?.asset_name || (state.platform === "windows" ? "amele-windows-x64.msi" : "amele-linux-x64.AppImage");
  const detectedBy = state.updateCheck?.detected_by ? `<span>Tespit: ${state.updateCheck.detected_by}</span>` : "";

  return `
    <section class="page">
      <div class="settings-grid">
        <article class="settings-card">
          <span class="settings-kicker">${t("settings.general")}</span>
          <h3>${t("settings.appearanceLanguage")}</h3>
          <div class="settings-row">
            <span>
              <strong>${t("settings.theme")}</strong>
            </span>
            <button class="secondary-button" data-action="theme-toggle">${isDark ? icon("sun") : icon("moon")} ${isDark ? t("settings.themeLight") : t("settings.themeDark")}</button>
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

export const KNOWN_CONTRIBUTORS = {
  melihemik: {
    key: "melihemik",
    name: "Melih Emik",
    roleKey: "about.role.lead",
    defaultRole: "BDFL",
    photo: "melih-emik.jpg",
    links: [
      ["GitHub", "https://github.com/melihemik"],
      ["LinkedIn", "https://www.linkedin.com/in/melihemik/"],
      ["Website", "https://melihemik.com.tr"]
    ]
  },
  yetece1: {
    key: "yetece1",
    name: "Yusuf Tuncel",
    roleKey: "about.role.windows",
    defaultRole: "Windows Sorumlusu",
    photo: "yusuf-tuncel.jpg",
    links: [
      ["GitHub", "https://github.com/yetece1"],
      ["LinkedIn", "https://www.linkedin.com/in/yusuf-tuncel/"],
      ["Website", "https://yusuftuncel.tr"]
    ]
  },
  kafkaskrtl: {
    key: "kafkaskrtl",
    name: "Muhammet Ali Güner",
    roleKey: "about.role.linux",
    defaultRole: "Linux Sorumlusu",
    photo: "muhammet-ali-guner.jpg",
    links: [
      ["GitHub", "https://github.com/kafkaskrtl"],
      ["LinkedIn", "https://www.linkedin.com/in/muhammetali-g%C3%BCner/"]
    ]
  },
  abdulhalimaltuntas: {
    key: "abdulhalimaltuntas",
    name: "Abdulhalim Altuntaş",
    roleKey: "about.role.android",
    defaultRole: "Android Sorumlusu",
    photo: "abdulhalim.jpg",
    links: [
      ["GitHub", "https://github.com/abdulhalimaltuntas"],
      ["LinkedIn", "https://www.linkedin.com/in/abdulhalim-altunta%C5%9F-7992672b5/"]
    ]
  }
};

export function renderContributors(contributors, t, icon, assetPath) {
  const list = (contributors && contributors.length > 0) ? contributors : [
    KNOWN_CONTRIBUTORS.melihemik,
    KNOWN_CONTRIBUTORS.yetece1,
    KNOWN_CONTRIBUTORS.kafkaskrtl,
    KNOWN_CONTRIBUTORS.abdulhalimaltuntas
  ];

  return list.map(c => {
    const roleText = c.roleKey ? t(c.roleKey) : (c.role || "Developer");
    const avatarSrc = c.photo ? (c.photo.startsWith("http") ? c.photo : `${assetPath}/contributors/${c.photo}`) : `${assetPath}/contributors/melih-emik.jpg`;
    return `
      <article class="contributor-card">
        <img class="avatar" src="${avatarSrc}" alt="${c.name}" onerror="this.src='${assetPath}/contributors/melih-emik.jpg'" />
        <h3>${c.name}</h3>
        <p>${roleText}</p>
        <div class="social-row" aria-label="${c.name} bağlantıları">
          ${(c.links || []).map(([label, url]) => socialLink(label, url, icon)).join("")}
        </div>
      </article>
    `;
  }).join("");
}

export function aboutPage({ t, icon, APP_VERSION, assetPath, theme, state }) {
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
        ${capabilityCard(t("home.windows.title"), t("home.windows.desc"), "windows", "var(--text)", icon)}
        ${capabilityCard(t("home.linux.title"), t("home.linux.desc"), "linux", "var(--text)", icon)}
        ${capabilityCard(t("home.docker.title"), t("home.docker.desc"), "docker", "var(--text)", icon)}
        ${capabilityCard(t("home.android.title"), t("home.android.desc"), "android", "var(--text)", icon)}
        ${capabilityCard(t("home.ios.title"), t("home.ios.desc"), "ios", "var(--text)", icon)}
        ${capabilityCard(t("home.agent.title"), t("home.agent.desc"), "network", "var(--text)", icon)}
        ${capabilityCard(t("home.other.title"), t("home.other.desc"), "tiles", "var(--text)", icon)}
      </div>

      <h2 class="section-heading">${t("about.maintainers")}</h2>
      <div class="contributor-grid">
        ${renderContributors(state?.contributors, t, icon, assetPath)}
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

function socialLink(label, url, icon) {
  const key = label === "LinkedIn" ? "linkedin" : label === "Website" ? "website" : "github";
  return `<a class="social-button" href="${url}" target="_blank" rel="noopener noreferrer" aria-label="${label}">${icon(key)}</a>`;
}
