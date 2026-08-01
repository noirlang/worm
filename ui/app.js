import { androidModePage, androidPage, handleAndroidAction, syncAndroidDeviceSelection } from "./android.js";
import { iosPage, handleIosAction, syncIosBackupPathInput } from "./ios.js";
import { createApiRequest } from "./core/api.js";
import { errorBoxHtml } from "./core/errors.js";
import { detectPlatform, platformLabel as platformName } from "./core/platform.js";
import { showToast } from "./core/toast.js";
import { canonicalRamFileName, compactLogLine, escapeHtml, formatBytes } from "./core/utils.js";
import { localText, toolCards, workflows } from "./core/workflows.js";
import { icon, hydrateIcons, fontIcons } from "./icons.js";
import { translate } from "./i18n.js";
import { homePage, metric } from "./pages/home.js";
import { windowsPage } from "./pages/windows.js";
import { linuxPage } from "./pages/linux.js";
import { agentPage } from "./pages/agent.js";
import { analysisPage } from "./pages/analysis.js";
import { otherPage, detailPanel, settingsPage, aboutPage, hashPanel } from "./pages/other.js";
import { workflowPage, pickerField, field, pageTitle, casePanel } from "./pages/workflow.js";
import { initDeveloperMode, devLog } from "./developer.js";

const APP_VERSION = "v0.0.15";
const assetPath = "./assets";
const backendAvailable = location.protocol === "http:" || location.protocol === "https:";
const urlParams = new URLSearchParams(window.location.search);
const isDevConsole = urlParams.get("route") === "devlogs";
const isNativeWebView = urlParams.get("native") === "1";
const isNativeLinux =
  isNativeWebView && /linux/i.test(`${navigator.platform || ""} ${navigator.userAgent || ""}`);
if (isNativeWebView) document.documentElement.classList.add("native-webview");
if (isNativeLinux) document.documentElement.classList.add("native-linux");

const app = document.querySelector("#app");
const view = document.querySelector("#view");
const profileGate = document.querySelector("#profile-gate");
const preferredLanguage = localStorage.getItem("amele-language") || "en";
const requestedTheme = urlParams.get("theme");
const preferredTheme = ["dark", "light"].includes(requestedTheme || "") ? requestedTheme : localStorage.getItem("amele-theme") || "dark";
const preferredSidebarCollapsed = localStorage.getItem("amele-sidebar-collapsed") === "1";
if (preferredSidebarCollapsed) app.classList.add("sidebar-collapsed");

function initialLogMessages(language) {
  return [translate(language, backendAvailable ? "log.appReady" : "log.previewMode")];
}

const state = {
  route: urlParams.get("route") || "home",
  isDevConsole,
  theme: preferredTheme,
  language: preferredLanguage,
  sidebarCollapsed: preferredSidebarCollapsed,
  platform: detectPlatform(),
  files: {},
  activeTab: "hash",
  approvedSecurityKey: "",
  remoteConnections: {},
  activeAcquisition: null,
  activeCase: null,
  pendingCaseName: "",
  cases: [],
  acquisitionHistory: [],
  caseBaseDir: "",
  profiles: [],
  activeProfile: null,
  mobileToolsAccess: { allowed: false, reason: "" },
  profileGateVisible: false,
  profileGateMode: "select",
  profileDraft: {
    fullName: "",
    username: "",
    onlineIdentifier: "",
    openDirectly: false
  },
  imageMount: null,
  imageMountLogHTML: "",
  imagePathInput: "",
  ramAnalysisPathInput: "",
  ramOsProfile: "windows",
  ramSymbolDirInput: "",
  latestUpdate: null,
  updateTarget: null,
  android: {
    adbStatus: null,
    devices: [],
    selectedDevice: ""
  },
  ios: {
    backupPath: "",
    profile: null,
    hashAlgorithms: ["md5", "sha1", "sha256"],
    normalizeJob: null,
    normalizeLog: []
  },
  jobs: {},
  cachedDefaultCaseName: "",
  lastLog: initialLogMessages(preferredLanguage)
};

function t(key, vars = {}) {
  return translate(state.language, key, vars);
}

const apiRequest = createApiRequest({ backendAvailable });
const localizeText = (value) => localText(value, state.language);

function boundDetailPanel(tab) {
  return detailPanel({
    tab,
    t,
    icon,
    state,
    pickerField: boundPickerField,
    field,
    escapeHtml,
    caseSelectOptions,
    hashPanel
  });
}

function boundPickerField(label, id, value, type = "file") {
  return pickerField(label, id, value, type, icon, t);
}

function setRoute(route) {
  if (route.startsWith("workflow:")) {
    const workflow = workflows[route.split(":")[1]];
    if (workflow && isLocalWorkflowBlocked(workflow)) {
      devLog("WARN", "ui:router", `Route blocked (platform mismatch): ${route} — expected ${workflow.platform}, got ${state.platform}`, apiRequest, backendReady);
      showToast(t("platformBlocked", { platform: workflow.platform }), "warning");
      return;
    }
  }
  devLog("DEBUG", "ui:router", `Navigate → ${route}`, apiRequest, backendReady);
  state.route = route;
  render();
}

function isLocalWorkflowBlocked(workflow) {
  if (!workflow.mode.startsWith("local")) return false;
  return workflow.platform.toLowerCase() !== state.platform;
}

function setTheme(theme) {
  const nextTheme = theme === "light" ? "light" : "dark";
  state.theme = nextTheme;
  localStorage.setItem("amele-theme", nextTheme);
  app.classList.toggle("theme-light", nextTheme === "light");
  app.classList.toggle("theme-dark", nextTheme !== "light");
  syncBrandLogo();
}

function setLanguage(language) {
  const nextLanguage = language === "en" ? "en" : "tr";
  state.language = nextLanguage;
  localStorage.setItem("amele-language", nextLanguage);
  document.documentElement.lang = nextLanguage;
  document.querySelectorAll("[data-i18n]").forEach((node) => {
    node.textContent = t(node.dataset.i18n);
  });
  syncProfileButton();
}

function syncBrandLogo() {
  const brandImg = document.querySelector("#brand-logo-img");
  if (!brandImg) return;
  brandImg.src = state.theme === "light" ? "./assets/logo/logo-siyah.png" : "./assets/logo/logo.png";
}

function syncSidebarState() {
  app.classList.toggle("sidebar-collapsed", state.sidebarCollapsed);
  document.querySelectorAll("[data-sidebar-toggle]").forEach((button) => {
    button.setAttribute("aria-expanded", String(!state.sidebarCollapsed));
    button.setAttribute("aria-label", state.sidebarCollapsed ? "Menüyü genişlet" : "Menüyü daralt");
  });
}

function setSidebarCollapsed(collapsed) {
  state.sidebarCollapsed = Boolean(collapsed);
  localStorage.setItem("amele-sidebar-collapsed", state.sidebarCollapsed ? "1" : "0");
  syncSidebarState();
}

function applyPersistedSettings(settings) {
  if (!settings || typeof settings !== "object") return;
  setLanguage(settings.dil === "en" ? "en" : "tr");
  setTheme(settings.karanlik_tema ? "dark" : "light");
}

async function loadPersistedSettings() {
  if (!backendReady()) return;
  try {
    const result = await apiRequest("/api/settings");
    applyPersistedSettings(result.settings);
    devLog(
      "INFO",
      "ui:settings",
      `Kalıcı ayarlar yüklendi: ${result.path || "(path yok)"}`,
      apiRequest,
      backendReady
    );
  } catch (error) {
    devLog("WARN", "ui:settings", `Kalıcı ayarlar yüklenemedi: ${error.message}`, apiRequest, backendReady);
  }
}

async function loadUpdateTarget() {
  if (!backendReady()) return;
  try {
    state.updateTarget = await apiRequest("/api/update-target");
    devLog(
      "INFO",
      "ui:update",
      `Güncelleme paket tipi algılandı: ${state.updateTarget.package_label || state.updateTarget.package_kind || "-"}`,
      apiRequest,
      backendReady
    );
  } catch (error) {
    devLog("WARN", "ui:update", `Güncelleme paket tipi algılanamadı: ${error.message}`, apiRequest, backendReady);
  }
}

async function saveSettingsFromControls() {
  const language = document.querySelector("[data-action='language-select']")?.value || state.language;
  setLanguage(language);
  setTheme(state.theme);
  if (!backendReady()) return { path: "localStorage" };

  const result = await apiRequest("/api/settings", {
    method: "POST",
    body: JSON.stringify({
      theme: state.theme,
      language: state.language
    })
  });
  applyPersistedSettings(result.settings);
  if (state.activeProfile) {
    state.activeProfile.language = state.language;
    state.activeProfile.theme = state.theme;
    upsertProfile(state.activeProfile);
    syncProfileButton();
  }
  return result;
}

async function persistSettingsFromControls() {
  const result = await saveSettingsFromControls();
  render();
  const savedPath = result?.path ? `<br /><small>${escapeHtml(result.path)}</small>` : "";
  setStatus("[data-settings-status]", `${icon("info")} ${t("settingsSaved")}${savedPath}`);
  showToast(t("settingsSaved"));
  return result;
}

async function loadProfiles() {
  if (!backendReady()) {
    hideProfileGate();
    return;
  }
  try {
    const result = await apiRequest("/api/profiles");
    state.profiles = Array.isArray(result.profiles) ? result.profiles : [];
    state.activeProfile = result.active_profile || null;
    if (state.activeProfile) {
      setLanguage(state.activeProfile.language === "en" ? "en" : "tr");
      setTheme(state.activeProfile.theme === "light" ? "light" : "dark");
      state.mobileToolsAccess = cachedMobileToolsAccess(state.activeProfile);
      hideProfileGate();
    } else {
      state.mobileToolsAccess = { allowed: false, reason: "" };
      state.profileGateMode = state.profiles.length ? "select" : "create";
      showProfileGate();
    }
    syncProfileButton();
  } catch (error) {
    devLog("ERROR", "ui:profile", `Profil listesi yüklenemedi: ${error.message}`, apiRequest, backendReady);
    state.profileGateMode = "create";
    showProfileGate(t("profile.loadFailed", { message: error.message }));
  }
}

function showProfileGate(errorMessage = "") {
  state.profileGateVisible = true;
  renderProfileGate(errorMessage);
}

function hideProfileGate() {
  state.profileGateVisible = false;
  if (profileGate) {
    profileGate.hidden = true;
    profileGate.innerHTML = "";
  }
}

function renderProfileGate(errorMessage = "") {
  if (!profileGate) return;
  profileGate.hidden = false;
  const isOnline = state.profileGateMode === "online";
  const isCreate = !isOnline && (state.profileGateMode === "create" || !state.profiles.length);
  const cards = state.profiles.map((profile) => `
    <button class="profile-card" data-action="profile-select" data-username="${escapeHtml(profile.username)}">
      <span class="profile-card-top">
        ${renderProfileAvatar(profile)}
        ${profile.online ? `<span class="online-profile-mark" title="${escapeHtml(t("profile.onlineConnectedShort"))}">${icon("globe")}</span>` : ""}
      </span>
      <strong>${escapeHtml(profile.full_name || profile.username)}</strong>
      <small>@${escapeHtml(profile.username)}</small>
      ${profile.online ? `<small class="profile-card-status">${icon("globe")} ${t("profile.onlineAccount")} @${escapeHtml(profile.online.username || "-")}</small>` : ""}
    </button>
  `).join("");
  profileGate.innerHTML = `
    <section class="profile-gate-card">
      <div class="profile-gate-head">
        <img src="./assets/logo/${state.theme === "light" ? "logo-siyah.png" : "logo.png"}" alt="Amele" />
        <div>
          <h1>${t(isOnline ? "profile.onlineTitle" : (isCreate ? "profile.createTitle" : "profile.selectTitle"))}</h1>
          <p>${t(isOnline ? "profile.onlineDesc" : (isCreate ? "profile.createDesc" : "profile.selectDesc"))}</p>
        </div>
      </div>
      ${errorMessage ? `<div class="error-panel">${escapeHtml(errorMessage)}</div>` : ""}
      ${isOnline ? onlineProfileFormHtml() : (isCreate ? profileFormHtml() : `
        <div class="profile-list">${cards}</div>
        <label class="check-row">
          <input type="checkbox" id="profile-open-directly" />
          <span>${t("profile.openDirectly")}</span>
        </label>
        <div class="button-row">
          <button class="secondary-button" data-action="profile-create-start">${icon("user")} ${t("profile.newProfile")}</button>
          <button class="primary-button" data-action="profile-online-start">${icon("globe")} ${t("profile.onlineConnect")}</button>
        </div>
      `)}
    </section>
  `;
  hydrateIcons(profileGate);
}

function profileFormHtml() {
  const draft = state.profileDraft || {};
  const draftLanguage = draft.language || state.language;
  const draftTheme = draft.theme || state.theme;
  return `
    <div class="profile-form">
      ${field(t("settings.language"), `
        <select id="profile-language" class="select">
          <option value="tr" ${draftLanguage === "tr" ? "selected" : ""}>Türkçe</option>
          <option value="en" ${draftLanguage === "en" ? "selected" : ""}>English</option>
        </select>
      `)}
      ${field(t("settings.darkTheme"), `
        <select id="profile-theme" class="select">
          <option value="dark" ${draftTheme !== "light" ? "selected" : ""}>${t("profile.themeDark")}</option>
          <option value="light" ${draftTheme === "light" ? "selected" : ""}>${t("profile.themeLight")}</option>
        </select>
      `)}
      ${field(t("profile.fullName"), `<input id="profile-full-name" class="input" autocomplete="name" value="${escapeHtml(draft.fullName || "")}" />`)}
      ${field(t("profile.username"), `<input id="profile-username" class="input" autocomplete="username" value="${escapeHtml(draft.username || "")}" />`)}
      <label class="check-row">
        <input type="checkbox" id="profile-create-directly" ${draft.openDirectly ? "checked" : ""} />
        <span>${t("profile.openDirectly")}</span>
      </label>
      <div class="button-row">
        ${state.profiles.length ? `<button class="secondary-button" data-action="profile-select-back">${t("profile.backToProfiles")}</button>` : ""}
        <button class="secondary-button" data-action="profile-online-start">${icon("globe")} ${t("profile.onlineConnect")}</button>
        <button class="primary-button" data-action="profile-submit">${icon("user")} ${t("profile.createButton")}</button>
      </div>
    </div>
  `;
}

function onlineProfileFormHtml() {
  const draft = state.profileDraft || {};
  const draftLanguage = draft.language || state.language;
  const draftTheme = draft.theme || state.theme;
  return `
    <div class="profile-form">
      <div class="info-panel profile-notice">
        ${icon("info")}
        <p>${t("profile.onlineNotice")}</p>
      </div>
      ${field(t("settings.language"), `
        <select id="profile-language" class="select">
          <option value="tr" ${draftLanguage === "tr" ? "selected" : ""}>Türkçe</option>
          <option value="en" ${draftLanguage === "en" ? "selected" : ""}>English</option>
        </select>
      `)}
      ${field(t("settings.darkTheme"), `
        <select id="profile-theme" class="select">
          <option value="dark" ${draftTheme !== "light" ? "selected" : ""}>${t("profile.themeDark")}</option>
          <option value="light" ${draftTheme === "light" ? "selected" : ""}>${t("profile.themeLight")}</option>
        </select>
      `)}
      ${field(t("profile.onlineIdentifier"), `<input id="profile-online-identifier" class="input" autocomplete="username" value="${escapeHtml(draft.onlineIdentifier || "")}" />`)}
      ${field(t("profile.onlinePassword"), `<input id="profile-online-password" class="input" type="password" autocomplete="current-password" />`)}
      <label class="check-row">
        <input type="checkbox" id="profile-online-directly" ${draft.openDirectly ? "checked" : ""} />
        <span>${t("profile.openDirectly")}</span>
      </label>
      <div class="button-row">
        <button class="secondary-button" data-action="profile-online-back">${t(state.profiles.length ? "profile.backToProfiles" : "profile.localSetup")}</button>
        <button class="primary-button" data-action="profile-online-submit">${icon("globe")} ${t("profile.onlineLogin")}</button>
      </div>
    </div>
  `;
}

function captureProfileDraft() {
  const fullName = document.querySelector("#profile-full-name");
  const username = document.querySelector("#profile-username");
  const onlineIdentifier = document.querySelector("#profile-online-identifier");
  const language = document.querySelector("#profile-language");
  const theme = document.querySelector("#profile-theme");
  const direct = document.querySelector("#profile-create-directly");
  const onlineDirect = document.querySelector("#profile-online-directly");
  state.profileDraft = {
    fullName: fullName ? fullName.value : state.profileDraft?.fullName || "",
    username: username ? username.value : state.profileDraft?.username || "",
    onlineIdentifier: onlineIdentifier ? onlineIdentifier.value : state.profileDraft?.onlineIdentifier || "",
    language: language ? language.value : state.profileDraft?.language || state.language,
    theme: theme ? theme.value : state.profileDraft?.theme || state.theme,
    openDirectly: direct ? direct.checked : (onlineDirect ? onlineDirect.checked : Boolean(state.profileDraft?.openDirectly))
  };
}

function profileInitials(profile) {
  const source = profile.full_name || profile.username || "A";
  return source
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toLocaleUpperCase(state.language === "tr" ? "tr-TR" : "en-US") || "")
    .join("") || "A";
}

function getAvatarUrl(profile) {
  if (!profile) return "";
  const rawUrl = profile.avatar_url || profile.avatarUrl || profile.online?.avatar_url || profile.online?.avatarUrl || "";
  if (!rawUrl) return "";
  if (rawUrl.startsWith("http://") || rawUrl.startsWith("https://") || rawUrl.startsWith("data:")) {
    return rawUrl;
  }
  if (rawUrl.startsWith("/")) {
    const base = profile.online?.apiBase || profile.online?.api_base || profile.apiBase || profile.api_base || "https://aamele-noirlang-tr.onrender.com";
    return `${base.replace(/\/+$/, "")}${rawUrl}`;
  }
  return rawUrl;
}

