import { formatBytes } from "./utils.js";

function injectModalStyles() {
  if (document.getElementById("storage-check-styles")) return;
  const style = document.createElement("style");
  style.id = "storage-check-styles";
  style.textContent = `
    .sc-modal-overlay {
      position: fixed;
      top: 0; left: 0; right: 0; bottom: 0;
      background: rgba(15, 23, 42, 0.6);
      backdrop-filter: blur(8px);
      -webkit-backdrop-filter: blur(8px);
      display: flex;
      justify-content: center;
      align-items: center;
      z-index: 10000;
      font-family: system-ui, -apple-system, sans-serif;
      animation: sc-fadein 0.2s ease-out;
    }
    
    @keyframes sc-fadein {
      from { opacity: 0; }
      to { opacity: 1; }
    }
    
    .sc-modal {
      background: #1e293b;
      border-radius: 12px;
      width: 440px;
      max-width: 90vw;
      box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.5), 0 10px 10px -5px rgba(0, 0, 0, 0.3);
      overflow: hidden;
      animation: sc-slideup 0.3s ease-out;
      border: 1px solid rgba(255, 255, 255, 0.1);
    }
    
    @keyframes sc-slideup {
      from { transform: translateY(20px); opacity: 0; }
      to { transform: translateY(0); opacity: 1; }
    }
    
    .sc-header {
      background: linear-gradient(135deg, #ef4444, #f97316);
      padding: 16px 20px;
      display: flex;
      align-items: center;
      gap: 12px;
      color: white;
    }
    
    .sc-header-icon {
      font-size: 24px;
    }
    
    .sc-header-title {
      font-size: 18px;
      font-weight: 600;
      margin: 0;
    }
    
    .sc-content {
      padding: 24px 20px;
      color: #cbd5e1;
    }
    
    .sc-warning-msg {
      background: rgba(239, 68, 68, 0.1);
      border-left: 3px solid #ef4444;
      padding: 12px;
      margin-bottom: 20px;
      color: #f8fafc;
      font-size: 14px;
      border-radius: 4px;
    }
    
    .sc-stats {
      display: flex;
      flex-direction: column;
      gap: 12px;
      margin-bottom: 24px;
    }
    
    .sc-stat-row {
      display: flex;
      justify-content: space-between;
      border-bottom: 1px solid rgba(255,255,255,0.05);
      padding-bottom: 8px;
    }
    
    .sc-stat-label {
      color: #94a3b8;
      font-size: 14px;
    }
    
    .sc-stat-value {
      color: #f8fafc;
      font-weight: 500;
      font-size: 14px;
    }
    
    .sc-stat-value.sc-danger {
      color: #ef4444;
    }
    
    .sc-actions {
      display: flex;
      justify-content: flex-end;
      gap: 12px;
      padding-top: 16px;
      border-top: 1px solid rgba(255, 255, 255, 0.1);
    }
    
    .sc-btn {
      padding: 10px 16px;
      border-radius: 6px;
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      transition: all 0.2s;
    }
    
    .sc-btn-cancel {
      background: #ef4444;
      color: white;
      border: none;
    }
    
    .sc-btn-cancel:hover {
      background: #dc2626;
    }
    
    .sc-btn-continue {
      background: transparent;
      color: #eab308;
      border: 1px solid #eab308;
    }
    
    .sc-btn-continue:hover {
      background: rgba(234, 179, 8, 0.1);
    }
  `;
  document.head.appendChild(style);
}

function showModal(data) {
  return new Promise((resolve) => {
    injectModalStyles();
    
    const overlay = document.createElement("div");
    overlay.className = "sc-modal-overlay";
    
    const requiredSize = data.required_bytes || 0;
    const availableSize = data.available_bytes || 0;
    const shortage = Math.max(0, requiredSize - availableSize);
    
    const requiredStr = formatBytes(requiredSize);
    const availableStr = formatBytes(availableSize);
    const shortageStr = formatBytes(shortage);
    
    const warningMsg = data.warning_message || "Disk alanı yetersiz olabilir.";
    
    overlay.innerHTML = `
      <div class="sc-modal">
        <div class="sc-header">
          <span class="sc-header-icon">⚠️</span>
          <h2 class="sc-header-title">Yetersiz Disk Alanı Uyarısı</h2>
        </div>
        <div class="sc-content">
          <div class="sc-warning-msg">${warningMsg}</div>
          
          <div class="sc-stats">
            <div class="sc-stat-row">
              <span class="sc-stat-label">Kaynak Boyutu (Tahmini):</span>
              <span class="sc-stat-value">${requiredStr}</span>
            </div>
            <div class="sc-stat-row">
              <span class="sc-stat-label">Hedefteki Boş Alan:</span>
              <span class="sc-stat-value">${availableStr}</span>
            </div>
            <div class="sc-stat-row">
              <span class="sc-stat-label">Eksik Alan:</span>
              <span class="sc-stat-value sc-danger">${shortageStr}</span>
            </div>
          </div>
          
          <div class="sc-actions">
            <button class="sc-btn sc-btn-continue" id="sc-btn-continue">Devam Et (Riskli)</button>
            <button class="sc-btn sc-btn-cancel" id="sc-btn-cancel">İptal Et</button>
          </div>
        </div>
      </div>
    `;
    
    document.body.appendChild(overlay);
    
    const close = (action) => {
      overlay.style.opacity = '0';
      setTimeout(() => {
        if (overlay.parentNode) {
          overlay.parentNode.removeChild(overlay);
        }
        resolve({ action });
      }, 200);
    };
    
    overlay.querySelector("#sc-btn-continue").addEventListener("click", () => close('continue'));
    overlay.querySelector("#sc-btn-cancel").addEventListener("click", () => close('cancel'));
  });
}

export async function preflightStorageCheck({ sourcePath, sourceType, caseName }) {
  try {
    const response = await fetch("/api/preflight-storage-check", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        source_path: sourcePath,
        source_type: sourceType,
        case_name: caseName
      })
    });
    
    if (!response.ok) {
      // If API fails, we just silently allow continue, or fallback to throwing.
      // Usually preflight shouldn't block on network error unless requested.
      return { action: 'continue' };
    }
    
    const data = await response.json();
    
    if (data.is_sufficient === true) {
      return { action: 'continue' };
    }
    
    // Otherwise show modal
    return await showModal(data);
    
  } catch (error) {
    console.warn("Preflight check failed:", error);
    // On fetch error, just continue
    return { action: 'continue' };
  }
}
