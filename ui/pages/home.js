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
        ${homeTile(t("home.acquire.title"), t("home.acquire.desc"), "disk", "windows", "var(--text)", icon)}
        ${homeTile(t("home.integrity.title"), t("home.integrity.desc"), "shield", "other", "var(--text)", icon)}
        ${homeTile(t("home.evidence.title"), t("home.evidence.desc"), "scale", "other", "var(--text)", icon)}
        ${homeTile(t("home.output.title"), t("home.output.desc"), "report", "other", "var(--text)", icon)}
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