function renderProfileAvatar(profile, extraClass = "") {
  const avatarUrl = getAvatarUrl(profile);
  const initials = profileInitials(profile || {});
  const sizeClass = extraClass ? ` ${extraClass}` : "";
  if (avatarUrl) {
    const rawPath = (profile.avatar_url || profile.avatarUrl || profile.online?.avatar_url || profile.online?.avatarUrl || "");
    const altBase = rawPath.startsWith("/") ? `https://amele.noirlang.tr${rawPath}` : "";
    return `<span class="profile-avatar${sizeClass}"><img src="${escapeHtml(avatarUrl)}" alt="Avatar" class="avatar-img" data-alt-src="${escapeHtml(altBase)}" onerror="if(this.dataset.altSrc && this.src !== this.dataset.altSrc){ this.src = this.dataset.altSrc; this.dataset.altSrc = ''; } else { this.style.display='none'; if(this.nextElementSibling) this.nextElementSibling.style.display='inline-grid'; }" /><span class="avatar-fallback" style="display:none;">${initials}</span></span>`;
  }
  return `<span class="profile-avatar${sizeClass}">${initials}</span>`;
}

function syncProfileButton() {
  const label = document.querySelector("[data-profile-label]");
  if (label) label.textContent = state.activeProfile?.display_name || t("profile.button");
  const button = document.querySelector(".profile-action");
  if (button && state.activeProfile) {
    const avatarEl = button.querySelector(".profile-avatar, [data-icon='user']");
    if (avatarEl) {
      avatarEl.outerHTML = renderProfileAvatar(state.activeProfile, "small-header");
    }
  }
}

function cachedMobileToolsAccess(profile = state.activeProfile) {
  const online = profile?.online;
  const licenses = Array.isArray(online?.licenses) ? online.licenses : [];
  const hasMobileLicense = licenses.some((license) => (
    String(license?.status || "").toLowerCase() === "active"
    && String(license?.plan || "").toLowerCase().replace(/[_\s]+/g, "-") === "mobile-tools"
  ));
  const allowed = Boolean(online?.mobile_tools_enabled || hasMobileLicense);
  return {
    allowed,
    reason: allowed ? "" : "",
    profile
  };
}

async function selectProfile(username) {
  const openDirectly = Boolean(document.querySelector("#profile-open-directly")?.checked);
  const result = await apiRequest("/api/profiles/select", {
    method: "POST",
    body: JSON.stringify({ username, open_directly: openDirectly })
  });
  state.activeProfile = result.profile;
  if (result.access) {
    state.mobileToolsAccess = result.access;
  }
  upsertProfile(result.profile);
  setLanguage(state.activeProfile.language === "en" ? "en" : "tr");
  setTheme(state.activeProfile.theme === "light" ? "light" : "dark");
  if (!result.access) {
    await refreshMobileToolsAccess({ silent: true });
  }
  hideProfileGate();
  syncProfileButton();
  await loadPersistedSettings();
  await loadEvidenceCases();
  render();
}

async function createProfileFromGate() {
  const language = document.querySelector("#profile-language")?.value || state.language;
  const theme = document.querySelector("#profile-theme")?.value || state.theme;
  const fullName = document.querySelector("#profile-full-name")?.value.trim() || "";
  const username = document.querySelector("#profile-username")?.value.trim() || "";
  const openDirectly = Boolean(document.querySelector("#profile-create-directly")?.checked);
  if (!fullName || !username) {
    showToast(t("profile.required"), "error");
    return;
  }
  const result = await apiRequest("/api/profiles/create", {
    method: "POST",
    body: JSON.stringify({ full_name: fullName, username, language, theme, open_directly: openDirectly })
  });
  upsertProfile(result.profile);
  state.activeProfile = result.profile;
  state.mobileToolsAccess = { allowed: false, reason: "" };
  state.profileDraft = { fullName: "", username: "", onlineIdentifier: "", openDirectly: false };
  setLanguage(result.profile.language === "en" ? "en" : "tr");
  setTheme(result.profile.theme === "light" ? "light" : "dark");
  hideProfileGate();
  syncProfileButton();
  await loadPersistedSettings();
  await loadEvidenceCases();
  render();
}

async function connectOnlineProfileFromGate() {
  captureProfileDraft();
  const identifier = document.querySelector("#profile-online-identifier")?.value.trim() || "";
  const password = document.querySelector("#profile-online-password")?.value || "";
  const language = document.querySelector("#profile-language")?.value || state.language;
  const theme = document.querySelector("#profile-theme")?.value || state.theme;
  const openDirectly = Boolean(document.querySelector("#profile-online-directly")?.checked);
  if (!identifier || !password) {
    showToast(t("profile.onlineRequired"), "error");
    return;
  }
  const result = await apiRequest("/api/profiles/online-login", {
    method: "POST",
    body: JSON.stringify({ identifier, password, language, theme, open_directly: openDirectly })
  });
  state.activeProfile = result.profile;
  upsertProfile(result.profile);
  state.mobileToolsAccess = result.access || { allowed: false, reason: "" };
  state.profileDraft = {
    fullName: "",
    username: "",
    onlineIdentifier: "",
    language,
    theme,
    openDirectly
  };
  setLanguage(result.profile.language === "en" ? "en" : "tr");
  setTheme(result.profile.theme === "light" ? "light" : "dark");
  hideProfileGate();
  syncProfileButton();
  await loadPersistedSettings();
  await loadEvidenceCases();
  render();
  showToast(t("profile.onlineConnected"), "success");
}

async function syncOnlineProfile(button) {
  button.disabled = true;
  try {
    const result = await apiRequest("/api/profiles/online-sync", { method: "POST" });
    state.activeProfile = result.profile;
    upsertProfile(result.profile);
    state.mobileToolsAccess = result.access || { allowed: false, reason: "" };
    syncProfileButton();
    render();
    showToast(t("profile.onlineSynced"), "success");
  } catch (error) {
    showToast(t("profile.onlineSyncFailed", { message: error.message }), "error");
  } finally {
    button.disabled = false;
  }
}

async function disconnectOnlineProfile(button) {
  button.disabled = true;
  try {
    const result = await apiRequest("/api/profiles/online-logout", { method: "POST" });
    state.activeProfile = result.profile || state.activeProfile;
    if (result.profile) upsertProfile(result.profile);
    state.mobileToolsAccess = result.access || { allowed: false, reason: "" };
    syncProfileButton();
    render();
    showToast(t("profile.onlineDisconnectedToast"), "success");
  } catch (error) {
    showToast(t("profile.onlineDisconnectFailed", { message: error.message }), "error");
  } finally {
    button.disabled = false;
  }
}

function upsertProfile(profile) {
  if (!profile) return;
  const index = state.profiles.findIndex((item) => item.username === profile.username);
  if (index >= 0) {
    state.profiles[index] = profile;
  } else {
    state.profiles.push(profile);
  }
}

async function refreshMobileToolsAccess({ silent = false } = {}) {
  if (!backendReady() || !state.activeProfile) {
    state.mobileToolsAccess = { allowed: false, reason: "" };
    return state.mobileToolsAccess;
  }
  try {
    const result = await apiRequest("/api/profiles/mobile-access");
    state.mobileToolsAccess = result.access || { allowed: false, reason: "" };
    if (result.access?.profile) {
      state.activeProfile = result.access.profile;
      upsertProfile(result.access.profile);
    }
  } catch (error) {
    state.mobileToolsAccess = { allowed: false, reason: error.message || "" };
    if (!silent) showToast(error.message, "error");
  }
  return state.mobileToolsAccess;
}

function isMobileToolsRoute(route) {
  return route === "android" || route === "ios" || route.startsWith("android:");
}

function onlineMobileToolsAllowed() {
  return Boolean(state.mobileToolsAccess?.allowed);
}

function mobileToolsLockedPage({ t, icon, pageTitle }) {
  return `
    <section class="page">
      ${pageTitle(t("mobile.locked.title"), t("mobile.locked.desc"), "key", icon)}
      <div class="workflow-layout locked-layout">
        <div class="workflow-panel mobile-lock-panel">
          <span class="metric-icon">${icon("shield")}</span>
          <h3>${t("mobile.locked.heading")}</h3>
          <p>${t("mobile.locked.body")}</p>
          <div class="button-row">
            <button class="primary-button" data-action="profile-online-start">${icon("globe")} ${t("profile.onlineConnect")}</button>
            <button class="secondary-button" data-route="profile">${icon("user")} ${t("profile.title")}</button>
          </div>
        </div>
      </div>
    </section>
  `;
}

function render() {
  if (state.isDevConsole) return;
  const activeGroup = routeGroup(state.route);
  document.querySelectorAll("[data-route]").forEach((button) => {
    button.classList.toggle("active", button.dataset.route === activeGroup);
  });

  const boundCasePanel = (subdir, hint) => casePanel(subdir, hint, {
    t,
    icon,
    state,
    caseSelectOptions,
    caseOutputLabel,
    escapeHtml
  });

  if (isMobileToolsRoute(state.route) && !onlineMobileToolsAllowed()) {
    view.innerHTML = mobileToolsLockedPage({ t, icon, pageTitle });
  } else if (state.route.startsWith("workflow:")) {
    view.innerHTML = workflowPage({
      id: state.route.split(":")[1],
      workflows,
      state,
      t,
      icon,
      localText: localizeText,
      canonicalRamFileName,
      caseSelectOptions,
      caseOutputLabel,
      escapeHtml
    });
  } else if (state.route.startsWith("android:")) {
    view.innerHTML = androidModePage({
      modeId: state.route.split(":")[1],
      t,
      icon,
      pageTitle,
      state,
      escapeHtml,
      backendReady,
      casePanel: boundCasePanel,
      field
    });
  } else if (state.route === "ios") {
    view.innerHTML = iosPage({
      t,
      icon,
      pageTitle,
      state,
      escapeHtml,
      backendReady,
      casePanel: boundCasePanel,
      field
    });
  } else {
    const pageCtx = {
      t,
      icon,
      state,
      assetPath,
      pageTitle,
      pickerField: boundPickerField,
      field,
      escapeHtml,
      caseSelectOptions,
      detailPanel: boundDetailPanel,
      toolHub: (platform) => toolHub(platform),
      platformLabel,
      APP_VERSION,
      theme: state.theme
    };

    syncBrandLogo();

    view.innerHTML = routes[state.route]?.(pageCtx) || homePage(pageCtx);
  }

  hydrateIcons(view);
  if (state.route === "other" && ["evidence", "reports", "history"].includes(state.activeTab)) {
    loadEvidenceCases();
  }
  if (state.route === "other" && state.activeTab === "history") {
    loadAcquisitionHistory();
  }
  if (state.route === "analysis") {
    loadEvidenceCases();
  }
  if (state.route === "profile") {
    loadEvidenceCases();
  }
  if (state.route.startsWith("workflow:")) {
    const workflow = workflows[state.route.split(":")[1]];
    if (workflow && workflow.mode.includes("disk")) loadEvidenceCases();
  }
  if (state.route === "android:logical" || state.route === "android:filesystem" || state.route === "android:ram") loadEvidenceCases();
  if (state.route === "ios") loadEvidenceCases();
  view.focus({ preventScroll: true });
}

function routeGroup(route) {
  if (route.startsWith("android:")) return "android";
  if (route === "ios") return "ios";
  if (!route.startsWith("workflow:")) return route;
  const workflowId = route.split(":")[1] || "";
  if (workflowId.startsWith("windows")) return "windows";
  if (workflowId.startsWith("linux")) return "linux";
  return route;
}

function toolHub(platform) {
  const cards = toolCards[platform]
    .map(
      (card) => {
        const workflow = workflows[card.id];
        const blocked = workflow && isLocalWorkflowBlocked(workflow);
        return `
        <button class="forensic-card ${blocked ? "is-disabled" : ""}" data-route="workflow:${card.id}" style="--accent:${card.accent}" ${blocked ? `aria-disabled="true" data-disabled-reason="${workflow.platform}"` : ""}>
          <span class="card-icon">${icon(card.icon)}</span>
          <h3>${localizeText(card.title)}</h3>
          <p>${localizeText(card.desc)}</p>
          <span class="meta">${blocked ? t("localUnsupported") : localizeText(card.badge)}</span>
        </button>
      `;
      }
    )
    .join("");

  const isWindows = platform === "windows";
  const detectedIcon =
    state.platform === "windows" ? "windows" :
    state.platform === "linux" ? "linux" :
    state.platform === "android" ? "android" :
    "monitor";
  return `
    <section class="page">
      <div class="platform-note">
        ${icon(detectedIcon)} ${t("hub.detected", { platform: `<strong>${platformLabel(state.platform)}</strong>` })}
      </div>
      ${pageTitle(
        t(isWindows ? "hub.windows.title" : "hub.linux.title"),
        t(isWindows ? "hub.windows.desc" : "hub.linux.desc"),
        isWindows ? "windows" : "linux",
        icon
      )}
      <div class="tool-grid">${cards}</div>
    </section>
  `;
}

function platformLabel(platform) {
  return platformName(platform, t("unknown"));
}


function contributorCard(initials, name, role, photo, links) {
  return `
    <article class="contributor-card">
      <img class="avatar" src="${assetPath}/contributors/${photo}" alt="${name}" />
      <h3>${name}</h3>
      <p>${role}</p>
      <div class="social-row" aria-label="${name} bağlantıları">
        ${links.map(([label, url]) => socialLink(label, url)).join("")}
      </div>
    </article>
  `;
}

function socialLink(label, url) {
  const key = label === "LinkedIn" ? "linkedin" : label === "Website" ? "website" : "github";
  return `<a class="social-button" href="${url}" target="_blank" rel="noopener noreferrer" aria-label="${label}">${icon(key)}</a>`;
}

const routes = {
  home: homePage,
  windows: () => toolHub("windows"),
  linux: () => toolHub("linux"),
  android: () => androidPage({ t, icon, pageTitle, state, escapeHtml, backendReady }),
  ios: () => "",
  agent: agentPage,
  analysis: analysisPage,
  profile: profilePage,
  other: otherPage,
  settings: settingsPage,
  about: aboutPage
};

function profilePage({ t, icon, state, pageTitle, escapeHtml }) {
  const profile = state.activeProfile;
  const online = profile?.online || null;
  const accountCard = online
    ? onlineAccountCard(profile, online, state, t, icon, escapeHtml)
    : localAccountCard(profile, state, t, icon, escapeHtml);
  const onlineConnectCard = online ? "" : onlineDisconnectedCard(t, icon);
  const caseCards = state.cases.length
    ? state.cases.map((item) => `
        <article class="case-profile-card">
          <strong>${escapeHtml(item.case_name || "-")}</strong>
          <small>${escapeHtml(item.case_dir || "")}</small>
          <span>${t("profile.caseCounts", {
            images: String(item.output_count || 0),
            ram: String(item.ram_count || 0),
            android: String(item.android_count || 0),
            ios: String(item.ios_count || 0)
          })}</span>
        </article>
      `).join("")
    : `<div class="log-box">${t("profile.noCases")}</div>`;

  return `
    <section class="page">
      ${pageTitle(t("profile.title"), t("profile.desc"), "user", icon)}
      <div class="settings-layout">
        ${accountCard}
        ${onlineConnectCard}
        <article class="settings-card">
          <span class="settings-kicker">${t("profile.cases")}</span>
          <h3>${t("profile.caseTitle")}</h3>
          <div class="profile-case-list">${caseCards}</div>
        </article>
      </div>
    </section>
  `;
}

function localAccountCard(profile, state, t, icon, escapeHtml) {
  return `
    <article class="settings-card settings-primary">
      <span class="settings-kicker">${t("profile.account")}</span>
      <div class="profile-summary">
        ${renderProfileAvatar(profile, "large")}
        <div>
          <h3>${escapeHtml(profile?.full_name || t("profile.noActive"))}</h3>
          <p>@${escapeHtml(profile?.username || "-")}</p>
        </div>
      </div>
      ${profilePreferenceRows(profile, state, t, escapeHtml)}
      <div class="button-row">
        <button class="secondary-button" data-action="profile-new">${icon("user")} ${t("profile.newProfile")}</button>
        <button class="danger-button" data-action="profile-logout">${icon("stop")} ${t("profile.logout")}</button>
      </div>
      ${profileActivityHtml(profile, t, icon, escapeHtml)}
    </article>
  `;
}

function onlineAccountCard(profile, online, state, t, icon, escapeHtml) {
  const mobileAllowed = onlineMobileToolsAllowed();
  const onlineName = onlineDisplayName(online);
  return `
    <article class="settings-card settings-primary">
      <span class="settings-kicker">${t("profile.onlineAccount")}</span>
      <div class="profile-summary">
        ${renderProfileAvatar(profile || { full_name: onlineName, username: online.username, online }, "large")}
        <div>
          <h3>${escapeHtml(onlineName || t("profile.noActive"))}</h3>
          <p>@${escapeHtml(online.username || profile?.username || "-")}</p>
        </div>
      </div>
      ${profilePreferenceRows(profile, state, t, escapeHtml)}
      <div class="settings-row">
        <strong>${t("profile.onlineStatus")}</strong>
        <span class="status-pill ok">${t("profile.onlineConnectedShort")}</span>
      </div>
      <div class="settings-row">
        <strong>${t("profile.roles")}</strong>
        <span class="role-list">${onlineRoleBadges(online, t, escapeHtml)}</span>
      </div>
      <div class="settings-row">
        <strong>${t("profile.license")}</strong>
        <span>${onlineLicenseText(online, t, escapeHtml)}</span>
      </div>
      <div class="settings-row">
        <strong>${t("profile.mobileAccess")}</strong>
        <span class="status-pill ${mobileAllowed ? "ok" : "danger"}">${mobileAllowed ? t("profile.mobileUnlocked") : t("profile.mobileLocked")}</span>
      </div>
      <div class="settings-row">
        <strong>${t("profile.workedTypes")}</strong>
        <span>${workedCaseTypesText(online, t, escapeHtml)}</span>
      </div>
      <div class="settings-row">
        <strong>${t("profile.lastSync")}</strong>
        <small>${escapeHtml(online.last_sync_at || "-")}</small>
      </div>
      <div class="button-row">
        <button class="secondary-button" data-action="profile-new">${icon("user")} ${t("profile.newProfile")}</button>
        <button class="secondary-button" data-action="profile-online-sync">${icon("refresh")} ${t("profile.onlineSync")}</button>
        <button class="danger-button" data-action="profile-online-logout">${icon("stop")} ${t("profile.onlineDisconnect")}</button>
        <button class="danger-button" data-action="profile-logout">${icon("stop")} ${t("profile.logout")}</button>
      </div>
      ${profileActivityHtml(profile, t, icon, escapeHtml)}
    </article>
  `;
}

