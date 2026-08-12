export function homePage({ t, icon, assetPath, theme }) {
  const logoFile = "amele.png";
  return `
    <section class="page">
      <div class="hero home-hero">
        <div class="amele-art">
          <img src="${assetPath}/logo/${logoFile}" alt="Amele logo" />
        </div>
      </div>

      <div class="home-grid">
        ${homeTile(t("home.windows.title"), t("home.windows.desc"), "windows", "windows", "var(--text)", icon)}
        ${homeTile(t("home.linux.title"), t("home.linux.desc"), "linux", "linux", "var(--text)", icon)}
        ${homeTile(t("home.docker.title"), t("home.docker.desc"), "docker", "docker", "var(--text)", icon)}
        ${homeTile(t("home.android.title"), t("home.android.desc"), "android", "android", "var(--text)", icon)}
        ${homeTile(t("home.ios.title"), t("home.ios.desc"), "ios", "ios", "var(--text)", icon)}
        ${homeTile(t("home.agent.title"), t("home.agent.desc"), "network", "agent", "var(--text)", icon)}
        ${homeTile(t("home.analysis.title"), t("home.analysis.desc"), "search", "analysis", "var(--text)", icon)}
        ${homeTile(t("home.other.title"), t("home.other.desc"), "tiles", "other", "var(--text)", icon)}
      </div>
    </section>
  `;
}

function homeTile(title, desc, iconName, route, accent, icon) {
  return `
    <button class="action-tile" data-route="${route}" style="--accent:${accent}">
      <span class="tile-icon">${icon(iconName)}</span>
      <span>
        <h3>${title}</h3>
        <p>${desc}</p>
      </span>
      <span class="tile-arrow">→</span>
    </button>
  `;
}

export function metric(label, value, iconName, accent, icon) {
  return `
    <div class="metric" style="--accent:${accent}">
      <span class="metric-icon">${icon(iconName)}</span>
      <span><small>${label}</small><strong>${value}</strong></span>
    </div>
  `;
}
