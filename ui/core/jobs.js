import { escapeHtml } from "./utils.js";

let widgetElement = null;
let minimizedElement = null;
let containerElement = null;
let currentJobs = {};
let isMinimized = false;
let pollingInterval = null;

const TOOL_ICONS = {
  disk: "💿",
  android: "📱",
  ios: "🍎",
  docker: "🐳",
  ram: "🧠",
  default: "⚙️"
};

function getIconForJobId(jobId) {
  const lowerId = (jobId || "").toLowerCase();
  if (lowerId.includes("disk")) return TOOL_ICONS.disk;
  if (lowerId.includes("android")) return TOOL_ICONS.android;
  if (lowerId.includes("ios")) return TOOL_ICONS.ios;
  if (lowerId.includes("docker")) return TOOL_ICONS.docker;
  if (lowerId.includes("ram") || lowerId.includes("memory")) return TOOL_ICONS.ram;
  return TOOL_ICONS.default;
}

function truncate(str, maxLength = 30) {
  if (!str) return "";
  if (str.length <= maxLength) return str;
  return str.slice(0, maxLength - 3) + "...";
}

function injectStyles() {
  if (document.getElementById("jobs-widget-styles")) return;
  const style = document.createElement("style");
  style.id = "jobs-widget-styles";
  style.textContent = `
    #amele-jobs-widget {
      position: fixed;
      bottom: 24px;
      right: 24px;
      z-index: 9999;
      display: none;
      font-family: system-ui, -apple-system, sans-serif;
      transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
      transform: translateY(0);
      opacity: 1;
    }
    
    #amele-jobs-widget.hidden-state {
      transform: translateY(120%);
      opacity: 0;
    }

    #amele-jobs-container {
      background: rgba(15, 23, 42, 0.7);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      border-radius: 12px;
      padding: 12px;
      width: 320px;
      box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.5), 0 8px 10px -6px rgba(0, 0, 0, 0.3);
      position: relative;
      border: 1px solid transparent;
      background-clip: padding-box;
    }
    
    #amele-jobs-container::before {
      content: '';
      position: absolute;
      top: 0; right: 0; bottom: 0; left: 0;
      z-index: -1;
      margin: -1px;
      border-radius: inherit;
      background: linear-gradient(135deg, #06b6d4, #14b8a6);
    }

    .jobs-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 8px;
      padding-bottom: 8px;
      border-bottom: 1px solid rgba(255, 255, 255, 0.1);
    }
    
    .jobs-title {
      color: #fff;
      font-weight: 600;
      font-size: 14px;
    }
    
    .jobs-minimize-btn {
      background: rgba(255, 255, 255, 0.1);
      border: none;
      color: #cbd5e1;
      cursor: pointer;
      border-radius: 4px;
      padding: 2px 6px;
      font-size: 12px;
      transition: background 0.2s;
    }
    
    .jobs-minimize-btn:hover {
      background: rgba(255, 255, 255, 0.2);
      color: #fff;
    }

    .job-item {
      display: flex;
      flex-direction: column;
      gap: 6px;
      padding: 8px;
      background: rgba(255, 255, 255, 0.03);
      border-radius: 8px;
      margin-bottom: 8px;
      cursor: pointer;
      transition: background 0.2s;
    }
    
    .job-item:last-child {
      margin-bottom: 0;
    }
    
    .job-item:hover {
      background: rgba(255, 255, 255, 0.08);
    }

    .job-info {
      display: flex;
      align-items: center;
      gap: 8px;
    }
    
    .job-icon {
      font-size: 16px;
    }
    
    .job-desc {
      color: #f8fafc;
      font-size: 13px;
      font-weight: 500;
      flex: 1;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .job-progress-bg {
      height: 4px;
      background: rgba(255, 255, 255, 0.1);
      border-radius: 2px;
      overflow: hidden;
    }
    
    .job-progress-fill {
      height: 100%;
      background: linear-gradient(90deg, #06b6d4, #14b8a6);
      transition: width 0.3s ease;
    }

    .job-status {
      display: flex;
      justify-content: space-between;
      color: #94a3b8;
      font-size: 11px;
    }

    /* Minimized Pill */
    #amele-jobs-minimized {
      display: none;
      background: rgba(15, 23, 42, 0.8);
      backdrop-filter: blur(16px);
      -webkit-backdrop-filter: blur(16px);
      border: 1px solid #14b8a6;
      border-radius: 20px;
      padding: 6px 12px;
      color: #fff;
      font-size: 13px;
      font-weight: 500;
      cursor: pointer;
      align-items: center;
      gap: 8px;
      box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
    }
    
    .spinner {
      width: 14px;
      height: 14px;
      border: 2px solid rgba(255, 255, 255, 0.3);
      border-top-color: #14b8a6;
      border-radius: 50%;
      animation: spin 1s linear infinite;
    }
    
    @keyframes spin {
      to { transform: rotate(360deg); }
    }
  `;
  document.head.appendChild(style);
}

