import { icon as defaultIcon } from "../icons.js";

export function workflowPage({ id, workflows, state, t, icon, localText, canonicalRamFileName, caseSelectOptions, caseOutputLabel, escapeHtml }) {
  const data = workflows[id] || workflows["windows-remote-disk"];
  const isSsh = data.mode.startsWith("ssh");
  const isRemote = data.mode.startsWith("remote") || isSsh;
  const isRam = data.mode.includes("ram");
  const toolCheck = data.platform === "Windows" ? "WinPMEM" : "AVML";
  const initialTargetLabel = t("scanDisksFirst");
  const outputField = isRam ? ramCasePanel({ t, icon, state, caseSelectOptions, caseOutputLabel, escapeHtml, canonicalRamFileName }) : imageCasePanel({ t, icon, state, caseSelectOptions, caseOutputLabel, escapeHtml });
  const targetSelect = isRam
    ? ""
    : field(t("workflow.disk"), `<select class="select" data-field="target"><option value="" disabled selected>${initialTargetLabel}</option></select>`);
  const scanLabel = isRam
    ? (isRemote ? t("workflow.checkTool", { tool: toolCheck }) : t("workflow.checkToolAction", { tool: toolCheck }))
    : (isRemote ? t("workflow.scanDisks") : t("workflow.scanLocalDisks"));

  const sshCliCmd = isRam
    ? `ssh <user>@<IP> -p <port> "sudo avml /dev/stdout" \\\n  | dd of=./remote_ram.lime`
    : `ssh <user>@<IP> -p <port> "sudo dd if=/dev/<disk> bs=4M status=progress" \\\n  | dd of=./remote_disk.img`;

  const connectionBlock = isSsh
    ? `
        <p class="section-label">${t("workflow.sshConnection")}</p>
        ${field(t("workflow.ip"), `<input class="input" data-field="ip" placeholder="192.168.1.100" value="" />`)}
        ${field(t("workflow.port"), `<input class="input" data-field="port" value="22" />`)}
        ${field(t("workflow.sshUser"), `<input class="input" data-field="ssh-user" placeholder="root" />`)}
        ${field(t("workflow.sshPass"), `<input class="input" type="password" data-field="ssh-pass" placeholder="${t("workflow.sshPassPlaceholder")}" />`)}
        ${pickerField(t("workflow.sshKey"), "ssh-key-file", t("workflow.sshKeyPlaceholder"), "file", icon, t)}
        <div class="button-row">
          <button class="primary-button" data-action="connect">${icon("key")} ${t("workflow.sshConnect")}</button>
        </div>
        <div class="section-divider"></div>
        <p class="section-label">${t("workflow.cliPreview")}</p>
        <div class="code-box ssh-cli-preview" id="ssh-cli-preview">${sshCliCmd}</div>
      `
    : `
        <p class="section-label">${t("workflow.connectionOps")}</p>
        ${field(t("workflow.ip"), `<input class="input" data-field="ip" placeholder="${t("workflow.ipPlaceholder")}" value="" />`)}
        ${field(t("workflow.port"), `<input class="input" data-field="port" value="4444" />`)}
        ${field(t("workflow.token"), `<input class="input" data-field="token" placeholder="${t("workflow.tokenPlaceholder")}" />`)}
        <div class="button-row">
          <button class="secondary-button" data-action="approve-key">${icon("key")} ${t("workflow.approveKey")}</button>
          <button class="secondary-button" data-action="reset-key">${icon("refresh")} ${t("workflow.reset")}</button>
          <button class="primary-button" data-action="connect">${icon("network")} ${t("workflow.connect")}</button>
        </div>
        <div class="toggle-row">
          <span>${t("workflow.useVpn")}</span>
          <button class="switch" data-action="toggle-vpn" aria-label="${t("workflow.useVpn")}"></button>
        </div>
        <div class="vpn-panel" hidden>
          <button class="secondary-button" data-action="vpn-config">${icon("settings")} ${t("workflow.configureVpn")}</button>
          ${field(t("workflow.server"), `<input class="input" data-field="vpn-endpoint" placeholder="10.0.0.1:51820" />`)}
          ${field(t("workflow.vpnPrivateKey"), `<input class="input" data-field="vpn-private-key" placeholder="YOUR_PRIVATE_KEY" />`)}
          ${field(t("workflow.vpnPublicKey"), `<input class="input" data-field="vpn-public-key" placeholder="SERVER_PUBLIC_KEY" />`)}
          ${field(t("workflow.allowedIps"), `<input class="input" data-field="vpn-allowed" value="0.0.0.0/0" />`)}
          ${field(t("workflow.vpnAddress"), `<input class="input" data-field="vpn-address" value="10.0.0.2/24" />`)}
          ${field(t("workflow.vpnDns"), `<input class="input" data-field="vpn-dns" value="1.1.1.1" />`)}
          ${field(t("workflow.vpnKeepalive"), `<input class="input" data-field="vpn-keepalive" value="25" />`)}
          ${pickerField(t("workflow.configFile"), "vpn-config-file", "wireguard.conf", "file", icon, t)}
          <div class="button-row">
            <button class="primary-button" data-action="save-vpn">${icon("shield")} ${t("workflow.saveVpn")}</button>
            <button class="secondary-button" data-action="start-vpn">${icon("play")} ${t("workflow.startVpn")}</button>
            <button class="danger-button" data-action="stop-vpn">${icon("stop")} ${t("workflow.stopVpn")}</button>
          </div>
        </div>
      `;

  return `
    <section class="page">
      <div class="workflow-layout">
        <div class="workflow-panel">
          ${pageTitle(localText(data.title), localText(data.desc), data.icon, icon)}
          <div class="form-grid">
            ${isRemote ? connectionBlock : ""}

            <div class="section-divider"></div>
            <p class="section-label">${t("workflow.caseSection")}</p>
            ${outputField}
            ${field(t("workflow.outputFormat"), `
              <select class="select" data-field="output-format">
                <option value="raw">${t("workflow.formatRaw")}</option>
                <option value="aff4">${t("workflow.formatAff4")}</option>
              </select>
            `)}

            <div class="section-divider"></div>
            <p class="section-label">${isRam ? t("workflow.ramOutput") : t("workflow.diskOutput")}</p>
            ${targetSelect}
            <div class="button-row workflow-target-actions">
              <button class="secondary-button" data-action="scan">${icon(isRam ? "chip" : "disk")} ${scanLabel}</button>
              ${!isRemote && isRam && data.platform === "Windows" ? `<button class="secondary-button" data-action="download">${icon("refresh")} ${t("workflow.downloadWinpmem")}</button>` : ""}
              ${!isRemote && isRam && data.platform === "Linux" ? `<button class="secondary-button" data-action="install-avml">${icon("download")} ${t("workflow.downloadAvml")}</button>` : ""}
              <button class="primary-button" data-action="start">${icon(isRam ? "ram" : "disk")} ${isRam ? t("workflow.startRam") : t("workflow.startImage")}</button>
            </div>

            <div class="button-row acquisition-controls" data-acquisition-controls hidden>
              <button class="secondary-button" data-action="pause">${icon("pause")} ${t("workflow.pause")}</button>
              <button class="secondary-button" data-action="resume">${icon("play")} ${t("workflow.resume")}</button>
              <button class="danger-button" data-action="stop">${icon("stop")} ${t("workflow.stop")}</button>
            </div>

            <div class="section-divider"></div>
            <p class="section-label">${t("workflow.progress")}</p>
            <div class="progress-bar" data-progress style="--value:0%"><span></span><b>0%</b></div>
            <div class="log-box" id="workflow-log">${state.lastLog.map((line) => escapeHtml(line)).join("<br />")}</div>
          </div>
        </div>

        <aside class="side-panel">
          <h3>${t("workflow.status")}</h3>
          ${sideInfo(t("workflow.platform"), `${data.platform} • ${isSsh ? "SSH (Agentless)" : isRemote ? t("remoteAgent") : t("localOperation")}`, data.icon, "", icon)}
          ${sideInfo(t("workflow.connection"), isRemote ? t("notConnected") : t("localCheckWaiting"), "monitor", "connection", icon)}
          ${sideInfo(isRam ? t("workflow.tool") : t("workflow.target"), isRam ? t("localCheckWaiting") : t("targetNotSelected"), isRam ? "chip" : "disk", "target", icon)}
          ${sideInfo(t("workflow.lastAction"), t("lastActionReady"), "clock", "last-action", icon)}
        </aside>
      </div>
    </section>
  `;
}


