export function remoteAcqPage({ t, icon, pageTitle }) {
  return `
    <section class="page">
      ${pageTitle(t("remote.title"), t("remote.desc"), "key")}

      <div class="agent-mode-banner">
        <div class="agent-mode-card agent-mode-with">
          <span class="card-icon">${icon("linux")}</span>
          <div>
            <h3>${t("remote.ssh.card.title")}</h3>
            <p>${t("remote.ssh.card.desc")}</p>
            <ul class="agent-feature-list">
              <li>${icon("disk")} ${t("remote.ssh.card.disk")}</li>
              <li>${icon("ram")} ${t("remote.ssh.card.ram")}</li>
              <li>${icon("shield")} ${t("remote.ssh.card.auth")}</li>
              <li>${icon("arrow")} ${t("remote.ssh.card.pipe")}</li>
            </ul>
          </div>
        </div>
        <div class="agent-mode-card agent-mode-without">
          <span class="card-icon">${icon("windows")}</span>
          <div>
            <h3>${t("remote.winrm.card.title")}</h3>
            <p>${t("remote.winrm.card.desc")}</p>
            <ul class="agent-feature-list">
              <li>${icon("disk")} ${t("remote.winrm.card.disk")}</li>
              <li>${icon("ram")} ${t("remote.winrm.card.ram")}</li>
              <li>${icon("shield")} ${t("remote.winrm.card.auth")}</li>
              <li>${icon("arrow")} ${t("remote.winrm.card.pipe")}</li>
            </ul>
          </div>
        </div>
      </div>

      <h2 class="section-heading">${t("remote.ssh.section")}</h2>
      <div class="doc-grid">
        ${remoteDoc({
          title: t("remote.ssh.disk.title"),
          iconName: "linux",
          tag: "SSH · Linux / Unix",
          note: t("remote.ssh.disk.desc"),
          steps: [
            t("remote.ssh.disk.step1"),
            t("remote.ssh.disk.step2"),
            t("remote.ssh.disk.step3"),
            t("remote.ssh.disk.step4")
          ],
          command: `# Disk listele\nssh user@<IP> "lsblk -J"\n\n# Disk imajını akıt\nssh user@<IP> "sudo dd if=/dev/sda bs=4M status=progress" \\\n  | dd of=./remote_disk.img`
        }, icon)}
        ${remoteDoc({
          title: t("remote.ssh.ram.title"),
          iconName: "linux",
          tag: "SSH · AVML",
          note: t("remote.ssh.ram.desc"),
          steps: [
            t("remote.ssh.ram.step1"),
            t("remote.ssh.ram.step2"),
            t("remote.ssh.ram.step3"),
            t("remote.ssh.ram.step4")
          ],
          command: `# AVML ile uzak RAM dökümü\nssh user@<IP> "sudo avml /dev/stdout" \\\n  | dd of=./remote_ram.lime\n\n# veya /proc/kcore yöntemi (root gerekir)\nssh user@<IP> "sudo dd if=/proc/kcore" \\\n  | dd of=./remote_kcore.img`
        }, icon)}
      </div>

      <h2 class="section-heading">${t("remote.winrm.section")}</h2>
      <div class="doc-grid">
        ${remoteDoc({
          title: t("remote.winrm.disk.title"),
          iconName: "windows",
          tag: "WinRM / OpenSSH",
          note: t("remote.winrm.disk.desc"),
          steps: [
            t("remote.winrm.disk.step1"),
            t("remote.winrm.disk.step2"),
            t("remote.winrm.disk.step3"),
            t("remote.winrm.disk.step4")
          ],
          command: `# WinRM aktif et (hedef PowerShell)\nEnable-PSRemoting -Force\n\n# Windows SSH varsa doğrudan:\nssh Admin@<IP> "diskpart /s list.txt"\n\n# Disk akışı (Windows SSH):\nssh Admin@<IP> "dd.exe if=\\\\\\\\.\\\\PhysicalDrive0 bs=4M" ^\n  | dd of=./remote_win_disk.img`
        }, icon)}
        ${remoteDoc({
          title: t("remote.winrm.ram.title"),
          iconName: "windows",
          tag: "WinPMEM · SSH",
          note: t("remote.winrm.ram.desc"),
          steps: [
            t("remote.winrm.ram.step1"),
            t("remote.winrm.ram.step2"),
            t("remote.winrm.ram.step3"),
            t("remote.winrm.ram.step4")
          ],
          command: `# WinPMEM ile uzak RAM dökümü\nssh Admin@<IP> "winpmem_mini.exe -" ^\n  | dd of=./remote_win_ram.aff4\n\n# WinRM ile:\nInvoke-Command -ComputerName <IP> {\n  & winpmem_mini.exe C:\\Temp\\ram.aff4\n}`
        }, icon)}
      </div>
    </section>
  `;
}

function remoteDoc({ title, iconName, tag, note, steps, command }, icon) {
  return `
    <article class="doc-card agentless-card">
      <div class="agentless-header">
        <span class="card-icon">${icon(iconName)}</span>
        <span class="agentless-tag">${tag}</span>
      </div>
      <h3>${title}</h3>
      <p>${note}</p>
      <ol class="step-list">
        ${steps.map((step, i) => `<li><b>${i + 1}</b><span>${step}</span></li>`).join("")}
      </ol>
      <div class="code-box">${command}</div>
    </article>
  `;
}