function onlineDisconnectedCard(t, icon) {
  return `
    <article class="settings-card">
      <span class="settings-kicker">${t("profile.onlineAccount")}</span>
      <h3>${t("profile.onlineDisconnected")}</h3>
      <div class="settings-row">
        <strong>${t("profile.onlineStatus")}</strong>
        <span class="status-pill warn">${t("profile.onlineDisconnectedShort")}</span>
      </div>
      <div class="button-row">
        <button class="primary-button" data-action="profile-online-start">${icon("globe")} ${t("profile.onlineConnect")}</button>
      </div>
    </article>
  `;
}

function profilePreferenceRows(profile, state, t, escapeHtml) {
  return `
    <div class="settings-row">
      <strong>${t("settings.language")}</strong>
      <span>${profile?.language === "en" ? "English" : "Türkçe"}</span>
    </div>
    <div class="settings-row">
      <strong>${t("profile.theme")}</strong>
      <span>${profile?.theme === "light" ? t("profile.themeLight") : t("profile.themeDark")}</span>
    </div>
    <div class="settings-row">
      <strong>${t("case.location")}</strong>
      <small>${escapeHtml(state.caseBaseDir || "~/Amele/Kullanicilar/.../Vakalar")}</small>
    </div>
  `;
}

function onlineDisplayName(online) {
  const fullName = [online?.first_name, online?.last_name]
    .map((value) => String(value || "").trim())
    .filter(Boolean)
    .join(" ");
  return fullName || online?.username || "";
}

function onlineRoleBadges(online, t, escapeHtml) {
  const roles = Array.isArray(online?.roles) ? online.roles : [];
  if (!roles.length) return `<span class="status-pill warn">${escapeHtml(t("profile.noRoles"))}</span>`;
  return roles
    .map((role) => `<span class="status-pill ok">${escapeHtml(roleLabel(role, t))}</span>`)
    .join("");
}

function roleLabel(role, t) {
  const labels = {
    bdfl: "BDFL",
    developer: t("profile.role.developer"),
    member: t("profile.role.member"),
    "windows-maintainer": "Windows Maintainer",
    "linux-maintainer": "Linux Maintainer",
    "android-maintainer": "Android Maintainer"
  };
  return labels[role] || role;
}

function onlineLicenseText(online, t, escapeHtml) {
  if (!online) return escapeHtml(t("profile.onlineRequiredForLicense"));
  const licenses = Array.isArray(online.licenses) ? online.licenses : [];
  const active = licenses.find((license) => license.status === "active");
  if (active) {
    const plan = active.plan || t("unknown");
    return `${escapeHtml(t("profile.licenseActive"))} · ${escapeHtml(plan)}`;
  }
  return escapeHtml(online.has_license ? t("profile.licenseActive") : t("profile.licenseNone"));
}

function workedCaseTypesText(online, t, escapeHtml) {
  const values = Array.isArray(online?.worked_case_types) ? online.worked_case_types : [];
  if (!values.length) return escapeHtml(t("profile.workedTypesEmpty"));
  return values.map((value) => escapeHtml(value)).join(" · ");
}

function profileActivityHtml(profile, t, icon, escapeHtml) {
  const log = Array.isArray(profile?.activity_log) ? profile.activity_log.slice(-5).reverse() : [];
  if (!log.length) return `<div class="log-box profile-activity-log">• ${escapeHtml(t("profile.activityEmpty"))}</div>`;
  return `
    <div class="profile-activity-log">
      <p class="section-label">${icon("clock")} ${t("profile.activity")}</p>
      ${log.map((entry) => `
        <div class="tree-node">
          <strong>${escapeHtml(entry.category || "-")} · ${escapeHtml(entry.action || "-")}</strong>
          <span>
            ${escapeHtml(entry.case_name || entry.details || "-")}
            <small>${escapeHtml(entry.timestamp || "")}</small>
          </span>
        </div>
      `).join("")}
    </div>
  `;
}

function isExternalUrl(url) {
  try {
    const parsed = new URL(url, window.location.href);
    return ["http:", "https:", "mailto:"].includes(parsed.protocol);
  } catch {
    return false;
  }
}

