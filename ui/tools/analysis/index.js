export function analysisPage({ t, icon, state, pageTitle, pickerField, caseSelectOptions }) {
  const activeTab = state.activeAnalysisTab || "image";
  return `
    <section class="page">
      ${pageTitle(t("analysis.title"), t("analysis.desc"), "search")}
      
      <div class="analysis-tabs">
        <button class="analysis-tab-btn ${activeTab === "image" ? "active" : ""}" data-analysis-tab="image">
          ${icon("disk")} ${t("analysis.tabImage")}
        </button>
        <button class="analysis-tab-btn ${activeTab === "ram" ? "active" : ""}" data-analysis-tab="ram">
          ${icon("shield")} ${t("analysis.tabRam")}
        </button>
        <button class="analysis-tab-btn ${activeTab === "android" ? "active" : ""}" data-analysis-tab="android">
          ${icon("android")} ${t("analysis.tabAndroid")}
        </button>
      </div>

      <div id="analysis-content">
        ${activeTab === "image" ? renderImageAnalysis({ t, icon, state, pickerField }) : ""}
        ${activeTab === "ram" ? renderRamAnalysis({ t, icon, state, pickerField }) : ""}
        ${activeTab === "android" ? renderAndroidAnalysis({ t, icon, state, caseSelectOptions }) : ""}
      </div>
    </section>
  `;
}

function renderImageAnalysis({ t, icon, state, pickerField }) {
  const activeMount = state.imageMount?.label || state.imageMount?.mountDir || t("analysis.noImage");
  const defaultPath = state.imagePathInput || ".img, .dd, .raw, .iso ...";
  return `
    <div class="workflow-panel">
      <p class="section-label">${t("analysis.tabImage")}</p>
      <p class="field-hint">${t("analysis.hint")}</p>
      ${pickerField(t("analysis.imageFile"), "image-path", defaultPath, "file")}
      <div class="button-row">
        <button class="primary-button" data-action="image-analyze">${icon("search")} ${t("analysis.btnDiskSummary")}</button>
        <button class="primary-button" data-action="mount-readonly">${icon("disk")} ${t("analysis.mount")}</button>
        <button class="danger-button" data-action="unmount-image">${icon("stop")} ${t("analysis.unmount")}</button>
      </div>
      <div class="section-divider"></div>
      <div class="side-info">
        <span class="metric-icon">${icon("info")}</span>
        <span><strong>${t("analysis.status")}</strong><small data-analysis-status>${activeMount}</small></span>
      </div>
      <div data-analysis-log style="${state.imageMountLogHTML ? "" : "display:none"}">
        ${state.imageMountLogHTML || ""}
      </div>
      
      <div class="forensic-split">
        <div class="tree-panel">
          <div class="panel-header">
            <h3>📁 Klasör Yapısı / Directory Tree</h3>
          </div>
          <div class="file-tree-container" id="image-tree-root">
            ${state.imageMountTreeHTML || `<div class="log-box">${t("analysis.outputWaiting")}</div>`}
          </div>
        </div>
        <div class="preview-panel">
          <div class="panel-header">
            <h3>📄 Dosya Önizleme / File Preview</h3>
          </div>
          <div class="file-tree-container" id="image-file-preview" style="background: rgba(0,0,0,0.12)">
            <div class="log-box" style="display:flex;align-items:center;justify-content:center;color:var(--muted);text-align:center;padding:20px">
              Klasör yapısında bir dosyaya tıklayarak içeriğini inceleyebilirsiniz.<br/>Click a file on the left to preview it.
            </div>
          </div>
        </div>
      </div>
      <div id="disk-analysis-results" class="workflow-panel" style="display:none;padding:16px;margin-top:14px"></div>
    </div>
  `;
}