export function pageTitle(title, desc, iconName, icon = defaultIcon) {
  return `
    <div class="page-title">
      <span class="card-icon">${icon(iconName)}</span>
      <span>
        <h1>${title}</h1>
        <p>${desc}</p>
      </span>
    </div>
  `;
}

export function field(label, control) {
  return `
    <div class="field">
      <label>${label}</label>
      ${control}
    </div>
  `;
}

export function pickerField(label, id, value, type = "file", icon = defaultIcon, t = (key) => key) {
  const action = type === "folder" ? "pick-folder" : "pick-file";
  const placeholderOnly = value.startsWith(".") || value.toLowerCase().includes("seç") || value.toLowerCase().includes("select");
  const valueAttr = placeholderOnly ? `placeholder="${value}" value=""` : `value="${value}"`;
  return field(
    label,
    `<div class="input-action"><input id="${id}" class="input" ${valueAttr} data-picker-target /><button class="secondary-button" data-action="${action}" data-target="#${id}">${icon(type === "folder" ? "folder" : "search")} ${t("select")}</button></div>`
  );
}

function imageCasePanel({ t, icon, state, caseSelectOptions, caseOutputLabel, escapeHtml }) {
  return casePanel("ciktilar", t("workflow.caseHint"), { t, icon, state, caseSelectOptions, caseOutputLabel, escapeHtml });
}

function ramCasePanel({ t, icon, state, caseSelectOptions, caseOutputLabel, escapeHtml, canonicalRamFileName }) {
  return `
    ${casePanel("ram", t("workflow.ramCaseHint"), { t, icon, state, caseSelectOptions, caseOutputLabel, escapeHtml })}
    ${field(t("workflow.outputFileName"), `<input id="workflow-output" class="input" value="${escapeHtml(canonicalRamFileName())}" readonly />`)}
  `;
}

export function casePanel(subdir, hint, { t, icon, state, caseSelectOptions, caseOutputLabel, escapeHtml }) {
  const selected = state.activeCase?.case_name || (state.cases.length ? state.cases[0].case_name : "");
  const output = caseOutputLabel(selected, subdir);
  void hint;
  return `
    ${field(t("workflow.case"), `<select id="workflow-case" class="select" data-case-select data-allow-new-case="1">${caseSelectOptions(selected, { allowNew: true })}</select>`)}
    <div class="button-row">
      <button class="secondary-button" data-action="refresh-cases">${icon("refresh")} ${t("case.refresh")}</button>
    </div>
    <div class="side-info">
      <span class="metric-icon">${icon("folder")}</span>
      <span><strong>${t("workflow.caseOutput")}</strong><small data-case-output data-case-output-subdir="${subdir}">${escapeHtml(output)}</small></span>
    </div>
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