function createDOM() {
  widgetElement = document.createElement("div");
  widgetElement.id = "amele-jobs-widget";
  widgetElement.className = "hidden-state";
  
  containerElement = document.createElement("div");
  containerElement.id = "amele-jobs-container";
  
  const header = document.createElement("div");
  header.className = "jobs-header";
  header.innerHTML = `
    <div class="jobs-title">Aktif İşlemler</div>
    <button class="jobs-minimize-btn">Gizle</button>
  `;
  
  header.querySelector(".jobs-minimize-btn").addEventListener("click", () => {
    isMinimized = true;
    render();
  });
  
  const jobsList = document.createElement("div");
  jobsList.id = "amele-jobs-list";
  
  containerElement.appendChild(header);
  containerElement.appendChild(jobsList);
  
  minimizedElement = document.createElement("div");
  minimizedElement.id = "amele-jobs-minimized";
  minimizedElement.innerHTML = `
    <div class="spinner"></div>
    <span class="count-text">0 İşlem</span>
  `;
  
  minimizedElement.addEventListener("click", () => {
    isMinimized = false;
    render();
  });
  
  widgetElement.appendChild(containerElement);
  widgetElement.appendChild(minimizedElement);
  document.body.appendChild(widgetElement);
}

function handleJobClick(jobId) {
  // Navigate to tool page based on jobId or some generic approach
  let targetPath = '';
  if (jobId.includes('disk')) targetPath = '#/disk';
  else if (jobId.includes('android')) targetPath = '#/android';
  else if (jobId.includes('ios')) targetPath = '#/ios';
  else if (jobId.includes('docker')) targetPath = '#/docker';
  else if (jobId.includes('ram')) targetPath = '#/ram';
  else targetPath = '#/jobs'; // fallback
  
  window.location.hash = targetPath;
  // Trigger hashchange manually just in case
  window.dispatchEvent(new Event('hashchange'));
}

function render() {
  const activeJobEntries = Object.entries(currentJobs).filter(([_, job]) => job.status === "running");
  
  if (activeJobEntries.length === 0) {
    widgetElement.classList.add("hidden-state");
    setTimeout(() => {
      if (Object.keys(currentJobs).filter(k => currentJobs[k].status === "running").length === 0) {
        widgetElement.style.display = "none";
      }
    }, 300);
    return;
  }
  
  widgetElement.style.display = "block";
  // Trigger reflow
  void widgetElement.offsetWidth;
  widgetElement.classList.remove("hidden-state");
  
  if (isMinimized) {
    containerElement.style.display = "none";
    minimizedElement.style.display = "flex";
    minimizedElement.querySelector(".count-text").textContent = `${activeJobEntries.length} İşlem`;
  } else {
    containerElement.style.display = "block";
    minimizedElement.style.display = "none";
    
    const listEl = containerElement.querySelector("#amele-jobs-list");
    listEl.innerHTML = "";
    
    activeJobEntries.forEach(([id, job]) => {
      const el = document.createElement("div");
      el.className = "job-item";
      el.onclick = () => handleJobClick(id);
      
      const pct = job.total ? Math.min(100, Math.round((job.done / job.total) * 100)) : 0;
      const msg = truncate(job.message || "İşlem devam ediyor...", 40);
      const icon = getIconForJobId(id);
      
      el.innerHTML = `
        <div class="job-info">
          <span class="job-icon">${icon}</span>
          <span class="job-desc">${escapeHtml(id)}</span>
        </div>
        <div class="job-progress-bg">
          <div class="job-progress-fill" style="width: ${pct}%"></div>
        </div>
        <div class="job-status">
          <span class="job-msg">${escapeHtml(msg)}</span>
          <span class="job-pct">${pct}%</span>
        </div>
      `;
      listEl.appendChild(el);
    });
  }
}

async function pollJobs() {
  try {
    const res = await fetch("/api/acquisition-status");
    if (res.ok) {
      const data = await res.json();
      currentJobs = data.jobs || {};
      render();
    }
  } catch (err) {
    // silently fail and try again next tick
  }
}

export function initJobWidget() {
  if (widgetElement) return;
  injectStyles();
  createDOM();
  pollJobs();
  pollingInterval = setInterval(pollJobs, 800);
}

export function getActiveJobs() {
  return Object.entries(currentJobs)
    .filter(([_, job]) => job.status === "running")
    .map(([id, job]) => ({ id, ...job }));
}

export function isToolBusy(toolName) {
  const toolLower = (toolName || "").toLowerCase();
  const jobs = getActiveJobs();
  return jobs.some(job => job.id.toLowerCase().includes(toolLower));
}
