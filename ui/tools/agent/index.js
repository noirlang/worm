export function agentPage({ t, icon, state, pageTitle }) {
  return `
    <section class="page">
      ${pageTitle("Agent", t("agent.desc"), "network")}

      <div class="agent-mode-banner">
        <div class="agent-mode-card agent-mode-with">
          <span class="card-icon">${icon("network")}</span>
          <div>
            <h3>${t("agent.withAgent.title")}</h3>
            <p>${t("agent.withAgent.desc")}</p>
            <ul class="agent-feature-list">
              <li>${icon("disk")} ${t("agent.withAgent.disk")}</li>
              <li>${icon("ram")} ${t("agent.withAgent.ram")}</li>
              <li>${icon("shield")} ${t("agent.withAgent.gpg")}</li>
              <li>${icon("play")} ${t("agent.withAgent.live")}</li>
            </ul>
          </div>
        </div>
        <div class="agent-mode-card agent-mode-without">
          <span class="card-icon">${icon("key")}</span>
          <div>
            <h3>${t("agent.noAgent.title")}</h3>
            <p>${t("agent.noAgent.desc")}</p>
            <ul class="agent-feature-list">
              <li>${icon("linux")} ${t("agent.noAgent.linux")}</li>
              <li>${icon("windows")} ${t("agent.noAgent.windows")}</li>
              <li>${icon("disk")} ${t("agent.noAgent.disk")}</li>
              <li>${icon("ram")} ${t("agent.noAgent.ram")}</li>
            </ul>
          </div>
        </div>
      </div>

      <h2 class="section-heading">${t("agent.agentSection")}</h2>
      <div class="doc-grid">
        ${agentDoc({
          title: "Windows Agent",
          repo: "https://github.com/noirlang/amele-win",
          binary: "amele-win.exe",
          url: "https://amele.noirlang.tr/amele-win.exe",
          note: t("agent.windowsNote"),
          iconName: "windows",
          steps: [
            t("agent.downloadWin"),
            t("agent.runWin"),
            t("agent.match")
          ]
        }, state, icon)}
        ${agentDoc({
          title: "Linux Agent",
          repo: "https://github.com/noirlang/amele-linux",
          binary: "amele-linux",
          url: "https://amele.noirlang.tr/amele-linux",
          note: t("agent.linuxNote"),
          iconName: "linux",
          steps: [
            t("agent.downloadLinux"),
            t("agent.chmod"),
            t("agent.runLinux"),
            t("agent.connect")
          ]
        }, state, icon)}
      </div>

      <h2 class="section-heading">${t("agent.noAgentSection")}</h2>
      <div class="doc-grid">
        ${agentlessDoc({
          title: t("agent.ssh.title"),
          iconName: "linux",
          note: t("agent.ssh.desc"),
          protocol: "SSH",
          port: "22",
          steps: [
            t("agent.ssh.step1"),
            t("agent.ssh.step2"),
            t("agent.ssh.step3"),
            t("agent.ssh.step4")
          ],
          command: `ssh user@<IP> "dd if=/dev/sda bs=4M status=progress" | dd of=disk.img`,
          tag: "Linux / Unix"
        }, icon)}
        ${agentlessDoc({
          title: t("agent.winrm.title"),
          iconName: "windows",
          note: t("agent.winrm.desc"),
          protocol: "WinRM / SSH",
          port: "5985 / 22",
          steps: [
            t("agent.winrm.step1"),
            t("agent.winrm.step2"),
            t("agent.winrm.step3"),
            t("agent.winrm.step4")
          ],
          command: `Enable-PSRemoting -Force\n# veya\nStart-Service sshd`,
          tag: "Windows 2019/11+"
        }, icon)}
      </div>
    </section>
  `;
}

function agentDoc({ title, repo, binary, url, note, iconName, steps }, state, icon) {
  const commands = iconName === "linux"
    ? `wget -O ${binary} ${url}\nchmod +x ${binary}\n./${binary}`
    : `wget -O ${binary} ${url}\n${state.language === "en" ? `Run ${binary} as Administrator.` : `${binary} dosyasını yönetici olarak çalıştırın.`}`;
  return `
    <article class="doc-card">
      <span class="card-icon">${icon(iconName)}</span>
      <h3>${title}</h3>
      <p>${note}</p>
      <div class="link-row">
        <a href="${repo}">${repo}</a>
        <a href="${url}">${url}</a>
      </div>
      <ol class="step-list">
        ${steps.map((step, index) => `<li><b>${index + 1}</b><span>${step}</span></li>`).join("")}
      </ol>
      <div class="code-box">${commands}</div>
    </article>
  `;
}

function agentlessDoc({ title, iconName, note, protocol, port, steps, command, tag }, icon) {
  return `
    <article class="doc-card agentless-card">
      <div class="agentless-header">
        <span class="card-icon">${icon(iconName)}</span>
        <span class="agentless-tag">${tag}</span>
      </div>
      <h3>${title}</h3>
      <p>${note}</p>
      <div class="agentless-meta">
        <span>${icon("network")} ${protocol}</span>
        <span>${icon("shield")} Port: ${port}</span>
      </div>
      <ol class="step-list">
        ${steps.map((step, index) => `<li><b>${index + 1}</b><span>${step}</span></li>`).join("")}
      </ol>
      <div class="code-box">${command}</div>
    </article>
  `;
}
