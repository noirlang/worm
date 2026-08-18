export function analysisPage({ t, icon, state, pageTitle, pickerField, caseSelectOptions }) {
  return `
    <section class="page">
      ${pageTitle(t("analysis.title"), t("analysis.desc"), "search")}
      
      <div class="workflow-panel" style="display: flex; flex-direction: column; align-items: center; justify-content: center; min-height: 420px; text-align: center; padding: 48px 24px; margin-top: 16px;">
        <div style="width: 72px; height: 72px; border-radius: 50%; background: var(--surface-2, rgba(255,255,255,0.06)); border: 1px solid var(--line); display: flex; align-items: center; justify-content: center; margin-bottom: 20px; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25); color: var(--accent);">
          <span style="display: flex; transform: scale(1.4);">${icon("search")}</span>
        </div>
        
        <span class="status-badge" style="margin-bottom: 14px; background: rgba(59, 130, 246, 0.12); color: #60a5fa; border: 1px solid rgba(59, 130, 246, 0.25); padding: 4px 14px; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.8px;">
          Geliştirme Aşamasında / In Development
        </span>
        
        <h2 style="font-size: 24px; font-weight: 700; margin: 0 0 10px 0; color: var(--text);">
          Yakında / Coming Soon
        </h2>
        
        <p style="max-width: 540px; font-size: 14px; line-height: 1.6; color: var(--muted); margin: 0 0 24px 0;">
          Gelişmiş adli analiz motoru (Disk İmajı İnceleme, RAM Bellek Taraması, Volatility3 ve Android Kanıt Analizi) yeni nesil mimari ile yeniden tasarlanmaktadır. Çok yakında yeni özelliklerle kullanıma sunulacaktır.
        </p>
        
        <div style="display: inline-flex; align-items: center; gap: 8px; font-size: 12px; color: var(--muted); padding: 8px 16px; background: var(--surface, rgba(0,0,0,0.15)); border-radius: 20px; border: 1px solid var(--line);">
          ${icon("info")} <span>Amele Forensic Tool v0.0.17</span>
        </div>
      </div>
    </section>
  `;
}