function renderRamAnalysis({ t, icon, state, pickerField }) {
  const defaultPath = state.ramAnalysisPathInput || ".raw, .bin, .mem ...";
  const osProfile = state.ramOsProfile || "windows";
  const symbolDir = state.ramSymbolDirInput || ".symbols";
  return `
    <div class="workflow-panel">
      <p class="section-label">${t("analysis.tabRam")}</p>
      <p class="field-hint">${t("analysis.ramHint")}</p>
      ${pickerField(t("analysis.ramFile"), "ram-analysis-path", defaultPath, "file")}
      
      <div style="margin-top: 14px; max-width: 400px; display: flex; flex-direction: column; gap: 4px;">
        <span style="font-size: 11px; font-weight: 600; color: var(--muted); text-transform: uppercase; letter-spacing: 0.5px;">İşletim Sistemi Profili / OS Profile</span>
        <select id="ram-os-profile" class="select" style="background: rgba(0,0,0,0.2); border: 1px solid var(--line); color: var(--text); padding: 8px 12px; border-radius: 6px; font-size: 13px; outline: none; width: 100%;">
          <option value="windows"${osProfile === "windows" ? " selected" : ""}>Windows (Volatility3)</option>
          <option value="linux"${osProfile === "linux" ? " selected" : ""}>Linux (Volatility3)</option>
        </select>
      </div>

      <div style="margin-top: 14px;">
        ${pickerField(t("analysis.symbolDir"), "ram-symbol-dir", symbolDir, "folder")}
      </div>

      <div class="button-row" style="margin-top:14px">
        <button class="secondary-button" data-action="ram-preflight">${icon("info")} ${t("analysis.btnPreflight")}</button>
        <button class="secondary-button" data-action="ram-symbol-install">${icon("download")} ${t("analysis.btnSymbolInstall")}</button>
        <button class="primary-button" data-action="ram-summary">${icon("search")} ${t("analysis.btnRamSummary")}</button>
        <button class="primary-button" data-action="ram-strings">${icon("shield")} ${t("analysis.btnStrings")}</button>
        <button class="primary-button" data-action="ram-carver">${icon("disk")} ${t("analysis.btnCarver")}</button>
        <button class="primary-button" data-action="ram-processes">${icon("menu")} ${t("analysis.btnProcess")}</button>
      </div>

      <div class="section-divider"></div>

      <div id="ram-analysis-results" class="ram-dashboard" style="display:none">
        <!-- Stats Row -->
        <div class="ram-stats">
          <div class="ram-stat-card">
            <span class="card-icon">${icon("shield")}</span>
            <div class="stat-info">
              <strong id="stat-strings-count">0</strong>
              <small>Bulgu Dizgileri / IOCs</small>
            </div>
          </div>
          <div class="ram-stat-card">
            <span class="card-icon">${icon("disk")}</span>
            <div class="stat-info">
              <strong id="stat-carved-count">0</strong>
              <small>Kurtarılan Dosya / Carved</small>
            </div>
          </div>
          <div class="ram-stat-card">
            <span class="card-icon">${icon("menu")}</span>
            <div class="stat-info">
              <strong id="stat-procs-count">0</strong>
              <small>Aktif Proses / PIDs</small>
            </div>
          </div>
          <div class="ram-stat-card">
            <span class="card-icon">${icon("info")}</span>
            <div class="stat-info">
              <strong id="stat-status-lbl">Hazır / Ready</strong>
              <small>Durum / Status</small>
            </div>
          </div>
        </div>

        <div class="forensic-split" id="ram-split-view" style="display:none">
          <!-- Left list -->
          <div class="tree-panel" id="ram-left-panel">
            <div class="panel-header">
              <h3 id="ram-left-panel-title">${t("analysis.lblProcesses")}</h3>
            </div>
            <div class="file-tree-container" id="ram-left-list" style="padding:0">
              <!-- Dynamic processes or carved files -->
            </div>
          </div>
          <!-- Right panel -->
          <div class="preview-panel" id="ram-right-panel">
            <div class="panel-header">
              <h3 id="ram-right-panel-title">${t("analysis.lblMaps")}</h3>
            </div>
            <div class="file-tree-container" id="ram-right-content">
              <!-- Dynamic details/maps -->
            </div>
          </div>
        </div>

        <!-- Flat Result Panel -->
        <div class="workflow-panel" id="ram-flat-results-panel" style="display:none;padding:16px;margin-top:14px">
          <p class="section-label" id="ram-flat-title">${t("analysis.lblStrings")}</p>
          <div class="strings-results-list" id="ram-flat-results-list">
            <!-- Matches list -->
          </div>
        </div>
      </div>
    </div>
  `;
}

function renderAndroidAnalysis({ t, icon, state, caseSelectOptions }) {
  return `
    <div class="workflow-panel">
      <p class="section-label">${t("analysis.tabAndroid")}</p>
      <p class="field-hint">${t("analysis.androidHint")}</p>
      <div class="form-grid">
        <label class="field">
          <span>${t("report.case")}</span>
          <select id="android-analysis-case" class="select" data-case-select>
            ${caseSelectOptions(state.activeCase?.case_name)}
          </select>
        </label>
      </div>
      <div class="button-row" style="margin-top:14px">
        <button class="primary-button" data-action="android-analysis">${icon("android")} ${t("analysis.btnAndroidSummary")}</button>
        <button class="secondary-button" data-action="refresh-cases">${icon("refresh")} ${t("case.refresh")}</button>
      </div>
      <div class="section-divider"></div>
      <div id="android-analysis-results" class="workflow-panel" style="padding:16px">
        <div class="log-box">${t("analysis.androidWaiting")}</div>
      </div>
    </div>
  `;
}