async function openExternalUrl(url) {
  try {
    await apiRequest("/api/open-url", {
      method: "POST",
      body: JSON.stringify({ url })
    });
    return;
  } catch (error) {
    console.warn("External link could not be opened by backend", error);
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

async function loadEvidenceCases({ silent = true } = {}) {
  if (!backendReady()) return;
  try {
    const result = await apiRequest("/api/evidence-cases");
    state.caseBaseDir = result.base_dir || "";
    state.cases = Array.isArray(result.cases) ? result.cases : [];
    
    // Keep frontend selected/pending case active even before it exists on disk.
    const activeCaseName = state.pendingCaseName || state.activeCase?.case_name;
    if (activeCaseName) {
      const stillExists = state.cases.find(c => c.case_name === activeCaseName);
      if (stillExists) {
        state.activeCase = stillExists;
        state.pendingCaseName = "";
      } else if (state.pendingCaseName) {
        state.activeCase = { case_name: state.pendingCaseName };
      } else if (result.current_case) {
        state.activeCase = result.current_case;
      }
    } else if (result.current_case) {
      state.activeCase = result.current_case;
    } else if (state.cases.length) {
      state.activeCase = state.cases[0];
    }

    updateCaseControls();
    if (!silent) showToast(t("case.loaded", { count: String(state.cases.length) }));
  } catch (error) {
    if (!silent) showToast(t("case.listFailed", { message: error.message }), "error");
  }
}

function updateCaseControls() {
  document.querySelectorAll("[data-case-base]").forEach((node) => {
    node.textContent = state.caseBaseDir || "~/Amele/Vakalar";
  });

  const selected = state.pendingCaseName || state.activeCase?.case_name || "";
  document.querySelectorAll("[data-case-select]").forEach((select) => {
    const allowNew = select.dataset.allowNewCase === "1";
    select.innerHTML = caseSelectOptions(selected, { allowNew });
    select.value = selected;
    toggleCaseCreateInput(select);
  });
}

function caseSelectOptions(selected = "", { allowNew = false } = {}) {
  const effectiveSelected = selected || (allowNew && !state.cases.length ? "__new__" : "");
  if (!state.cases.length && !allowNew) {
    return `<option value="">${t("case.noCases")}</option>`;
  }
  const hasSelected = effectiveSelected && effectiveSelected !== "__new__"
    ? state.cases.some((item) => item.case_name === effectiveSelected)
    : true;
  const pendingOption = !hasSelected
    ? `<option value="${escapeHtml(effectiveSelected)}" selected>${escapeHtml(effectiveSelected)}</option>`
    : "";
  const options = state.cases
    .map((item) => {
      const name = escapeHtml(item.case_name || "");
      const isSelected = item.case_name === effectiveSelected ? " selected" : "";
      return `<option value="${name}"${isSelected}>${name}</option>`;
    })
    .join("");
  const newSelected = effectiveSelected === "__new__" || (allowNew && !state.cases.length) ? " selected" : "";
  const newOption = allowNew ? `<option value="__new__"${newSelected}>${t("workflow.newCase")}</option>` : "";
  return `${pendingOption}${options}${newOption}`;
}

function toggleCaseCreateInput(select) {
  document.querySelectorAll("[data-case-output]").forEach((output) => {
    output.textContent = caseOutputLabel(select.value, output.dataset.caseOutputSubdir || "ciktilar");
  });
}

function imageCaseOutputLabel(caseName) {
  return caseOutputLabel(caseName, "ciktilar");
}

function caseOutputLabel(caseName, subdir = "ciktilar") {
  if (caseName === "__new__") caseName = state.pendingCaseName || "";
  const selected = state.cases.find((item) => item.case_name === caseName)
    || (state.activeCase?.case_name === caseName ? state.activeCase : null);
  const keyBySubdir = {
    android: "android_dir",
    ios: "ios_dir",
    ram: "ram_dir",
    ciktilar: "output_dir"
  };
  const key = keyBySubdir[subdir] || "output_dir";
  if (caseName && caseName !== "__new__" && selected?.[key]) return selected[key];
  const folderBySubdir = {
    android: "android",
    ios: "ios",
    ram: "ram",
    ciktilar: "ciktilar"
  };
  const folder = folderBySubdir[subdir] || "ciktilar";
  if (state.caseBaseDir) return `${state.caseBaseDir}/${caseName || "vaka"}/${folder}`;
  return `~/Amele/Vakalar/${caseName || "vaka"}/${folder}`;
}

function reportCaseName() {
  return resolveSelectedCaseName("#report-case", { fallbackToDefault: true });
}

function resolveSelectedCaseName(selector = "#workflow-case", { fallbackToDefault = false } = {}) {
  const selected = document.querySelector(selector)?.value.trim() || "";
  if (selected && selected !== "__new__") return selected;
  if (state.pendingCaseName) return state.pendingCaseName;
  if (state.activeCase?.case_name) return state.activeCase.case_name;
  return fallbackToDefault ? defaultCaseName() : "";
}

async function ensureImageCase() {
  const selected = resolveSelectedCaseName("#workflow-case");
  if (selected) {
    const existing = state.cases.find((item) => item.case_name === selected);
    if (existing && (existing.case_dir || existing.case_path)) return existing;
    
    // Create new case folder on backend disk if not already existing
    const created = await apiRequest("/api/evidence-create", {
      method: "POST",
      body: JSON.stringify({ case_name: selected })
    });
    state.activeCase = created;
    state.pendingCaseName = "";
    state.cachedDefaultCaseName = "";
    await loadEvidenceCases();
    return created;
  }

  const caseName = defaultCaseName();
  const created = await apiRequest("/api/evidence-create", {
    method: "POST",
    body: JSON.stringify({ case_name: caseName })
  });
  state.activeCase = created;
  state.pendingCaseName = "";
  state.cachedDefaultCaseName = "";
  await loadEvidenceCases();
  return created;
}

function defaultCaseName() {
  const now = new Date();
  const pad = (value) => String(value).padStart(2, "0");
  return `Case_${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}_${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
}

function stableDefaultCaseName() {
  if (!state.cachedDefaultCaseName) {
    state.cachedDefaultCaseName = defaultCaseName();
  }
  return state.cachedDefaultCaseName;
}

function selectedTargetName() {
  const select = document.querySelector("[data-field='target']");
  const option = select?.selectedOptions?.[0];
  return option?.dataset.diskName || option?.textContent?.split("·")[0]?.trim() || "";
}

function backendReady() {
  return backendAvailable;
}

function connectionPayload() {
  const tokenText = document.querySelector("[data-field='token']")?.value.trim() || "";
  if (tokenText && !state.approvedSecurityKey) {
    throw new Error(t("connection.keyApproveFirst"));
  }
  if (tokenText && tokenText !== state.approvedSecurityKey) {
    throw new Error(t("connection.keyChanged"));
  }
  return {
    ip: document.querySelector("[data-field='ip']")?.value.trim() || "",
    port: Number(document.querySelector("[data-field='port']")?.value.trim() || 0),
    token: tokenText ? state.approvedSecurityKey : null
  };
}

function vpnPayload() {
  const endpoint = document.querySelector("[data-field='vpn-endpoint']")?.value.trim() || "";
  const configFile = document.querySelector("#vpn-config-file")?.value.trim() || "";
  if (!endpoint) throw new Error(t("vpn.endpointRequired"));
  if (!configFile) throw new Error(t("vpn.configRequired"));
  return {
    config_file: configFile,
    private_key: document.querySelector("[data-field='vpn-private-key']")?.value.trim() || "",
    public_key: document.querySelector("[data-field='vpn-public-key']")?.value.trim() || "",
    endpoint,
    allowed_ips: document.querySelector("[data-field='vpn-allowed']")?.value.trim() || "0.0.0.0/0",
    address: document.querySelector("[data-field='vpn-address']")?.value.trim() || "10.0.0.2/24",
    dns: document.querySelector("[data-field='vpn-dns']")?.value.trim() || "1.1.1.1",
    keepalive: Number(document.querySelector("[data-field='vpn-keepalive']")?.value.trim() || 25)
  };
}

function currentWorkflowId() {
  return state.route.startsWith("workflow:") ? state.route.split(":")[1] : "";
}

function currentWorkflow() {
  return workflows[currentWorkflowId()];
}

function rememberConnection(workflowId, payload, details) {
  state.remoteConnections[workflowId] = {
    ip: payload.ip,
    port: payload.port,
    token: payload.token || "",
    serverName: details.server_name || "",
    serverVersion: details.server_version || "",
    features: details.features || []
  };
}

function forgetConnection(workflowId = currentWorkflowId()) {
  if (workflowId) delete state.remoteConnections[workflowId];
}

function requireActiveConnection(workflow, payload) {
  if (!workflow?.mode.startsWith("remote")) return true;
  const connection = state.remoteConnections[currentWorkflowId()];
  const matches = connection
    && connection.ip === payload.ip
    && Number(connection.port) === Number(payload.port)
    && (connection.token || "") === (payload.token || "");
  if (!matches) {
    showToast(t("connection.connectFirst"), "error");
    updateSide("connection", t("connection.none"));
    writeWorkflowLog(t("connection.required"));
    return false;
  }
  return true;
}

document.addEventListener("click", async (event) => {
  const externalLink = event.target.closest("a[href]");
  if (externalLink && isExternalUrl(externalLink.href)) {
    event.preventDefault();
    openExternalUrl(externalLink.href);
    return;
  }

  const sidebarToggle = event.target.closest("[data-sidebar-toggle]");
  if (sidebarToggle) {
    event.preventDefault();
    setSidebarCollapsed(sidebarToggle.dataset.sidebarToggle === "collapse");
    return;
  }

  const routeButton = event.target.closest("[data-route]");
  if (routeButton) {
    if (isMobileToolsRoute(routeButton.dataset.route || "")) {
      await refreshMobileToolsAccess({ silent: true });
    }
    setRoute(routeButton.dataset.route);
    return;
  }

  const actionButton = event.target.closest("[data-action]");
  if (actionButton) {
    handleAction(actionButton);
    return;
  }

  const tabButton = event.target.closest("[data-tab]");
  if (tabButton) {
    state.activeTab = tabButton.dataset.tab;
    const detail = document.querySelector("#other-detail");
    if (detail) detail.innerHTML = boundDetailPanel(state.activeTab);
    hydrateIcons(detail);
    if (["evidence", "reports"].includes(state.activeTab)) loadEvidenceCases();
    if (state.activeTab === "history") {
      loadEvidenceCases();
      loadAcquisitionHistory();
    }
    return;
  }

  const analysisTabButton = event.target.closest("[data-analysis-tab]");
  if (analysisTabButton) {
    const imgInput = document.querySelector("#image-path");
    if (imgInput) state.imagePathInput = imgInput.value.trim();
    const ramInput = document.querySelector("#ram-analysis-path");
    if (ramInput) state.ramAnalysisPathInput = ramInput.value.trim();
    const ramOsSelect = document.querySelector("#ram-os-profile");
    if (ramOsSelect) state.ramOsProfile = ramOsSelect.value || "windows";
    const ramSymbolDir = document.querySelector("#ram-symbol-dir");
    if (ramSymbolDir) state.ramSymbolDirInput = ramSymbolDir.value.trim();

    state.activeAnalysisTab = analysisTabButton.dataset.analysisTab;
    render();
    return;
  }

  const treeNode = event.target.closest(".tree-node");
  if (treeNode && treeNode.closest("#image-tree-root")) {
    const isDir = treeNode.dataset.isDir === "true";
    const relativePath = treeNode.dataset.path;
    const isVirtual = treeNode.dataset.virtual === "true";
    document.querySelectorAll(".tree-node").forEach(el => el.classList.remove("active"));
    treeNode.classList.add("active");
    if (isVirtual) {
      if (isDir && treeNode.dataset.hasChildren === "true") {
        toggleExistingTreeChildren(treeNode);
      }
      showVirtualTreeInfo(treeNode);
      return;
    }
    if (isDir) {
      expandTreeNode(treeNode, relativePath);
    } else {
      previewImageFile(relativePath);
    }
    return;
  }

  const procRow = event.target.closest(".proc-row");
  if (procRow) {
    document.querySelectorAll(".proc-row").forEach(el => el.classList.remove("active"));
    procRow.classList.add("active");
    const pid = procRow.dataset.pid;
    const name = procRow.dataset.name;
    inspectProcessDetails(pid, name);
    return;
  }

  const carvedPreviewBtn = event.target.closest("[data-carved-preview]");
  if (carvedPreviewBtn) {
    const path = carvedPreviewBtn.dataset.carvedPreview;
    previewCarvedFile(path);
    return;
  }
});

document.addEventListener("change", async (event) => {
  const profileLanguage = event.target.closest("#profile-language");
  if (profileLanguage) {
    captureProfileDraft();
    state.profileDraft.language = profileLanguage.value;
    setLanguage(profileLanguage.value);
    renderProfileGate();
    return;
  }

  const profileTheme = event.target.closest("#profile-theme");
  if (profileTheme) {
    captureProfileDraft();
    state.profileDraft.theme = profileTheme.value;
    setTheme(profileTheme.value);
    renderProfileGate();
    return;
  }

  const profileDirect = event.target.closest("#profile-create-directly");
  if (profileDirect) {
    captureProfileDraft();
    return;
  }

  const profileOnlineDirect = event.target.closest("#profile-online-directly");
  if (profileOnlineDirect) {
    captureProfileDraft();
    return;
  }

  const select = event.target.closest("[data-action='language-select']");
  if (select) {
    try {
      await persistSettingsFromControls();
    } catch (error) {
      showToast(`Ayarlar kaydedilemedi: ${error.message}`, "error");
    }
    return;
  }

  const target = event.target.closest("[data-field='target']");
  if (target) {
    updateSide("target", target.value || t("targetNotSelected"));
  }

  const caseSelect = event.target.closest("[data-case-select]");
  if (caseSelect) {
    if (caseSelect.value === "__new__") {
      const promptTitle = t("case.promptNewName") || "Lütfen oluşturmak istediğiniz yeni vaka adını girin:";
      const newName = prompt(promptTitle);
      if (newName && newName.trim()) {
        const cleanName = newName.trim();
        state.pendingCaseName = cleanName;
        state.activeCase = { case_name: cleanName };
        
        // Mirror to all data-case-select fields on the page
        document.querySelectorAll("[data-case-select]").forEach((el) => {
          el.innerHTML = caseSelectOptions(cleanName, { allowNew: el.dataset.allowNewCase === "1" });
          el.value = cleanName;
        });

        // Set value to legacy hidden input
        const legacyInput = document.querySelector("#workflow-case-name");
        if (legacyInput) {
          legacyInput.value = cleanName;
        }
      } else {
        // Revert to first case or empty if cancelled
        const fallback = state.cases.length ? state.cases[0].case_name : "";
        state.pendingCaseName = "";
        state.activeCase = state.cases.find((c) => c.case_name === fallback) || null;
        caseSelect.value = fallback;
        document.querySelectorAll("[data-case-select]").forEach((el) => {
          el.value = fallback;
        });
      }
    } else {
      state.pendingCaseName = "";
      state.activeCase = state.cases.find((c) => c.case_name === caseSelect.value) || { case_name: caseSelect.value };
    }
    toggleCaseCreateInput(caseSelect);
  }

  const androidDeviceSelect = event.target.closest("[data-android-device-select]");
  if (androidDeviceSelect) {
    syncAndroidDeviceSelection(androidDeviceSelect, { state, t, showToast });
  }

  const iosBackupPath = event.target.closest("#ios-backup-path");
  if (iosBackupPath) {
    syncIosBackupPathInput(iosBackupPath, state);
  }

  const ramOsSelect = event.target.closest("#ram-os-profile");
  if (ramOsSelect) {
    state.ramOsProfile = ramOsSelect.value || "windows";
  }

  const ramSymbolDir = event.target.closest("#ram-symbol-dir");
  if (ramSymbolDir) {
    state.ramSymbolDirInput = ramSymbolDir.value.trim();
  }
});

document.addEventListener("input", (event) => {
  if (
    event.target.closest("#profile-full-name") ||
    event.target.closest("#profile-username") ||
    event.target.closest("#profile-online-identifier")
  ) {
    captureProfileDraft();
  }
  const iosBackupPath = event.target.closest("#ios-backup-path");
  if (iosBackupPath) {
    syncIosBackupPathInput(iosBackupPath, state);
  }
});

async function handleAction(button) {
  const action = button.dataset.action;
  if (action === "profile-create-start" || action === "profile-new") {
    state.profileGateMode = "create";
    showProfileGate();
    return;
  }
  if (action === "profile-online-start") {
    captureProfileDraft();
    state.profileGateMode = "online";
    showProfileGate();
    return;
  }
  if (action === "profile-online-back") {
    captureProfileDraft();
    state.profileGateMode = state.profiles.length ? "select" : "create";
    showProfileGate();
    return;
  }
  if (action === "profile-select-back") {
    state.profileGateMode = "select";
    showProfileGate();
    return;
  }
  if (action === "profile-select") {
    try {
      await selectProfile(button.dataset.username || "");
    } catch (error) {
      showToast(t("profile.selectFailed", { message: error.message }), "error");
      showProfileGate(error.message);
    }
    return;
  }
  if (action === "profile-submit") {
    try {
      await createProfileFromGate();
    } catch (error) {
      showToast(t("profile.createFailed", { message: error.message }), "error");
      showProfileGate(error.message);
    }
    return;
  }
  if (action === "profile-online-submit") {
    try {
      await connectOnlineProfileFromGate();
    } catch (error) {
      showToast(t("profile.onlineFailed", { message: error.message }), "error");
      showProfileGate(error.message);
    }
    return;
  }
  if (action === "profile-online-sync") {
    await syncOnlineProfile(button);
    return;
  }
  if (action === "profile-online-logout") {
    await disconnectOnlineProfile(button);
    return;
  }
  if (action === "profile-logout") {
    try {
      await apiRequest("/api/profiles/logout", { method: "POST" });
      state.activeProfile = null;
      state.activeCase = null;
      state.cases = [];
      state.mobileToolsAccess = { allowed: false, reason: "" };
      state.profileGateMode = state.profiles.length ? "select" : "create";
      syncProfileButton();
      showProfileGate();
    } catch (error) {
      showToast(t("profile.logoutFailed", { message: error.message }), "error");
    }
    return;
  }

  if (action?.startsWith("android-")) {
    if (!onlineMobileToolsAllowed()) {
      showToast(t("mobile.locked.toast"), "warning");
      state.profileGateMode = "online";
      showProfileGate();
      return;
    }
    const handled = await handleAndroidAction(button, {
      apiRequest,
      backendReady,
      state,
      t,
      showToast,
      render,
      resolveCase() {
        return resolveSelectedCaseName("#workflow-case") || null;
      }
    });
    if (handled) return;
  }

  if (action?.startsWith("ios-")) {
    if (!onlineMobileToolsAllowed()) {
      showToast(t("mobile.locked.toast"), "warning");
      state.profileGateMode = "online";
      showProfileGate();
      return;
    }
    const handled = await handleIosAction(button, {
      apiRequest,
      backendReady,
      state,
      t,
      showToast,
      render,
      resolveCase() {
        return resolveSelectedCaseName("#workflow-case") || null;
      }
    });
    if (handled) return;
  }

  if (action === "theme-toggle") {
    setTheme(state.theme === "dark" ? "light" : "dark");
    try {
      await persistSettingsFromControls();
    } catch (error) {
      showToast(`Ayarlar kaydedilemedi: ${error.message}`, "error");
      render();
    }
    return;
  }

  if (action === "pick-file") {
    await pickFile(button.dataset.target);
    return;
  }

  if (action === "pick-folder") {
    await pickFolder(button.dataset.target);
    return;
  }

  if (action === "toggle-vpn") {
    button.classList.toggle("on");
    const panel = document.querySelector(".vpn-panel");
    if (panel) panel.hidden = !button.classList.contains("on");
    writeWorkflowLog(button.classList.contains("on") ? t("vpn.enabled") : t("vpn.disabled"));
    updateSide("connection", button.classList.contains("on") ? t("vpn.waiting") : t("vpn.off"));
    return;
  }

  if (action === "vpn-config") {
    const panel = document.querySelector(".vpn-panel");
    if (panel) panel.hidden = false;
    document.querySelector("[data-action='toggle-vpn']")?.classList.add("on");
    writeWorkflowLog(t("vpn.opened"));
    return;
  }

  if (action === "save-vpn") {
    try {
      const payload = vpnPayload();
      const result = await apiRequest("/api/wireguard-config", {
        method: "POST",
        body: JSON.stringify(payload)
      });
      writeWorkflowLog(t("vpn.configured", { endpoint: payload.endpoint }));
      updateSide("connection", t("vpn.ready"));
      showToast(t("vpn.saved"));
      if (result.path) document.querySelector("#vpn-config-file").value = result.path;
    } catch (error) {
      showToast(t("vpn.failed", { message: error.message }), "error");
      writeWorkflowLog(t("vpn.failed", { message: error.message }));
    }
    return;
  }

  if (action === "start-vpn") {
    const configFile = document.querySelector("#vpn-config-file")?.value.trim();
    if (!configFile) {
      showToast(t("vpn.configRequired"), "error");
      return;
    }
    try {
      await apiRequest("/api/wireguard-start", {
        method: "POST",
        body: JSON.stringify({ config_file: configFile })
      });
      writeWorkflowLog(t("vpn.started"));
      updateSide("connection", t("vpn.ready"));
      showToast(t("vpn.started"));
    } catch (error) {
      showToast(t("vpn.failed", { message: error.message }), "error");
      writeWorkflowLog(t("vpn.failed", { message: error.message }));
    }
    return;
  }

  if (action === "stop-vpn") {
    try {
      await apiRequest("/api/wireguard-stop", { method: "POST" });
      writeWorkflowLog(t("vpn.stopped"));
      updateSide("connection", t("vpn.off"));
      showToast(t("vpn.stopped"));
    } catch (error) {
      showToast(t("vpn.failed", { message: error.message }), "error");
      writeWorkflowLog(t("vpn.failed", { message: error.message }));
    }
    return;
  }

  if (action === "approve-key") {
    const token = document.querySelector("[data-field='token']");
    const value = token?.value.trim() || "";
    if (!value) {
      showToast(t("key.required"), "error");
      return;
    }
    state.approvedSecurityKey = value;
    if (token) token.readOnly = true;
    writeWorkflowLog(t("key.approved"));
    showToast(t("key.active"));
    return;
  }

  if (action === "reset-key") {
    const token = document.querySelector("[data-field='token']");
    state.approvedSecurityKey = "";
    if (token) {
      token.value = "";
      token.readOnly = false;
    }
    forgetConnection();
    writeWorkflowLog(t("key.reset"));
    return;
  }

  if (action === "connect") {
    const workflowId = currentWorkflowId();
    const workflow = currentWorkflow();
    let payload;
    try {
      payload = connectionPayload();
    } catch (error) {
      showToast(error.message, "error");
      return;
    }
    if (!payload.ip || !payload.port) {
      showToast(t("connection.ipPortRequired"), "error");
      return;
    }
    if (payload.port <= 0 || payload.port > 65535) {
      showToast(t("connection.invalidPort"), "error");
      return;
    }
    if (!workflow?.mode.startsWith("remote")) {
      showToast(t("connection.remoteOnly"), "error");
      return;
    }

    forgetConnection(workflowId);
    button.disabled = true;
    updateSide("connection", t("connection.connecting"));
    writeWorkflowLog(t("connection.starting", { host: `${payload.ip}:${payload.port}` }));
    try {
      const result = await apiRequest("/api/connect", {
        method: "POST",
        body: JSON.stringify(payload)
      });
      rememberConnection(workflowId, payload, result);
      updateSide("connection", t("connection.connected", { ip: payload.ip }));
      writeWorkflowLog(t("connection.connectedLog", { ip: payload.ip }));
      showToast(t("connection.success"));
    } catch (error) {
      forgetConnection(workflowId);
      updateSide("connection", t("connection.failed"));
      writeWorkflowLog(t("connection.failedLog", { ip: payload.ip, message: error.message }));
      showToast(t("connection.cannotConnect", { message: error.message }), "error");
    } finally {
      button.disabled = false;
    }
    return;
  }

  if (action === "scan") {
    await scanTargets();
    return;
  }

  if (action === "download") {
    await installWinpmem(button);
    return;
  }

  if (action === "install-avml") {
    await installAvml(button);
    return;
  }

  if (action === "start") {
    await startAcquisition(button);
    return;
  }

  if (action === "pause") {
    try {
      await sendAcquisitionControl("pause");
    } catch (error) {
      showToast(t("workflow.pauseFailed", { message: error.message }), "error");
    }
    return;
  }

  if (action === "resume") {
    try {
      await sendAcquisitionControl("resume");
    } catch (error) {
      showToast(t("workflow.resumeFailed", { message: error.message }), "error");
    }
    return;
  }

  if (action === "stop") {
    try {
      await sendAcquisitionControl("stop");
    } catch (error) {
      showToast(t("workflow.stopFailed", { message: error.message }), "error");
    }
    return;
  }

  if (action === "mount-readonly") {
    const imagePath = document.querySelector("#image-path")?.value.trim();
    if (!imagePath || imagePath.startsWith(".")) {
      showToast(t("analysis.imageRequired"), "error");
      return;
    }
    setAnalysisStatus(t("analysis.mounting"), t("analysis.mounting"));
    try {
      const result = await apiRequest("/api/image-mount-readonly", {
        method: "POST",
        body: JSON.stringify({ path: imagePath })
      });
      state.imageMount = {
        imagePath: result.image_path,
        mountDir: result.mount_dir || "",
        mountMode: result.mount_mode || "mounted",
        label: result.mount_mode === "analysis-only"
          ? t("analysis.analysisOnlyStatus")
          : t("analysis.mounted", { path: result.mount_dir })
      };
      state.imageMountLogHTML = renderMountResultInfo(result);
      state.imageMountTreeHTML = renderTree(result.tree);
      const container = document.querySelector("#image-tree-root");
      if (container) {
        container.innerHTML = state.imageMountTreeHTML;
      }
      const summaryContainer = document.querySelector("#disk-analysis-results");
      if (summaryContainer && result.analysis) {
        summaryContainer.style.display = "block";
        summaryContainer.innerHTML = renderDiskAnalysisSummary(result.analysis);
        hydrateIcons(summaryContainer);
      }
      setAnalysisStatus(
        state.imageMount.label,
        state.imageMountLogHTML
      );
      showToast(result.mount_mode === "analysis-only" ? t("analysis.analysisOnlyPrepared") : t("analysis.mountPrepared"));
    } catch (error) {
      state.imageMountLogHTML = "";
      setAnalysisStatus(t("analysis.noImage"), renderErrorPanel(t("analysis.errorTitle"), error));
      showToast(t("analysis.mountFailed", { message: error.message }), "error");
    }
    return;
  }

  if (action === "unmount-image") {
    try {
      await apiRequest("/api/image-unmount", { method: "POST" });
      state.imageMount = null;
      state.imageMountTreeHTML = "";
      state.imageMountLogHTML = "";
      const container = document.querySelector("#image-tree-root");
      if (container) {
        container.innerHTML = `<div class="log-box">${t("analysis.outputWaiting")}</div>`;
      }
      const preview = document.querySelector("#image-file-preview");
      if (preview) {
        preview.innerHTML = `
          <div class="log-box" style="display:flex;align-items:center;justify-content:center;color:var(--muted);text-align:center;padding:20px">
            Klasör yapısında bir dosyaya tıklayarak içeriğini inceleyebilirsiniz.<br/>Click a file on the left to preview it.
          </div>
        `;
      }
      const summary = document.querySelector("#disk-analysis-results");
      if (summary) {
        summary.style.display = "none";
        summary.innerHTML = "";
      }
      setAnalysisStatus(t("analysis.unmounted"), t("analysis.noActiveMount"));
      showToast(t("analysis.unmounted"));
    } catch (error) {
      showToast(t("analysis.unmountFailed", { message: error.message }), "error");
    }
    return;
  }

  if (action === "image-analyze") {
    const imagePath = document.querySelector("#image-path")?.value.trim();
    if (!imagePath || imagePath.startsWith(".")) {
      showToast(t("analysis.imageRequired"), "error");
      return;
    }
    const container = document.querySelector("#disk-analysis-results");
    if (container) {
      container.style.display = "block";
      container.innerHTML = `<div class="log-box">${escapeHtml(t("analysis.runningAnalysis"))}</div>`;
    }
    try {
      const result = await apiRequest("/api/image-analyze", {
        method: "POST",
        body: JSON.stringify({ path: imagePath })
      });
      if (container) {
        container.innerHTML = renderDiskAnalysisSummary(result);
        hydrateIcons(container);
      }
      showToast(t("analysis.doneAnalysis"));
    } catch (error) {
      if (container) container.innerHTML = renderErrorPanel(t("analysis.errorTitle"), error);
      showToast(t("analysis.summaryFailed", { message: error.message }), "error");
    }
    return;
  }

  if (action === "ram-summary") {
    const ramPath = document.querySelector("#ram-analysis-path")?.value.trim();
    if (!ramPath || ramPath.startsWith(".")) {
      showToast("Önce geçerli bir RAM dosyası seçin / Select a valid RAM file first", "error");
      return;
    }
    const osProfile = document.querySelector("#ram-os-profile")?.value || "windows";
    const symbolDir = ramSymbolDirValue();
    document.querySelector("#ram-analysis-results").style.display = "block";
    document.querySelector("#ram-split-view").style.display = "none";
    document.querySelector("#ram-flat-results-panel").style.display = "block";
    const statusLbl = document.querySelector("#stat-status-lbl");
    if (statusLbl) statusLbl.textContent = t("analysis.runningAnalysis");
    const flatTitle = document.querySelector("#ram-flat-title");
    const flatResults = document.querySelector("#ram-flat-results-list");
    if (flatTitle) flatTitle.textContent = t("analysis.ramSummary");
    
    if (flatResults) flatResults.innerHTML = ramConsoleHtml("Canlı Analiz Konsolu", [
      "Volatility3 ile uçucu bellek analizi başlatılıyor...",
      "İlk çalıştırmada sembol çözümleme/indirme sürebilir."
    ]);
    try {
      const start = await apiRequest("/api/ram-analyze-summary-start", {
        method: "POST",
        body: JSON.stringify({ path: ramPath, os_type: osProfile, symbol_dir: symbolDir })
      });
      if (!start.job_id) throw new Error(t("workflow.jobIdMissing"));
      const result = await waitForAcquisitionJob(start.job_id, {
        onUpdate(job) {
          updateRamConsole("#ram-flat-results-list", job, "Canlı Analiz Konsolu");
          if (statusLbl && job.message) statusLbl.textContent = job.message;
        }
      });
      document.querySelector("#stat-strings-count").textContent = String(result.string_match_count || 0);
      document.querySelector("#stat-carved-count").textContent = "-";
      document.querySelector("#stat-procs-count").textContent = String(result.process_count || 0);
      if (statusLbl) statusLbl.textContent = t("analysis.doneAnalysis");
      if (flatResults) {
        flatResults.innerHTML = renderRamAnalysisSummary(result);
        hydrateIcons(flatResults);
      }
      showToast(t("analysis.doneAnalysis"));
    } catch (error) {
      if (statusLbl) statusLbl.textContent = "Hata / Failed";
      if (flatResults) {
        flatResults.innerHTML += renderErrorPanel(t("analysis.errorTitle"), error);
      }
      showToast(t("analysis.summaryFailed", { message: error.message }), "error");
    }
    return;
  }

  if (action === "ram-preflight") {
    const ramPath = document.querySelector("#ram-analysis-path")?.value.trim();
    if (!ramPath || ramPath.startsWith(".")) {
      showToast("Önce geçerli bir RAM dosyası seçin / Select a valid RAM file first", "error");
      return;
    }
    const osProfile = document.querySelector("#ram-os-profile")?.value || "windows";
    const symbolDir = ramSymbolDirValue();
    document.querySelector("#ram-analysis-results").style.display = "block";
    document.querySelector("#ram-split-view").style.display = "none";
    document.querySelector("#ram-flat-results-panel").style.display = "block";
    const flatTitle = document.querySelector("#ram-flat-title");
    const flatResults = document.querySelector("#ram-flat-results-list");
    const statusLbl = document.querySelector("#stat-status-lbl");
    if (flatTitle) flatTitle.textContent = t("analysis.preflightTitle");
    if (statusLbl) statusLbl.textContent = "Ön kontrol / Preflight";
    if (flatResults) flatResults.innerHTML = ramConsoleHtml(t("analysis.preflightTitle"), [
      "Volatility yolu, symbol dizini ve Linux kernel banner bilgisi kontrol ediliyor..."
    ]);
    try {
      const result = await apiRequest("/api/ram-volatility-preflight", {
        method: "POST",
        body: JSON.stringify({ path: ramPath, os_type: osProfile, symbol_dir: symbolDir })
      });
      if (statusLbl) statusLbl.textContent = result.ready ? "Hazır / Ready" : "Eksik / Missing";
      if (flatResults) flatResults.innerHTML = renderVolatilityPreflight(result);
      showToast(result.ready ? "Volatility ön kontrol hazır." : "Volatility ön kontrol eksik uyarılar verdi.", result.ready ? "success" : "error");
    } catch (error) {
      if (statusLbl) statusLbl.textContent = "Hata / Failed";
      if (flatResults) flatResults.innerHTML = renderErrorPanel(t("analysis.preflightFailedTitle"), error);
      showToast("Volatility ön kontrol başarısız: " + error.message, "error");
    }
    return;
  }

  if (action === "ram-symbol-install") {
    const ramPath = document.querySelector("#ram-analysis-path")?.value.trim();
    if (!ramPath || ramPath.startsWith(".")) {
      showToast("Önce geçerli bir RAM dosyası seçin / Select a valid RAM file first", "error");
      return;
    }
    const osProfile = document.querySelector("#ram-os-profile")?.value || "windows";
    const symbolDir = ramSymbolDirValue();
    document.querySelector("#ram-analysis-results").style.display = "block";
    document.querySelector("#ram-split-view").style.display = "none";
    document.querySelector("#ram-flat-results-panel").style.display = "block";
    const flatTitle = document.querySelector("#ram-flat-title");
    const flatResults = document.querySelector("#ram-flat-results-list");
    const statusLbl = document.querySelector("#stat-status-lbl");
    if (flatTitle) flatTitle.textContent = t("analysis.symbolInstallTitle");
    if (statusLbl) statusLbl.textContent = t("analysis.symbolInstallRunning");
    if (flatResults) flatResults.innerHTML = ramConsoleHtml(t("analysis.symbolInstallTitle"), [
      t("analysis.symbolInstallScanning"),
      t("analysis.symbolInstallExact")
    ]);
    try {
      const result = await apiRequest("/api/ram-volatility-symbol-install", {
        method: "POST",
        body: JSON.stringify({ path: ramPath, os_type: osProfile, symbol_dir: symbolDir })
      });
      if (result.symbol_dir) {
        state.ramSymbolDirInput = result.symbol_dir;
        const input = document.querySelector("#ram-symbol-dir");
        if (input) input.value = result.symbol_dir;
      }
      const symbolReady = result.installed || result.status === "windows-automatic";
      if (statusLbl) statusLbl.textContent = symbolReady ? t("analysis.symbolInstallDone") : t("analysis.symbolInstallMissing");
      if (flatResults) flatResults.innerHTML = renderVolatilitySymbolInstall(result);
      showToast(result.message || t("analysis.symbolInstallDone"), symbolReady ? "success" : "error");
    } catch (error) {
      if (statusLbl) statusLbl.textContent = "Hata / Failed";
      if (flatResults) flatResults.innerHTML = renderErrorPanel(t("analysis.symbolInstallFailedTitle"), error);
      showToast(t("analysis.symbolInstallFailed", { message: error.message }), "error");
    }
    return;
  }

  if (action === "android-analysis") {
    const caseName = document.querySelector("#android-analysis-case")?.value.trim();
    if (!caseName) {
      showToast(t("analysis.androidRequired"), "error");
      return;
    }
    const container = document.querySelector("#android-analysis-results");
    if (container) container.innerHTML = `<div class="log-box">${escapeHtml(t("analysis.runningAnalysis"))}</div>`;
    try {
      const result = await apiRequest("/api/android-case-analysis", {
        method: "POST",
        body: JSON.stringify({ case_name: caseName })
      });
      if (container) {
        container.innerHTML = renderAndroidAnalysisSummary(result);
        hydrateIcons(container);
      }
      showToast(t("analysis.doneAnalysis"));
    } catch (error) {
      if (container) container.innerHTML = renderErrorPanel(t("analysis.errorTitle"), error);
      showToast(t("analysis.summaryFailed", { message: error.message }), "error");
    }
    return;
  }

  if (action === "ram-strings") {
    const ramPath = document.querySelector("#ram-analysis-path")?.value.trim();
    if (!ramPath || ramPath.startsWith(".")) {
      showToast("Önce geçerli bir RAM dosyası seçin / Select a valid RAM file first", "error");
      return;
    }
    
    document.querySelector("#ram-analysis-results").style.display = "block";
    document.querySelector("#ram-split-view").style.display = "none";
    document.querySelector("#ram-flat-results-panel").style.display = "block";
    
    const statusLbl = document.querySelector("#stat-status-lbl");
    if (statusLbl) statusLbl.textContent = t("analysis.runningAnalysis");

    const flatResults = document.querySelector("#ram-flat-results-list");
    flatResults.innerHTML = `<div class="log-box" style="text-align:center;padding:30px">⌛ Uçucu bellek taranıyor, dizgiler çıkartılıyor... (Bu işlem bir miktar sürebilir)<br/>Scanning dynamic memory heap and extracting evidential strings...</div>`;

    try {
      const result = await apiRequest("/api/ram-analyze-strings", {
        method: "POST",
        body: JSON.stringify({ path: ramPath })
      });
      
      const count = result.length || 0;
      document.querySelector("#stat-strings-count").textContent = count;
      document.querySelector("#stat-carved-count").textContent = "0";
      document.querySelector("#stat-procs-count").textContent = "0";
      if (statusLbl) statusLbl.textContent = "Analiz Edildi / Analysed";

      if (count === 0) {
        flatResults.innerHTML = `<div class="log-box" style="text-align:center;padding:20px;color:var(--muted)">Hiçbir bulgu dizgisi bulunamadı / No evidential strings found.</div>`;
      } else {
        flatResults.innerHTML = result.map(item => `
          <div class="string-match-item">
            <div class="match-meta">
              <span>Kategori: <strong>${escapeHtml(item.category)}</strong></span>
              <span>Ofset: <strong>0x${item.offset.toString(16).toUpperCase()}</strong></span>
            </div>
            <div class="match-value">${escapeHtml(item.value)}</div>
            <div class="match-context">${escapeHtml(item.context)}</div>
          </div>
        `).join("");
      }
      showToast("Dizgi analizi başarıyla tamamlandı.");
    } catch (error) {
      if (statusLbl) statusLbl.textContent = "Hata / Failed";
      flatResults.innerHTML = renderErrorPanel(t("analysis.stringsFailedTitle"), error);
      showToast("RAM dizgi analizi başarısız oldu: " + error.message, "error");
    }
    return;
  }

  if (action === "ram-carver") {
    const ramPath = document.querySelector("#ram-analysis-path")?.value.trim();
    if (!ramPath || ramPath.startsWith(".")) {
      showToast("Önce geçerli bir RAM dosyası seçin / Select a valid RAM file first", "error");
      return;
    }

    document.querySelector("#ram-analysis-results").style.display = "block";
    document.querySelector("#ram-split-view").style.display = "grid";
    document.querySelector("#ram-flat-results-panel").style.display = "none";

    document.querySelector("#ram-left-panel-title").textContent = t("analysis.lblCarved");
    document.querySelector("#ram-right-panel-title").textContent = "Dosya Önizleme / File Preview";

    const leftList = document.querySelector("#ram-left-list");
    leftList.innerHTML = `<div class="log-box" style="text-align:center;padding:20px">⌛ Bellekten gömülü dosyalar kurtarılıyor... / Carving files from memory...</div>`;
    const rightContent = document.querySelector("#ram-right-content");
    rightContent.innerHTML = `<div class="log-box" style="display:flex;align-items:center;justify-content:center;color:var(--muted);text-align:center">Kurtarılan bir dosyaya tıklayarak içeriğini inceleyin.<br/>Click a carved file on the left to preview.</div>`;

    const statusLbl = document.querySelector("#stat-status-lbl");
    if (statusLbl) statusLbl.textContent = t("analysis.runningAnalysis");

    try {
      const result = await apiRequest("/api/ram-carve-files", {
        method: "POST",
        body: JSON.stringify({ path: ramPath })
      });

      const count = result.length || 0;
      document.querySelector("#stat-strings-count").textContent = "0";
      document.querySelector("#stat-carved-count").textContent = count;
      document.querySelector("#stat-procs-count").textContent = "0";
      if (statusLbl) statusLbl.textContent = "Kurtarıldı / Carved";

      if (count === 0) {
        leftList.innerHTML = `<div class="log-box" style="text-align:center;padding:12px;color:var(--muted)">Kurtarılan dosya bulunamadı / No carved files.</div>`;
      } else {
        leftList.innerHTML = result.map(file => {
          const isImg = file.mime_type.startsWith("image/");
          const fileIcon = isImg ? "🖼️" : "📄";
          return `
            <div class="tree-node" data-carved-preview="${escapeHtml(file.file_path)}" style="padding:10px;border-bottom:1px solid var(--line)">
              <span class="node-icon">${fileIcon}</span>
              <div style="display:flex;flex-direction:column;min-width:0;flex:1">
                <strong style="font-size:12px;color:var(--text);overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${escapeHtml(file.file_name)}</strong>
                <small style="font-size:10px;color:var(--muted)">Ofset: 0x${file.offset.toString(16).toUpperCase()} · ${formatBytes(file.size)}</small>
              </div>
            </div>
          `;
        }).join("");
      }
      showToast("Dosya kurtarma (carving) başarıyla tamamlandı.");
    } catch (error) {
      if (statusLbl) statusLbl.textContent = "Hata / Failed";
      leftList.innerHTML = renderErrorPanel(t("analysis.carvingFailedTitle"), error);
      showToast("RAM dosya kurtarma başarısız: " + error.message, "error");
    }
    return;
  }

  if (action === "ram-processes") {
    const ramPath = document.querySelector("#ram-analysis-path")?.value.trim();
    if (!ramPath || ramPath.startsWith(".")) {
      showToast("Önce geçerli bir RAM dosyası seçin / Select a valid RAM file first", "error");
      return;
    }
    const osProfile = document.querySelector("#ram-os-profile")?.value || "windows";
    const symbolDir = ramSymbolDirValue();

    document.querySelector("#ram-analysis-results").style.display = "block";
    document.querySelector("#ram-split-view").style.display = "grid";
    document.querySelector("#ram-flat-results-panel").style.display = "none";

    document.querySelector("#ram-left-panel-title").textContent = t("analysis.lblProcesses");
    document.querySelector("#ram-right-panel-title").textContent = "Proses Detayları / Process Inspector";

    const leftList = document.querySelector("#ram-left-list");
    leftList.innerHTML = ramConsoleHtml("Canlı Proses Analiz Konsolu", [
      "Volatility3 ile proses tablosu çıkartılıyor..."
    ]);
    const rightContent = document.querySelector("#ram-right-content");
    rightContent.innerHTML = `<div class="log-box" style="display:flex;align-items:center;justify-content:center;color:var(--muted);text-align:center">Proses seçildiğinde bellek haritası ve arama alanları burada açılacak.<br/>Select a process from the left to inspect memory maps.</div>`;

    const statusLbl = document.querySelector("#stat-status-lbl");
    if (statusLbl) statusLbl.textContent = t("analysis.runningAnalysis");

    try {
      const start = await apiRequest("/api/ram-list-processes-start", {
        method: "POST",
        body: JSON.stringify({ path: ramPath, os_type: osProfile, symbol_dir: symbolDir })
      });
      if (!start.job_id) throw new Error(t("workflow.jobIdMissing"));
      const result = await waitForAcquisitionJob(start.job_id, {
        onUpdate(job) {
          updateRamConsole("#ram-left-list", job, "Canlı Proses Analiz Konsolu");
          if (statusLbl && job.message) statusLbl.textContent = job.message;
        }
      });

      const count = result.length || 0;
      document.querySelector("#stat-strings-count").textContent = "0";
      document.querySelector("#stat-carved-count").textContent = "0";
      document.querySelector("#stat-procs-count").textContent = count;
      if (statusLbl) statusLbl.textContent = "Hazır / Ready";

      if (count === 0) {
        leftList.innerHTML = `<div class="log-box" style="text-align:center;padding:12px;color:var(--muted)">Bu arşivde proses bulunamadı (Sadece .tar arşivleri desteklenir) / No processes.</div>`;
      } else {
        leftList.innerHTML = result.map(proc => `
          <div class="proc-row" data-pid="${escapeHtml(proc.pid)}" data-name="${escapeHtml(proc.name)}">
            <strong>${escapeHtml(proc.pid)}</strong>
            <span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${escapeHtml(proc.name)}</span>
            <small style="text-align:right">${formatBytes(proc.dump_size)}</small>
          </div>
        `).join("");
      }
    } catch (error) {
      if (statusLbl) statusLbl.textContent = "Hata / Failed";
      leftList.innerHTML = renderErrorPanel(t("analysis.processFailedTitle"), error);
      showToast("Proses listeleme başarısız: " + error.message, "error");
    }
    return;
  }

  if (action === "hash") {
    await calculateHashes();
    return;
  }

  if (action === "compare") {
    compareHash();
    return;
  }

  if (action === "save-settings") {
    try {
      await persistSettingsFromControls();
    } catch (error) {
      const message = `Ayarlar kaydedilemedi: ${error.message}`;
      setStatus("[data-settings-status]", `${icon("info")} ${escapeHtml(message)}`);
      showToast(message, "error");
    }
    return;
  }

  if (action === "check-update") {
    try {
      setStatus("[data-update-status]", `${icon("refresh")} ${t("settings.updateChecked")}`);
      const result = await apiRequest("/api/update-check");
      state.latestUpdate = result;
      state.updateTarget = result.update_target || state.updateTarget;
      render();
      const asset = result.platform_asset || {};
      const target = result.update_target || {};
      const missingAsset = result.asset_error || t("settings.noAssetForPackage", {
        package: target.asset_package_label || target.package_label || "-"
      });
      const assetLine = asset.name ? `<br />Asset: ${escapeHtml(asset.name)} (${formatBytes(asset.size)})` : `<br />${escapeHtml(missingAsset)}`;
      const packageLine = target.asset_package_label || target.package_label
        ? `<br />${t("settings.package")}: ${escapeHtml(target.asset_package_label || target.package_label)}`
        : "";
      const commandLine = target.install_command
        ? `<br />${t("settings.installCommand")}: <code>${escapeHtml(target.install_command)}</code>`
        : "";
      setStatus("[data-update-status]", `${icon("info")} ${t("settings.latestVersion", { version: result.tag_name || result.name || "-" })}`);
      setStatus("[data-update-log]", `${escapeHtml(result.body || t("settings.releaseNotes")).replaceAll("\n", "<br />")}${assetLine}${packageLine}${commandLine}`);
      showToast(t("settings.updateDone"));
    } catch (error) {
      setStatus("[data-update-status]", `${icon("info")} ${t("settings.updateFailed", { message: escapeHtml(error.message) })}`);
      showToast(t("settings.updateFailed", { message: error.message }), "error");
    }
    return;
  }

  if (action === "download-update") {
    await downloadUpdatePackage();
    return;
  }

  if (action === "list-files") {
    await listEvidenceFiles();
    return;
  }

  if (action === "refresh-cases") {
    await loadEvidenceCases({ silent: false });
    return;
  }

  if (action === "create-case") {
    await createEvidenceCase();
    return;
  }

  if (action === "create-manifest") {
    await createCaseManifest();
    return;
  }

  if (action === "load-history") {
    await loadAcquisitionHistory({ silent: false });
    return;
  }

  if (action === "add-note") {
    await addEvidenceNote();
    return;
  }

  if (action === "create-report") {
    await createEvidenceReport();
    return;
  }

  const label = button.textContent.trim().replace(/\s+/g, " ");
  writeWorkflowLog(`${label}: ${t("ready")}`);
  showToast(`${label}: ${t("ready")}`);
}

function renderErrorPanel(title, errorOrMessage) {
  const message = errorOrMessage?.message || errorOrMessage || t("unknown");
  return errorBoxHtml(title, message);
}

function installUiErrorHandlers() {
  window.addEventListener("error", (event) => {
    const location = event.filename
      ? `${event.filename}:${event.lineno || 0}:${event.colno || 0}`
      : "bilinmeyen dosya";
    showToast(`Arayüz hatası: ${event.message}\nKonum: ${location}`, "error");
  });
  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason?.message || event.reason || "Bilinmeyen hata";
    showToast(`Arayüz işlemi tamamlanamadı:\n${reason}`, "error");
  });
}

async function pickFile(targetSelector) {
  const target = targetSelector ? document.querySelector(targetSelector) : null;
  if (backendReady()) {
    try {
      const result = await apiRequest("/api/pick-file", { method: "POST" });
      if (target) {
        target.value = result.path;
        delete state.files[targetSelector];
      }
      showToast(t("workflow.selectFile", { path: result.path }));
      return result.path;
    } catch (error) {
      if (String(error?.message || "").includes("cancelled")) return null;
      showToast(t("workflow.filePickerFailed", { message: error.message }), "error");
      return null;
    }
  }

  try {
    if (window.showOpenFilePicker) {
      const [handle] = await window.showOpenFilePicker({ multiple: false });
      const file = await handle.getFile();
      if (target) {
        target.value = file.name;
        state.files[targetSelector] = file;
      }
      showToast(t("workflow.selectFile", { path: file.name }));
      return file;
    }
  } catch (error) {
    if (error?.name === "AbortError") return null;
    showToast(t("workflow.filePickerFailedShort"), "error");
    return null;
  }

  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.style.position = "fixed";
    input.style.opacity = "0";
    input.addEventListener("change", () => {
      const file = input.files?.[0] || null;
      if (file && target) {
        target.value = file.name;
        state.files[targetSelector] = file;
        showToast(t("workflow.selectFile", { path: file.name }));
      }
      input.remove();
      resolve(file);
    });
    document.body.appendChild(input);
    input.click();
  });
}

async function pickFolder(targetSelector) {
  const target = targetSelector ? document.querySelector(targetSelector) : null;
  if (backendReady()) {
    try {
      const result = await apiRequest("/api/pick-folder", { method: "POST" });
      if (target) {
        target.value = result.path;
        if (targetSelector === "#ios-backup-path") syncIosBackupPathInput(target, state);
      }
      showToast(t("workflow.selectFolder", { path: result.path }));
      return result.path;
    } catch (error) {
      if (String(error?.message || "").includes("cancelled")) return null;
      showToast(t("workflow.folderPickerFailed", { message: error.message }), "error");
      return null;
    }
  }

  try {
    if (window.showDirectoryPicker) {
      const handle = await window.showDirectoryPicker();
      if (target) {
        target.value = handle.name;
        if (targetSelector === "#ios-backup-path") syncIosBackupPathInput(target, state);
      }
      showToast(t("workflow.selectFolder", { path: handle.name }));
      return handle;
    }
  } catch (error) {
    if (error?.name === "AbortError") return null;
    showToast(t("workflow.folderPickerFailedShort"), "error");
    return null;
  }

  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.webkitdirectory = true;
    input.style.position = "fixed";
    input.style.opacity = "0";
    input.addEventListener("change", () => {
      const first = input.files?.[0];
      const folder = first?.webkitRelativePath?.split("/")?.[0] || first?.name || "";
      if (folder && target) {
        target.value = folder;
        if (targetSelector === "#ios-backup-path") syncIosBackupPathInput(target, state);
      }
      if (folder) showToast(t("workflow.selectFolder", { path: folder }));
      input.remove();
      resolve(folder);
    });
    document.body.appendChild(input);
    input.click();
  });
}

function writeWorkflowLog(message) {
  const next = compactLogLine(message);
  if (!next) return;
  if (state.lastLog[0] !== next) state.lastLog.unshift(next);
  state.lastLog = state.lastLog.slice(0, 5);
  const log = document.querySelector("#workflow-log");
  if (log) log.innerHTML = state.lastLog.map((line) => escapeHtml(line)).join("<br />");
  updateSide("last-action", escapeHtml(next));
}

function updateSide(key, value) {
  const item = document.querySelector(`[data-side="${key}"] small`);
  if (item) item.innerHTML = value;
}

function setAcquisitionControlsVisible(active, startButton = document.querySelector("[data-action='start']")) {
  const controls = document.querySelector("[data-acquisition-controls]");
  if (controls) controls.hidden = !active;
  if (startButton) {
    startButton.hidden = active;
    startButton.disabled = active;
  }
}

async function scanTargets() {
  const routeId = state.route.split(":")[1];
  const workflow = workflows[routeId];
  const isRam = workflow?.mode.includes("ram");

  if (isRam) {
    if (backendReady()) {
      try {
        const toolKey = workflow.platform === "Windows" ? "winpmem" : "avml";
        let status;
        if (workflow.mode.startsWith("remote")) {
          const payload = connectionPayload();
          if (!payload.ip || !payload.port) {
            showToast(t("connection.ipPortRequired"), "error");
            return;
          }
          if (!requireActiveConnection(workflow, payload)) return;
          const result = await apiRequest("/api/remote-tool-check", {
            method: "POST",
            body: JSON.stringify({ ...payload, tool: toolKey })
          });
          status = result.status;
          updateSide("connection", t("connection.checked", { host: `${payload.ip}:${payload.port}` }));
        } else {
          const result = await apiRequest("/api/ram-status");
          status = result[toolKey];
        }
        const toolName = workflow.platform === "Windows" ? "WinPMEM" : "AVML";
        const statusMessage = String(status?.message || "");
        const missingTool = status?.tool_present === false || /not found|bulunamad/i.test(statusMessage);
        if (missingTool) {
          updateSide("target", t("scan.toolMissing", { tool: toolName }));
          writeWorkflowLog(t("scan.toolMissing", { tool: toolName }));
          showToast(t("scan.toolMissing", { tool: toolName }), "error");
          return;
        }
        const label = status?.tool_path || statusMessage || t("scan.toolReady", { tool: toolName });
        updateSide("target", escapeHtml(label));
        writeWorkflowLog(t("scan.toolDoneLog", { target: toolName, message: statusMessage || t("ready") }));
        showToast(t("scan.toolReady", { tool: toolName }));
      } catch (error) {
        if (workflow?.mode.startsWith("remote")) {
          forgetConnection();
          updateSide("connection", t("connection.toolFailed"));
        }
        showToast(t("scan.failed", { message: error.message }), "error");
        writeWorkflowLog(t("scan.ramFailedLog", { message: error.message }));
      }
      return;
    }

    updateSide("target", t("localCheckWaiting"));
    writeWorkflowLog(t("scan.appModeRequired"));
    showToast(t("workflow.appModeRequired"), "error");
    return;
  }

  const select = document.querySelector("[data-field='target']");
  if (!select) return;

  if (backendReady()) {
    try {
      let disks = [];
      if (workflow.mode.startsWith("remote")) {
        const payload = connectionPayload();
        if (!payload.ip || !payload.port) {
          showToast(t("connection.ipPortRequired"), "error");
          return;
        }
        if (!requireActiveConnection(workflow, payload)) return;
        const result = await apiRequest("/api/remote-disks", {
          method: "POST",
          body: JSON.stringify(payload)
        });
        disks = result.disks || [];
        updateSide("connection", t("connection.alive", { host: `${payload.ip}:${payload.port}` }));
      } else {
        const result = await apiRequest("/api/disk-list");
        disks = result.disks || [];
        if (result.elevated) {
          writeWorkflowLog(t("scan.elevated"));
        } else if (result.elevation_error) {
          writeWorkflowLog(t("scan.elevationFailed", { message: result.elevation_error }));
          showToast(t("scan.elevationFailed", { message: result.elevation_error }), "error");
        }
      }

      const options = disks
        .map((disk) => {
          const value = disk.id || disk.device || disk.name || disk.path || "";
          if (!value) return "";
          const size = disk.boyut || disk.total_size || 0;
          const name = disk.ad || disk.device || disk.name || value;
          const access = disk.accessible === false ? ` ${t("scan.accessDenied")}` : "";
          return `<option value="${escapeHtml(value)}" data-disk-name="${escapeHtml(name)}">${escapeHtml(name)} · ${formatBytes(size)}${access}</option>`;
        })
        .filter(Boolean);

      if (options.length === 0) {
        select.innerHTML = `<option value="" disabled selected>${t("scan.noDisk")}</option>`;
        updateSide("target", t("targetNotSelected"));
        writeWorkflowLog(t("scan.noDiskLog"));
        showToast(t("scan.noDisk"), "error");
        return;
      }

      select.innerHTML = options.join("");
      updateSide("target", select.value);
      writeWorkflowLog(t("scan.diskDoneLog"));
      showToast(t("scan.diskDone"));
      return;
    } catch (error) {
      if (workflow?.mode.startsWith("remote")) {
        forgetConnection();
        updateSide("connection", t("connection.disksFailed"));
      }
      showToast(t("scan.diskFailed", { message: error.message }), "error");
      writeWorkflowLog(t("scan.diskFailed", { message: error.message }));
      return;
    }
  }

  const tauriInvoke = window.__TAURI__?.core?.invoke || window.__TAURI__?.tauri?.invoke;
  if (tauriInvoke) {
    try {
      const disks = await tauriInvoke(workflow.mode.startsWith("remote") ? "remote_disk_list" : "local_disk_list", {
        platform: workflow.platform.toLowerCase()
      });
      const targets = Array.isArray(disks) ? disks.map((disk) => disk.id || disk.name || disk.path || disk).filter(Boolean) : [];
      if (targets.length > 0) {
        select.innerHTML = targets.map((target) => `<option value="${escapeHtml(target)}" data-disk-name="${escapeHtml(target)}">${escapeHtml(target)}</option>`).join("");
        updateSide("target", targets[0]);
        writeWorkflowLog(t("scan.diskDoneLog"));
        showToast(t("scan.diskDone"));
        return;
      }
    } catch (error) {
      showToast(t("scan.diskFailedShort"), "error");
      writeWorkflowLog(t("scan.diskFailed", { message: error?.message || error }));
      return;
    }
  }

  select.innerHTML = `<option value="" disabled selected>${t("scan.waiting")}</option>`;
  updateSide("target", t("targetNotSelected"));
  writeWorkflowLog(t("scan.appModeRequired"));
  showToast(t("scan.completed"));
}

async function installAvml(button) {
  const workflow = currentWorkflow();
  if (!workflow || workflow.platform !== "Linux" || workflow.mode !== "local-ram") {
    showToast(t("workflow.avmlUnsupported"), "error");
    return;
  }
  if (!backendReady()) {
    showToast(t("workflow.appModeRequired"), "error");
    return;
  }

  button.disabled = true;
  writeWorkflowLog(t("workflow.avmlInstalling"));
  updateSide("last-action", t("workflow.avmlInstalling"));
  try {
    const result = await apiRequest("/api/avml-install", { method: "POST" });
    const status = result.status || {};
    const path = status.tool_path || result.path || "/usr/bin/avml";
    const label = status.message || result.message || "AVML ready";
    updateSide("target", escapeHtml(path));
    writeWorkflowLog(t("scan.toolDoneLog", { target: "AVML", message: escapeHtml(label) }));
    writeWorkflowLog(t("workflow.avmlInstalled", { path: escapeHtml(path) }));
    showToast(t("workflow.avmlInstalled", { path }));
  } catch (error) {
    writeWorkflowLog(t("workflow.avmlInstallFailed", { message: escapeHtml(error.message) }));
    showToast(t("workflow.avmlInstallFailed", { message: error.message }), "error");
  } finally {
    button.disabled = false;
  }
}

async function installWinpmem(button) {
  const workflow = currentWorkflow();
  if (!workflow || workflow.platform !== "Windows" || workflow.mode !== "local-ram") {
    showToast(t("workflow.winpmemUnsupported"), "error");
    return;
  }
  if (!backendReady()) {
    showToast(t("workflow.appModeRequired"), "error");
    return;
  }

  button.disabled = true;
  writeWorkflowLog(t("workflow.winpmemInstalling"));
  updateSide("last-action", t("workflow.winpmemInstalling"));
  setProgress(0, "0%");
  try {
    const start = await apiRequest("/api/winpmem-install", { method: "POST" });
    if (!start.job_id) throw new Error(t("workflow.jobIdMissing"));

    // Wait for the download/install job to finish
    const result = await waitForAcquisitionJob(start.job_id);

    const status = result.status || {};
    const path = status.tool_path || result.path || "C:\\Tools\\go-winpmem_amd64_1.0-rc2_signed.exe";
    const label = status.message || result.message || "WinPMEM ready";
    updateSide("target", escapeHtml(path));
    writeWorkflowLog(t("scan.toolDoneLog", { target: "WinPMEM", message: escapeHtml(label) }));
    writeWorkflowLog(t("workflow.winpmemInstalled", { path: escapeHtml(path) }));
    showToast(t("workflow.winpmemInstalled", { path }));
  } catch (error) {
    setProgress(0);
    writeWorkflowLog(t("workflow.winpmemInstallFailed", { message: escapeHtml(error.message) }));
    showToast(t("workflow.winpmemInstallFailed", { message: error.message }), "error");
  } finally {
    button.disabled = false;
  }
}

function setProgress(value, labelText = `${value}%`) {
  const progress = document.querySelector("[data-progress]");
  if (!progress) return;
  setProgressElement(progress, value, labelText);
}

function setProgressElement(progress, value, labelText = `${value}%`) {
  const numericValue = Math.max(0, Math.min(100, Number(value) || 0));
  const next = `${numericValue}%`;
  progress.style.setProperty("--value", next);
  progress.classList.toggle("is-past-half", numericValue >= 50);
  const label = progress.querySelector("b");
  if (label) label.textContent = labelText;
}

function acquisitionPercent(job) {
  const done = Number(job?.done || 0);
  const total = Number(job?.total || 0);
  if (!Number.isFinite(done) || !Number.isFinite(total) || total <= 0) return 0;
  return Math.max(0, Math.min(100, Math.floor((done * 100) / total)));
}

async function waitForAcquisitionJob(jobId, options = {}) {
  while (true) {
    const job = await apiRequest("/api/acquisition-status", {
      method: "POST",
      body: JSON.stringify({ job_id: jobId })
    });
    if (typeof options.onUpdate === "function") options.onUpdate(job);
    const percent = acquisitionPercent(job);
    setProgress(percent, `${percent}%`);
    if (job.message) updateSide("last-action", job.message);

    if (job.status === "completed") {
      setProgress(100, "100%");
      return job.result || {};
    }
    if (job.status === "failed") {
      throw new Error(job.error || job.message || t("acquisitionFailed"));
    }

    await new Promise((resolve) => window.setTimeout(resolve, 500));
  }
}

function ramConsoleHtml(title, logs = []) {
  const entries = Array.isArray(logs) && logs.length
    ? logs
    : ["Analiz başlatıldı. Volatility3 çıktısı bekleniyor..."];
  return `
    <div class="log-box ram-analysis-console">
      <strong>${escapeHtml(title)}</strong>
      <pre>${entries.map((line) => escapeHtml(line)).join("\n")}</pre>
    </div>
  `;
}

function updateRamConsole(selector, job, title = "Canlı Analiz Konsolu") {
  const container = document.querySelector(selector);
  if (!container) return;
  const logs = Array.isArray(job.logs) ? job.logs : [];
  container.innerHTML = ramConsoleHtml(title, logs);
  container.scrollTop = container.scrollHeight;
}

function ramSymbolDirValue() {
  const value = document.querySelector("#ram-symbol-dir")?.value.trim() || "";
  if (!value || value.startsWith(".")) return null;
  state.ramSymbolDirInput = value;
  return value;
}

function renderVolatilityPreflight(result) {
  const warnings = Array.isArray(result.warnings) ? result.warnings : [];
  const recommendations = Array.isArray(result.recommendations) ? result.recommendations : [];
  const banners = Array.isArray(result.banners) ? result.banners : [];
  const symbolDirs = Array.isArray(result.symbol_dirs) ? result.symbol_dirs : [];
  const matches = Array.isArray(result.matching_symbols) ? result.matching_symbols : [];
  const badge = result.ready
    ? `<span class="status-pill ok">${escapeHtml(t("analysis.preflightReady"))}</span>`
    : `<span class="status-pill danger">${escapeHtml(t("analysis.preflightMissing"))}</span>`;
  return `
    <div class="analysis-summary">
      <p class="section-label">${escapeHtml(t("analysis.preflightTitle"))}</p>
      <div class="summary-grid">
        <div><strong>${escapeHtml(t("analysis.preflightStatus"))}</strong><span>${badge}</span></div>
        <div><strong>vol.py</strong><span>${escapeHtml(result.vol_py || t("analysis.preflightNotFound"))}</span></div>
        <div><strong>${escapeHtml(t("analysis.preflightSymbols"))}</strong><span>${Number(result.symbol_count || 0)} ${escapeHtml(t("analysis.preflightTotal"))} · ${Number(result.linux_symbol_count || 0)} Linux</span></div>
        <div><strong>${escapeHtml(t("analysis.preflightMatch"))}</strong><span>${matches.length ? `${matches.length} symbol` : escapeHtml(t("analysis.preflightNoMatch"))}</span></div>
      </div>
      <div class="section-divider"></div>
      <div class="log-box">
        <strong>${escapeHtml(t("analysis.preflightBanners"))}</strong>
        <pre>${banners.length ? banners.map(escapeHtml).join("\n") : escapeHtml(t("analysis.preflightNoBanner"))}</pre>
      </div>
      <div class="log-box">
        <strong>${escapeHtml(t("analysis.preflightSymbolDirs"))}</strong>
        <pre>${symbolDirs.length ? symbolDirs.map(escapeHtml).join("\n") : escapeHtml(t("analysis.preflightNoSymbolDir"))}</pre>
      </div>
      ${warnings.length ? `<div class="log-box" style="color:#ffb4b4"><strong>${escapeHtml(t("analysis.preflightWarnings"))}</strong><pre>${warnings.map(escapeHtml).join("\n")}</pre></div>` : ""}
      ${recommendations.length ? `<div class="log-box"><strong>${escapeHtml(t("analysis.preflightRecommendations"))}</strong><pre>${recommendations.map(escapeHtml).join("\n")}</pre></div>` : ""}
    </div>
  `;
}

function renderVolatilitySymbolInstall(result) {
  const banners = Array.isArray(result.banners) ? result.banners : [];
  const matches = Array.isArray(result.matches) ? result.matches : [];
  const recommendations = Array.isArray(result.recommendations) ? result.recommendations : [];
  const preflight = result.preflight || null;
  const ready = Boolean(preflight?.ready || result.status === "windows-automatic");
  const badge = ready
    ? `<span class="status-pill ok">${escapeHtml(t("analysis.preflightReady"))}</span>`
    : `<span class="status-pill danger">${escapeHtml(t("analysis.preflightMissing"))}</span>`;
  const matchLines = matches.map((item) => {
    const remotePath = item.remote_path || item.url || "";
    return `${item.banner || "-"}\n  -> ${remotePath}`;
  });
  return `
    <div class="analysis-summary">
      <p class="section-label">${escapeHtml(t("analysis.symbolInstallTitle"))}</p>
      <div class="summary-grid">
        <div><strong>${escapeHtml(t("analysis.preflightStatus"))}</strong><span>${badge}</span></div>
        <div><strong>${escapeHtml(t("analysis.symbolDir"))}</strong><span>${escapeHtml(result.symbol_dir || "-")}</span></div>
        <div><strong>${escapeHtml(t("analysis.symbolInstallTarget"))}</strong><span>${escapeHtml(result.target || "-")}</span></div>
        <div><strong>SHA256</strong><span>${escapeHtml(result.sha256 || "-")}</span></div>
      </div>
      <div class="section-divider"></div>
      <div class="log-box">
        <strong>${escapeHtml(t("analysis.symbolInstallMessage"))}</strong>
        <pre>${escapeHtml(result.message || "-")}</pre>
      </div>
      <div class="log-box">
        <strong>${escapeHtml(t("analysis.preflightBanners"))}</strong>
        <pre>${banners.length ? banners.map(escapeHtml).join("\n") : escapeHtml(t("analysis.preflightNoBanner"))}</pre>
      </div>
      <div class="log-box">
        <strong>${escapeHtml(t("analysis.symbolInstallMatches"))}</strong>
        <pre>${matchLines.length ? matchLines.map(escapeHtml).join("\n") : escapeHtml(t("analysis.preflightNoMatch"))}</pre>
      </div>
      ${preflight ? renderVolatilityPreflight(preflight) : ""}
      ${recommendations.length ? `<div class="log-box"><strong>${escapeHtml(t("analysis.preflightRecommendations"))}</strong><pre>${recommendations.map(escapeHtml).join("\n")}</pre></div>` : ""}
    </div>
  `;
}

async function sendAcquisitionControl(action) {
  const active = state.activeAcquisition;
  if (!active || !active.jobId || !active.workflowId) {
    showToast(t("workflow.activeJobMissing"), "error");
    return;
  }
  const workflow = workflows[active.workflowId];
  const body = {
    job_id: active.jobId,
    action
  };
  if (workflow?.mode.startsWith("remote")) {
    Object.assign(body, active.payload || {});
  }

  await apiRequest("/api/acquisition-control", {
    method: "POST",
    body: JSON.stringify(body)
  });
  const label = action === "stop" ? t("workflow.stopLabel") : action === "pause" ? t("workflow.pauseLabel") : t("workflow.resumeLabel");
  const message = workflow?.mode.startsWith("remote")
    ? t("workflow.controlSent", { label })
    : t("workflow.controlApplied", { label });
  writeWorkflowLog(message);
  updateSide("last-action", message);
  showToast(message);
}

async function startAcquisition(button) {
  const routeId = state.route.split(":")[1];
  const workflow = workflows[routeId];
  const isRam = workflow?.mode.includes("ram");
  let payload = null;
  if (workflow?.mode.startsWith("remote")) {
    try {
      payload = connectionPayload();
    } catch (error) {
      showToast(error.message, "error");
      return;
    }
    if (!requireActiveConnection(workflow, payload)) return;
  }
  const target = document.querySelector("[data-field='target']")?.value.trim();
  const outputFormat = document.querySelector("[data-field='output-format']")?.value || "raw";
  if (workflow && !workflow.mode.includes("ram") && !target) {
    showToast(t("workflow.diskRequired"), "error");
    return;
  }
  let output = document.querySelector("#workflow-output")?.value.trim() || "";
  const diskName = isRam ? "" : selectedTargetName();
  let caseName = null;
  button.disabled = true;
  window.clearInterval(state.jobs.workflow);
  setProgress(0, "0%");
  const operation = isRam ? t("ramAcquisition") : t("imageAcquisition");

  try {
    setAcquisitionControlsVisible(true, button);
    await loadEvidenceCases();
    const evidenceCase = await ensureImageCase();
    caseName = evidenceCase.case_name;
    if (isRam) {
      const remoteIp = workflow?.mode.startsWith("remote") ? payload?.ip : "";
      const fileName = canonicalRamFileName(remoteIp);
      const outputInput = document.querySelector("#workflow-output");
      if (outputInput) outputInput.value = fileName;
      const ramDir = evidenceCase.ram_dir || `${evidenceCase.case_dir}/ram`;
      output = `${ramDir}/${fileName}`;
    } else {
      output = evidenceCase.output_dir || `${evidenceCase.case_dir}/ciktilar`;
    }
    document.querySelectorAll("[data-case-output]").forEach((outputNode) => {
      outputNode.textContent = outputNode.dataset.caseOutputSubdir === "ram"
        ? (evidenceCase.ram_dir || `${evidenceCase.case_dir}/ram`)
        : (evidenceCase.output_dir || `${evidenceCase.case_dir}/ciktilar`);
    });

    writeWorkflowLog(t("workflow.operationStarted", { operation }));
    updateSide("last-action", t("workflow.operationRunning", { operation }));
    if (workflow?.mode.startsWith("remote")) updateSide("connection", t("workflow.operationRunning", { operation }));

    const start = workflow?.mode.startsWith("remote")
      ? await apiRequest(isRam ? "/api/remote-ram" : "/api/remote-image", {
          method: "POST",
          body: JSON.stringify(isRam
            ? {
                ...payload,
                output,
                case_name: caseName,
                output_format: outputFormat
              }
            : {
                ...payload,
                disk_id: target,
                disk_name: diskName,
                output,
                case_name: caseName,
                output_format: outputFormat
              })
        })
      : await apiRequest(isRam ? "/api/local-ram" : "/api/local-image", {
          method: "POST",
          body: JSON.stringify(isRam
            ? {
                output,
                tool: workflow.platform === "Windows" ? "winpmem" : "avml",
                tool_path: target,
                case_name: caseName,
                output_format: outputFormat
            }
            : {
                source: target,
                disk_name: diskName,
                output,
                case_name: caseName,
                output_format: outputFormat
              })
        });
    if (!start.job_id) throw new Error(t("workflow.jobIdMissing"));
    state.activeAcquisition = {
      jobId: start.job_id,
      workflowId: routeId,
      payload
    };
    const result = await waitForAcquisitionJob(start.job_id);

    setProgress(100);
    const targetPath = result.target_path || result.target || output;
    writeWorkflowLog(t("workflow.operationCompletedPath", { operation, path: targetPath }));
    if (result.sha256) {
      writeWorkflowLog(t("workflow.hashWritten", { hash: escapeHtml(result.sha256) }));
    }
    if (result.output_format) {
      writeWorkflowLog(t("workflow.formatCompleted", { format: String(result.output_format).toUpperCase() }));
    }
    updateSide("last-action", t("workflow.operationCompleted", { operation }));
    if (workflow?.mode.startsWith("remote") && payload) {
      updateSide("connection", t("connection.connected", { ip: payload.ip }));
    }
    showToast(t("workflow.operationCompleted", { operation }));
  } catch (error) {
    setProgress(0);
    writeWorkflowLog(t("workflow.operationFailedDetail", { operation, message: error.message }));
    updateSide("last-action", t("workflow.operationFailed", { operation }));
    if (workflow?.mode.startsWith("remote")) {
      updateSide("connection", t("workflow.operationFailed", { operation }));
    }
    showToast(t("workflow.operationFailedDetail", { operation, message: error.message }), "error");
  } finally {
    state.activeAcquisition = null;
    setAcquisitionControlsVisible(false, button);
  }
}

function setAnalysisStatus(status, log) {
  const statusNode = document.querySelector("[data-analysis-status]");
  const logNode = document.querySelector("[data-analysis-log]");
  if (statusNode) statusNode.textContent = status;
  state.imageMountLogHTML = log || "";
  if (logNode) {
    logNode.innerHTML = log || "";
    logNode.style.display = log ? "" : "none";
  }
}

function renderMountResultInfo(result) {
  const mode = result.mount_mode || "mounted";
  const analysis = result.analysis || {};
  const filesystems = Array.isArray(analysis.filesystems) ? analysis.filesystems : [];
  const partitions = Array.isArray(analysis.partitions) ? analysis.partitions : [];
  const status = mode === "analysis-only"
    ? t("analysis.analysisOnlyLog")
    : t("analysis.mountedLog");
  const details = [
    analysis.image_type ? `${t("analysis.imageType")}: ${analysis.image_type}` : "",
    analysis.size ? `${t("analysis.imageSize")}: ${formatBytes(analysis.size)}` : "",
    analysis.partition_scheme ? `${t("analysis.partitionScheme")}: ${analysis.partition_scheme}` : "",
    `${t("analysis.partitionCount")}: ${partitions.length}`,
    `${t("analysis.filesystemCount")}: ${filesystems.length}`
  ].filter(Boolean);
  return `
    <div class="mount-info-panel">
      <strong>${escapeHtml(status)}</strong>
      ${result.mount_error ? `<pre>${escapeHtml(result.mount_error)}</pre>` : ""}
      ${details.length ? `<div class="mount-info-grid">${details.map((item) => `<span>${escapeHtml(item)}</span>`).join("")}</div>` : ""}
    </div>
  `;
}

function renderDiskAnalysisSummary(result) {
  const partitions = Array.isArray(result.partitions) ? result.partitions : [];
  const filesystems = Array.isArray(result.filesystems) ? result.filesystems : [];
  const mounted = result.mounted || null;
  return `
    <p class="section-label">${t("analysis.diskSummary")}</p>
    <div class="hash-grid">
      ${analysisMetric("İmaj", escapeHtml(result.image_type || "-"))}
      ${analysisMetric("Boyut", formatBytes(result.size || 0))}
      ${analysisMetric("Bölüm", escapeHtml(result.partition_scheme || "-"))}
      ${analysisMetric("FS", String(filesystems.length))}
    </div>
    ${analysisList("Bölümler", partitions.map((part) => `${part.index}. ${part.scheme} ${part.type_name} LBA ${part.start_lba} · ${formatBytes(part.size || 0)}`))}
    ${analysisList("Dosya sistemi imzaları", filesystems.map((fs) => `${fs.source}: ${fs.fs_type} @ ${fs.offset}`))}
    ${mounted ? `
      <div class="section-divider"></div>
      <div class="hash-grid">
        ${analysisMetric("Dosya", String(mounted.file_count || 0))}
        ${analysisMetric("Klasör", String(mounted.directory_count || 0))}
        ${analysisMetric("Görünen veri", formatBytes(mounted.total_visible_bytes || 0))}
        ${analysisMetric("Taranan", String(mounted.scanned_entries || 0))}
      </div>
      ${analysisList("Uzantılar", (mounted.top_extensions || []).map((item) => `${item.extension}: ${item.count}`))}
      ${analysisList("En büyük dosyalar", (mounted.largest_files || []).map((item) => `${item.path} · ${formatBytes(item.size || 0)}`))}
    ` : ""}
    ${analysisList("Uyarılar", result.warnings || [], "warning")}
    ${analysisList("Öneriler", result.recommendations || [])}
  `;
}

function renderRamAnalysisSummary(result) {
  return `
    <div class="hash-grid">
      ${analysisMetric("Tip", escapeHtml(result.dump_type || "-"))}
      ${analysisMetric("Boyut", formatBytes(result.size || 0))}
      ${analysisMetric("Entropi", Number(result.entropy_sample || 0).toFixed(2))}
      ${analysisMetric("IOC", String(result.string_match_count || 0))}
    </div>
    ${analysisList("Kategori sayımları", (result.category_counts || []).map((item) => `${item.category}: ${item.count}`))}
    ${analysisList("Proses özeti", (result.largest_processes || []).map((proc) => `${proc.pid} ${proc.name} · ${formatBytes(proc.dump_size || 0)}`))}
    ${analysisList("Örnek bulgular", (result.sample_matches || []).slice(0, 20).map((item) => `${item.category} @ 0x${Number(item.offset || 0).toString(16).toUpperCase()}: ${item.value}`))}
    ${analysisList("Uyarılar", result.warnings || [], "warning")}
    ${analysisList("Öneriler", result.recommendations || [])}
  `;
}

function renderAndroidAnalysisSummary(result) {
  const profile = result.device_profile || {};
  const profileBits = [
    profile.manufacturer,
    profile.model,
    profile.android_release ? `Android ${profile.android_release}` : "",
    profile.security_patch ? `Patch ${profile.security_patch}` : ""
  ].filter(Boolean).join(" · ");
  return `
    <p class="section-label">${t("analysis.androidSummary")}</p>
    <div class="hash-grid">
      ${analysisMetric("Vaka", escapeHtml(result.case_name || "-"))}
      ${analysisMetric("Kayıt", String(result.record_count || 0))}
      ${analysisMetric("Timeline", String(result.timeline_event_count || 0))}
      ${analysisMetric("Korelasyon", String(result.correlation_count || 0))}
    </div>
    ${profileBits ? `<div class="side-info"><span class="metric-icon">${icon("android")}</span><span><strong>Cihaz</strong><small>${escapeHtml(profileBits)}</small></span></div>` : ""}
    ${analysisList("Kayıt türleri", (result.record_types || []).map((item) => `${item.record_type}: ${item.count}`))}
    ${analysisList("Önemli timeline olayları", (result.recent_events || []).slice(0, 15).map((event) => `${event.type || "event"} [${event.severity || 0}] ${event.summary || ""}`))}
    ${analysisList("Uçucu veri bölümleri", result.volatile_sections || [])}
    ${analysisList("Dosyalar", (result.files || []).slice(0, 20).map((file) => `${file.name} · ${formatBytes(file.size || 0)}`))}
    ${result.report_preview ? `<div class="section-divider"></div><pre class="log-box" style="white-space:pre-wrap;max-height:260px">${escapeHtml(result.report_preview)}</pre>` : ""}
    ${analysisList("Uyarılar", result.warnings || [], "warning")}
    ${analysisList("Öneriler", result.recommendations || [])}
  `;
}

function analysisMetric(label, value) {
  return `
    <div class="hash-result">
      <small>${escapeHtml(label)}</small>
      <strong>${value}</strong>
    </div>
  `;
}

function analysisList(title, items, tone = "") {
  const safeItems = Array.isArray(items) ? items.filter((item) => String(item || "").trim()) : [];
  if (!safeItems.length) return "";
  const color = tone === "warning" ? " style=\"color:#ffb86b\"" : "";
  return `
    <div class="section-divider"></div>
    <p class="section-label"${color}>${escapeHtml(title)}</p>
    <div class="strings-results-list">
      ${safeItems.map((item) => `<div class="string-match-item"><div class="match-value">${escapeHtml(String(item))}</div></div>`).join("")}
    </div>
  `;
}

function renderTree(node, depth = 0) {
  if (!node) return `<div class="log-box">${escapeHtml(t("analysis.outputWaiting"))}</div>`;
  const isVirtual = Boolean(node.virtual);
  const hasChildren = Array.isArray(node.children) && node.children.length > 0;
  const expanded = isVirtual && depth === 0;
  const fileIcon = node.is_dir ? "📁" : "📄";
  const toggle = node.is_dir ? `<span class="toggle-icon">${expanded ? "▾" : "▸"}</span>` : "";
  const sizeStr = node.is_dir ? "" : `<span class="node-size">${formatBytes(node.size)}</span>`;
  const note = node.note || node.name || "";
  
  let relativePath = node.path;
  if (!isVirtual && state.imageMount && state.imageMount.mountDir) {
    if (node.path.startsWith(state.imageMount.mountDir)) {
      relativePath = node.path.substring(state.imageMount.mountDir.length);
    }
  }

  const current = `
    <div class="tree-node" data-path="${escapeHtml(relativePath)}" data-is-dir="${node.is_dir}" data-virtual="${isVirtual}" data-has-children="${hasChildren}" data-note="${escapeHtml(note)}">
      <span style="width:16px;display:inline-block">${toggle}</span>
      <span class="node-icon">${fileIcon}</span>
      <span class="node-name">${escapeHtml(node.name || node.path.split('/').pop() || "/")}</span>
      ${sizeStr}
    </div>
    <div class="tree-children-container"></div>
  `;

  const children = hasChildren
    ? `<div class="tree-children" style="padding-left:14px; display:${expanded ? "block" : "none"}">${node.children.map(child => renderTree(child, depth + 1)).join("")}</div>`
    : "";

  return current + children;
}

async function calculateHashes() {
  const inputPath = document.querySelector("#hash-file")?.value.trim();
  if (backendReady() && inputPath) {
    try {
      const hashes = await apiRequest("/api/hash", {
        method: "POST",
        body: JSON.stringify({
          path: inputPath,
          algorithms: ["md5", "sha1", "sha256", "sha512"]
        })
      });
      setHashResult("md5", hashes.md5 || "-");
      setHashResult("sha1", hashes.sha1 || "-");
      setHashResult("sha256", hashes.sha256 || "-");
      setHashResult("sha512", hashes.sha512 || "-");
      showToast(t("hash.done"));
      return;
    } catch (error) {
      showToast(t("hash.failed", { message: error.message }), "error");
      return;
    }
  }

  const file = state.files["#hash-file"];
  if (!file) {
    showToast(t("fileRequired"), "error");
    return;
  }
  const buffer = await file.arrayBuffer();
  setHashResult("md5", t("hash.fullAppRequired"));
  setHashResult("sha1", await digestHex("SHA-1", buffer));
  setHashResult("sha256", await digestHex("SHA-256", buffer));
  setHashResult("sha512", await digestHex("SHA-512", buffer));
  showToast(t("hash.done"));
}

async function digestHex(algorithm, buffer) {
  const hash = await crypto.subtle.digest(algorithm, buffer.slice(0));
  return [...new Uint8Array(hash)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

function setHashResult(key, value) {
  const node = document.querySelector(`[data-hash-result="${key}"] strong`);
  if (node) node.textContent = value;
}

function compareHash() {
  const expected = document.querySelector("[data-hash-expected]")?.value.trim().toLowerCase();
  const values = [...document.querySelectorAll("[data-hash-result] strong")].map((node) => node.textContent.trim().toLowerCase());
  const result = document.querySelector("[data-hash-compare-result] small");
  if (!expected) {
    showToast(t("hash.compareRequired"), "error");
    return;
  }
  const matched = values.includes(expected);
  if (result) result.textContent = matched ? t("hash.matched") : t("hash.notMatched");
  showToast(matched ? t("hash.matchedToast") : t("hash.notMatchedToast"), matched ? "success" : "error");
}

function setStatus(selector, html) {
  const node = document.querySelector(selector);
  if (node) node.innerHTML = html;
}

async function createEvidenceCase() {
  const caseName = document.querySelector("#case-name")?.value.trim();
  if (!caseName) {
    showToast(t("case.required"), "error");
    return;
  }
  try {
    const result = await apiRequest("/api/evidence-create", {
      method: "POST",
      body: JSON.stringify({ case_name: caseName })
    });
    state.activeCase = result;
    state.pendingCaseName = "";
    await loadEvidenceCases();
    setStatus("[data-case-status]", `${icon("info")} ${t("case.created", { path: escapeHtml(result.case_dir) })}`);
    showToast(t("case.created", { path: result.case_dir }));
  } catch (error) {
    setStatus("[data-case-status]", `${icon("info")} ${t("case.createFailed", { message: escapeHtml(error.message) })}`);
    showToast(t("case.createFailed", { message: error.message }), "error");
  }
}

async function listEvidenceFiles() {
  if (!state.activeCase) {
    showToast(t("case.required"), "error");
    return;
  }
  const subdir = document.querySelector("#case-folder")?.value || "ciktilar";
  try {
    const result = await apiRequest("/api/evidence-list-files", {
      method: "POST",
      body: JSON.stringify({ subdir })
    });
    const files = result.files || [];
    const select = document.querySelector("#case-file-list");
    if (select) {
      select.innerHTML = files.length
        ? files.map((file) => `<option value="${escapeHtml(file.path)}">${escapeHtml(file.name)} · ${formatBytes(file.size)}</option>`).join("")
        : `<option>${t("case.empty")}</option>`;
    }
    setStatus("[data-case-status]", `${icon("info")} ${t("case.filesListed", { count: String(files.length) })}`);
    showToast(t("case.filesListed", { count: String(files.length) }));
  } catch (error) {
    showToast(t("case.listFailed", { message: error.message }), "error");
  }
}

async function createCaseManifest() {
  const caseName = state.pendingCaseName || state.activeCase?.case_name || "";
  if (!caseName) {
    showToast(t("case.required"), "error");
    return;
  }
  try {
    const result = await apiRequest("/api/evidence-manifest", {
      method: "POST",
      body: JSON.stringify({ case_name: caseName })
    });
    if (state.activeCase) {
      state.activeCase.manifest_path = result.path || state.activeCase.manifest_path;
    }
    await loadEvidenceCases();
    setStatus("[data-manifest-status]", `${icon("shield")} ${t("case.manifest.ready", { path: escapeHtml(result.path || "") })}`);
    showToast(t("case.manifest.created", { path: result.path || "" }));
  } catch (error) {
    setStatus("[data-manifest-status]", `${icon("info")} ${t("case.manifest.failed", { message: escapeHtml(error.message) })}`);
    showToast(t("case.manifest.failed", { message: error.message }), "error");
  }
}

async function loadAcquisitionHistory({ silent = true } = {}) {
  if (!backendReady()) return;
  const caseName = resolveSelectedCaseName("#history-case", { fallbackToDefault: true });
  if (!caseName) {
    if (!silent) showToast(t("case.required"), "error");
    return;
  }
  try {
    const result = await apiRequest("/api/acquisition-history", {
      method: "POST",
      body: JSON.stringify({ case_name: caseName })
    });
    state.acquisitionHistory = Array.isArray(result.history) ? result.history : [];
    const detail = document.querySelector("#other-detail");
    if (detail && state.activeTab === "history") {
      detail.innerHTML = boundDetailPanel(state.activeTab);
      hydrateIcons(detail);
    }
    if (!silent) showToast(t("acquisition.history.loaded", { count: String(state.acquisitionHistory.length) }));
  } catch (error) {
    if (!silent) showToast(t("acquisition.history.failed", { message: error.message }), "error");
  }
}

async function addEvidenceNote() {
  const note = document.querySelector("#report-note")?.value.trim();
  if (!note) {
    showToast(t("report.noteRequired"), "error");
    return;
  }
  const caseName = reportCaseName();
  try {
    const result = await apiRequest("/api/evidence-add-note", {
      method: "POST",
      body: JSON.stringify({ note, case_name: caseName })
    });
    await loadEvidenceCases();
    setStatus("[data-report-status]", `${icon("info")} ${t("report.noteAdded", { path: escapeHtml(result.path) })}`);
    showToast(t("report.noteAdded", { path: result.path }));
  } catch (error) {
    setStatus("[data-report-status]", `${icon("info")} ${t("report.noteFailed", { message: escapeHtml(error.message) })}`);
    showToast(t("report.noteFailed", { message: error.message }), "error");
  }
}

async function createEvidenceReport() {
  const caseName = reportCaseName();
  const title = document.querySelector("#report-title")?.value.trim() || t("report.defaultTitle");
  const format = document.querySelector("#report-format")?.value || "txt";
  const description = document.querySelector("#report-note")?.value.trim() || "";
  try {
    const result = await apiRequest("/api/report-create", {
      method: "POST",
      body: JSON.stringify({ case_name: caseName, title, description, format })
    });
    await loadEvidenceCases();
    setStatus("[data-report-status]", `${icon("info")} ${t("report.created", { path: escapeHtml(result.path) })}`);
    showToast(t("report.created", { path: result.path }));
  } catch (error) {
    setStatus("[data-report-status]", `${icon("info")} ${t("report.failed", { message: escapeHtml(error.message) })}`);
    showToast(t("report.failed", { message: error.message }), "error");
  }
}

async function downloadUpdatePackage() {
  const progress = document.querySelector("[data-update-progress]");
  const status = document.querySelector("[data-update-status]");
  const update = state.latestUpdate || await apiRequest("/api/update-check");
  state.latestUpdate = update;
  const asset = update.platform_asset || {};
  if (!asset.download_url) {
    const target = update.update_target || state.updateTarget || {};
    const message = update.asset_error || t("settings.noAssetForPackage", {
      package: target.asset_package_label || target.package_label || "-"
    });
    if (status) status.innerHTML = `${icon("info")} ${escapeHtml(message)}`;
    setStatus("[data-update-log]", escapeHtml(message));
    showToast(message, "error");
    return;
  }
  if (progress) {
    setProgressElement(progress, 35, "35%");
  }
  if (status) status.innerHTML = `${icon("download")} ${t("settings.downloading")}`;
  try {
    const result = await apiRequest("/api/update-download", {
      method: "POST",
      body: JSON.stringify({
        url: asset.download_url,
        name: asset.name,
        expected_sha256: asset.digest || ""
      })
    });
    if (progress) {
      setProgressElement(progress, 75, "75%");
    }
    if (status) status.innerHTML = `${icon("download")} ${t("settings.installing")}`;
    const install = await apiRequest("/api/update-install", {
      method: "POST",
      body: JSON.stringify({ path: result.path })
    });
    if (progress) {
      setProgressElement(progress, 100, "100%");
    }
    if (status) status.innerHTML = `${icon("shield")} ${t("settings.installStarted")}`;
    setStatus(
      "[data-update-log]",
      `${t("settings.downloaded", { path: escapeHtml(result.path) })}<br />${t("settings.sha256", { hash: escapeHtml(result.sha256) })}<br />${escapeHtml(install.message || t("settings.installStarted"))}`
    );
    showToast(t("settings.installStarted"));
  } catch (error) {
    if (progress) {
      setProgressElement(progress, 0, "0%");
    }
    const failedKey = String(error.message || "").toLowerCase().includes("installer")
      ? "settings.installFailed"
      : "settings.downloadFailed";
    if (status) status.innerHTML = `${icon("info")} ${t(failedKey, { message: escapeHtml(error.message) })}`;
    showToast(t(failedKey, { message: error.message }), "error");
  }
}

async function expandTreeNode(nodeElement, relativePath) {
  if (toggleExistingTreeChildren(nodeElement)) {
    return;
  }

  const tempContainer = document.createElement("div");
  tempContainer.className = "tree-children";
  tempContainer.style.paddingLeft = "14px";
  const placeholder = nodeElement.nextElementSibling?.classList.contains("tree-children-container")
    ? nodeElement.nextElementSibling
    : null;
  nodeElement.parentNode.insertBefore(tempContainer, placeholder ? placeholder.nextSibling : nodeElement.nextSibling);

  try {
    nodeElement.querySelector(".toggle-icon").innerHTML = "⌛";
    const result = await apiRequest("/api/image-browse", {
      method: "POST",
      body: JSON.stringify({ path: relativePath })
    });
    
    let html = "";
    if (result.files && result.files.length > 0) {
      result.files.sort((a, b) => b.is_dir - a.is_dir || a.name.localeCompare(b.name));
      result.files.forEach(file => {
        const fileIcon = file.is_dir ? "📁" : "📄";
        const toggle = file.is_dir ? `<span class="toggle-icon">▸</span>` : "";
        const sizeStr = file.is_dir ? "" : `<span class="node-size">${formatBytes(file.size)}</span>`;
        html += `
          <div class="tree-node" data-path="${escapeHtml(file.relative_path)}" data-is-dir="${file.is_dir}">
            <span style="width:16px;display:inline-block">${toggle}</span>
            <span class="node-icon">${fileIcon}</span>
            <span class="node-name">${escapeHtml(file.name)}</span>
            ${sizeStr}
          </div>
          <div class="tree-children-container"></div>
        `;
      });
    } else {
      html += `<div class="tree-node" style="opacity:0.5;padding-left:20px">Boş Klasör / Empty Directory</div>`;
    }
    
    nodeElement.querySelector(".toggle-icon").innerHTML = "▾";
    tempContainer.innerHTML = html;
  } catch (error) {
    nodeElement.querySelector(".toggle-icon").innerHTML = "▸";
    tempContainer.remove();
    showToast("Klasör açma başarısız: " + error.message, "error");
  }
}

function findExistingTreeChildren(nodeElement) {
  let sibling = nodeElement.nextElementSibling;
  if (sibling?.classList.contains("tree-children-container")) {
    sibling = sibling.nextElementSibling;
  }
  return sibling?.classList.contains("tree-children") ? sibling : null;
}

function toggleExistingTreeChildren(nodeElement) {
  const childrenContainer = findExistingTreeChildren(nodeElement);
  if (!childrenContainer) return false;
  const toggleIcon = nodeElement.querySelector(".toggle-icon");
  if (childrenContainer.style.display === "none") {
    childrenContainer.style.display = "block";
    if (toggleIcon) toggleIcon.innerHTML = "▾";
  } else {
    childrenContainer.style.display = "none";
    if (toggleIcon) toggleIcon.innerHTML = "▸";
  }
  return true;
}

function showVirtualTreeInfo(nodeElement) {
  const container = document.querySelector("#image-file-preview");
  if (!container) return;
  const title = nodeElement.querySelector(".node-name")?.textContent?.trim() || t("analysis.virtualInfo");
  const note = nodeElement.dataset.note || title;
  container.innerHTML = `
    <div class="log-box" style="padding:20px;white-space:pre-wrap">
      <strong>${escapeHtml(title)}</strong>
      <div class="section-divider"></div>
      ${escapeHtml(note)}
    </div>
  `;
}

async function previewImageFile(relativePath) {
  const container = document.querySelector("#image-file-preview");
  if (!container) return;
  container.innerHTML = `<div class="log-box" style="display:flex;align-items:center;justify-content:center;color:var(--muted);height:200px">⌛ Yükleniyor / Loading...</div>`;
  
  try {
    const result = await apiRequest("/api/image-read-file", {
      method: "POST",
      body: JSON.stringify({ path: relativePath })
    });
    
    let contentHtml = "";
    if (result.type === "image") {
      contentHtml = `
        <div class="image-viewer-area" style="height:320px">
          <img src="${result.content}" alt="preview" style="max-height:300px" />
        </div>
      `;
    } else if (result.type === "text") {
      contentHtml = `
        <textarea class="text-viewer-area" style="height:320px" readonly>${escapeHtml(result.content)}</textarea>
      `;
    } else {
      contentHtml = `
        <div class="hex-viewer-area" style="height:320px">${escapeHtml(result.content)}</div>
      `;
    }
    
    container.innerHTML = `
      <div style="display:flex;flex-direction:column;gap:12px;padding:14px">
        <div style="display:flex;justify-content:space-between;align-items:center;border-bottom:1px solid var(--line);padding-bottom:10px">
          <strong style="color:var(--text);font-size:14px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${escapeHtml(relativePath.split('/').pop())}</strong>
          <small class="meta" style="margin-top:0">${formatBytes(result.size)}</small>
        </div>
        <div style="flex:1;overflow:hidden">
          ${contentHtml}
        </div>
      </div>
    `;
  } catch (error) {
    container.innerHTML = renderErrorPanel(t("analysis.previewFailedTitle"), error);
  }
}

async function inspectProcessDetails(pid, name) {
  const rightContent = document.querySelector("#ram-right-content");
  if (!rightContent) return;
  
  const osProfile = document.querySelector("#ram-os-profile")?.value || "windows";
  rightContent.innerHTML = ramConsoleHtml("Canlı Proses Detay Konsolu", [
    "Volatility3 ile proses detayları yükleniyor...",
    osProfile === "windows" ? "DLL listesi çıkarılıyor." : "Açık dosyalar listeleniyor."
  ]);
  
  const ramPath = document.querySelector("#ram-analysis-path")?.value.trim();
  const symbolDir = ramSymbolDirValue();
  try {
    const start = await apiRequest("/api/ram-process-details-start", {
      method: "POST",
      body: JSON.stringify({ path: ramPath, pid, os_type: osProfile, symbol_dir: symbolDir })
    });
    if (!start.job_id) throw new Error(t("workflow.jobIdMissing"));
    const result = await waitForAcquisitionJob(start.job_id, {
      onUpdate(job) {
        updateRamConsole("#ram-right-content", job, "Canlı Proses Detay Konsolu");
      }
    });
    
    const label = osProfile === "windows" ? "Yüklenen DLL Modülleri (Loaded DLLs)" : 
                  "Açık Dosyalar (Open Files / lsof)";
    
    rightContent.innerHTML = `
      <div style="display:flex;flex-direction:column;gap:14px;padding:12px">
        <div style="display:flex;justify-content:space-between;align-items:center;border-bottom:1px solid var(--line);padding-bottom:8px">
          <strong style="color:var(--text)">${escapeHtml(name)} (${escapeHtml(pid)})</strong>
          <span style="font-size:12px;color:var(--muted)">Döküm: ${result.dumps?.length || 0} segment</span>
        </div>
        
        <p style="margin:0;font-size:12px;font-weight:bold;color:var(--muted)">${escapeHtml(label)}</p>
        <div class="maps-pre-box">${escapeHtml(result.maps || "Detay bulunamadı")}</div>
        
        <div class="section-divider" style="margin:8px 0"></div>
        
        <p style="margin:0;font-size:12px;font-weight:bold;color:var(--muted)">Proses Belleğinde Kelime/Dizgi Ara</p>
        <div class="input-action">
          <input type="text" id="proc-search-query" class="input" placeholder="Örn: whatsapp, telegram, token, password..." />
          <button class="primary-button" data-action="proc-search" data-pid="${escapeHtml(pid)}">Bellekte Ara</button>
        </div>
        
        <div id="proc-search-results" style="margin-top:10px"></div>
      </div>
    `;
    hydrateIcons(rightContent);
  } catch (error) {
    rightContent.innerHTML = renderErrorPanel(t("analysis.processDetailFailedTitle"), error);
  }
}

// Add global listener to inspect proc-search data action
document.addEventListener("click", async (event) => {
  const searchBtn = event.target.closest("[data-action='proc-search']");
  if (searchBtn) {
    const pid = searchBtn.dataset.pid;
    await runProcessMemorySearch(pid);
  }
});

async function runProcessMemorySearch(pid) {
  const query = document.querySelector("#proc-search-query")?.value.trim();
  const resultsDiv = document.querySelector("#proc-search-results");
  if (!query || !resultsDiv) {
    showToast("Arama sorgusu boş olamaz", "error");
    return;
  }
  
  const osProfile = document.querySelector("#ram-os-profile")?.value || "windows";
  resultsDiv.innerHTML = `<div class="log-box" style="text-align:center;padding:10px">⌛ Uçucu bellek taranıyor...</div>`;
  
  const ramPath = document.querySelector("#ram-analysis-path")?.value.trim();
  try {
    const result = await apiRequest("/api/ram-process-search", {
      method: "POST",
      body: JSON.stringify({ path: ramPath, pid, query, os_type: osProfile })
    });
    
    const count = result.length || 0;
    if (count === 0) {
      resultsDiv.innerHTML = `<div class="log-box" style="color:var(--muted);text-align:center;padding:10px">Hiçbir eşleşme bulunamadı.</div>`;
    } else {
      resultsDiv.innerHTML = `
        <div class="strings-results-list" style="max-height:220px">
          ${result.map(item => `
            <div class="string-match-item">
              <div class="match-meta">
                <span>Segment: <strong>${escapeHtml(item.category)}</strong></span>
                <span>Ofset: <strong>0x${item.offset.toString(16).toUpperCase()}</strong></span>
              </div>
              <div class="match-value" style="color:var(--text)">${escapeHtml(item.value)}</div>
              <div class="match-context">${escapeHtml(item.context)}</div>
            </div>
          `).join("")}
        </div>
      `;
    }
  } catch (error) {
    resultsDiv.innerHTML = renderErrorPanel(t("analysis.searchFailedTitle"), error);
  }
}

async function previewCarvedFile(filePath) {
  const rightContent = document.querySelector("#ram-right-content");
  if (!rightContent) return;
  
  rightContent.innerHTML = `<div class="log-box" style="text-align:center;padding:20px">⌛ Kurtarılan dosya yükleniyor... / Loading carved file...</div>`;
  
  try {
    const result = await apiRequest("/api/ram-read-carved", {
      method: "POST",
      body: JSON.stringify({ path: filePath })
    });
    
    let contentHtml = "";
    if (result.type === "image") {
      contentHtml = `
        <div class="image-viewer-area" style="height:320px">
          <img src="${result.content}" alt="carved preview" style="max-height:300px" />
        </div>
      `;
    } else if (result.type === "text") {
      contentHtml = `
        <textarea class="text-viewer-area" style="height:320px" readonly>${escapeHtml(result.content)}</textarea>
      `;
    } else {
      contentHtml = `
        <div class="hex-viewer-area" style="height:320px">${escapeHtml(result.content)}</div>
      `;
    }
    
    rightContent.innerHTML = `
      <div style="display:flex;flex-direction:column;gap:12px;padding:12px">
        <div style="display:flex;justify-content:space-between;align-items:center;border-bottom:1px solid var(--line);padding-bottom:10px">
          <strong style="color:var(--text);font-size:14px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${escapeHtml(filePath.split('/').pop())}</strong>
          <small class="meta" style="margin-top:0">${formatBytes(result.size)}</small>
        </div>
        <div style="flex:1;overflow:hidden">
          ${contentHtml}
        </div>
      </div>
    `;
  } catch (error) {
    rightContent.innerHTML = renderErrorPanel(t("analysis.previewFailedTitle"), error);
  }
}

async function bootApp() {
  setLanguage(state.language);
  setTheme(state.theme);
  syncSidebarState();
  installUiErrorHandlers();
  hydrateIcons();
  await loadProfiles();
  await loadUpdateTarget();
  if (state.activeProfile) {
    await loadPersistedSettings();
    await loadEvidenceCases();
  }
  render();
  if (state.profileGateVisible) renderProfileGate();

  // Developer mode — 5 kez logoya tıklayınca aktifleşir
  initDeveloperMode({ apiRequest, backendReady });
  devLog("INFO", "ui:startup", `Amele ${APP_VERSION} başlatıldı — platform: ${state.platform}, dil: ${state.language}, tema: ${state.theme}, backend: ${backendAvailable}`, apiRequest, backendReady);
}

bootApp().catch((error) => {
  console.error("Amele UI boot failed", error);
  showToast(`Arayüz başlatılamadı: ${error.message || error}`, "error");
});
