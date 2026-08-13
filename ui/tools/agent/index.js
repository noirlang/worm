export function agentPage({ t, icon, state, pageTitle }) {
  return `
    <section class="page">
      ${pageTitle("Agent", t("agent.desc"), "network")}
      <div class="doc-grid">
        ${agentDoc({
          title: "Windows Agent",
          repo: "https://github.com/noirlang/amele-win",
          binary: "amele-win.exe",
          url: "https://amele.noirlang.tr/amele-win.exe",
          note: t("agent.windowsNote"),
          iconName: "windows",
          stepsTr: [
            t("agent.downloadWin"),
            t("agent.runWin"),
            t("agent.match")
          ],
          stepsEn: [
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
          stepsTr: [
            t("agent.downloadLinux"),
            t("agent.chmod"),
            t("agent.runLinux"),
            t("agent.connect")
          ],
          stepsEn: [
            t("agent.downloadLinux"),
            t("agent.chmod"),
            t("agent.runLinux"),
            t("agent.connect")
          ]
        }, state, icon)}
      </div>
    </section>
  `;
}

function agentDoc({ title, repo, binary, url, note, iconName, stepsTr, stepsEn }, state, icon) {
  const commands = iconName === "linux"
    ? `wget -O ${binary} ${url}\nchmod +x ${binary}\n./${binary}`
    : `wget -O ${binary} ${url}\n${state.language === "en" ? `Run ${binary} as Administrator.` : `${binary} dosyasını yönetici olarak çalıştırın.`}`;
  const steps = state.language === "en" ? stepsEn : stepsTr;
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
