import { resumeTourIfActive } from './tour.js';

// ===== Tauri v2 API helpers =====
const TAURI = window.__TAURI__;
const invoke = TAURI ? TAURI.core.invoke : null;
const listen = TAURI ? TAURI.event.listen : null;

async function tauriInvoke(cmd, args) {
  if (!invoke) throw new Error("Tauri not available");
  return await invoke(cmd, args || {});
}

async function openExternal(url) {
  if (invoke) {
    try { await tauriInvoke('open_url', { url }); } catch (e) { console.error(e); }
  } else {
    window.open(url, '_blank');
  }
}

// ===== Navigation =====
history.pushState(null, null, location.href);
window.addEventListener('popstate', (e) => {
  const btnBack = document.getElementById('btn-back');
  if (btnBack && btnBack.disabled) {
    history.pushState(null, null, location.href);
    if (typeof cancelActivation === 'function') cancelActivation();
  } else {
    window.location.href = 'home_page_v2.html';
  }
});

function goBack() {
  window.location.href = 'home_page_v2.html';
}

function closeResultModal() {
  document.getElementById('result-modal').classList.remove('active');
  window.location.href = 'home_page_v2.html';
}

// ===== Activation state =====
function setActivating(active) {
  document.getElementById('start-btn').disabled = active;
  document.getElementById('btn-back').disabled = active;
  const cancelBtn = document.getElementById('cancel-btn');
  const pauseBtn = document.getElementById('pause-btn');
  if (active) {
    cancelBtn.classList.add('active');
    pauseBtn.classList.add('active');
  } else {
    cancelBtn.classList.remove('active');
    pauseBtn.classList.remove('active');
    pauseBtn.classList.remove('paused');
    pauseBtn.textContent = 'PAUSE';
  }
}

let lastProgressPercent = 0;
let lastProgressLabel = '';

// ===== UI =====
function updateProgress(percent, text) {
  lastProgressPercent = percent;
  lastProgressLabel = text;
  document.getElementById('progress-container').classList.add('active');
  document.getElementById('progress-fill').style.width = percent + '%';
  document.getElementById('progress-text-content').textContent = text;
  document.getElementById('progress-pct').textContent = percent + '%';
}

function showSuccess(customMessage) {
  const title = document.getElementById('result-title');
  const message = document.getElementById('result-message');
  const box = document.getElementById('result-modal-box');

  title.textContent = 'ACTIVATION SUCCESSFUL';
  message.textContent = customMessage || 'Your software has been activated successfully.';
  box.className = 'modal res-success';
  document.getElementById('result-badge-icon').className = 'ti ti-check';
  document.getElementById('result-status-text').textContent = 'SYSTEM · SUCCESS';
  
  const btnIcon = document.getElementById('result-btn-icon');
  btnIcon.className = 'ti ti-arrow-right';
  const btn = document.getElementById('result-btn');
  btn.className = 'btn-success';

  document.getElementById('result-modal').classList.add('active');
  setActivating(false);
}

function showFailure(customMessage) {
  const title = document.getElementById('result-title');
  const message = document.getElementById('result-message');
  const box = document.getElementById('result-modal-box');

  title.textContent = 'ACTIVATION FAILED';
  message.textContent = customMessage || 'An error occurred during activation.';
  box.className = 'modal res-fail';
  document.getElementById('result-badge-icon').className = 'ti ti-x';
  document.getElementById('result-status-text').textContent = 'SYSTEM · ERROR';
  
  const btnIcon = document.getElementById('result-btn-icon');
  btnIcon.className = 'ti ti-refresh';
  const btn = document.getElementById('result-btn');
  btn.className = 'btn-fail';

  document.getElementById('result-modal').classList.add('active');
  setActivating(false);
}



function renderMode() {
  let mode = null;
  try { mode = sessionStorage.getItem('ta_mode'); } catch (e) {}
  if (!mode) mode = 'FMM';

  const chip = document.getElementById('mode-chip');
  const val = document.getElementById('mode-value');
  val.textContent = mode.toUpperCase();

  chip.classList.remove('live');
  if (mode.toUpperCase() === 'LIVE EDITOR') {
    chip.classList.add('live');
  }
}

async function startActivation() {
  let exeDir = null;
  try { exeDir = sessionStorage.getItem('ta_exe_dir'); } catch (e) {}
  let selection = null;
  try { selection = sessionStorage.getItem('ta_mode'); } catch (e) {}

  setActivating(true);
  document.getElementById('status-text').textContent = "Activating...";

  if (invoke) {
    try {
      await tauriInvoke('start_activation', { exeDir, selection });
    } catch (e) {
      showFailure(String(e));
    }
  } else {
    let p = 0;
    const interval = setInterval(() => {
      p += 10;
      updateProgress(p, `Task progress ${p}%`);
      if (p >= 100) {
        clearInterval(interval);
        showSuccess();
      }
    }, 500);
  }
}

async function togglePause() {
  const btn = document.getElementById('pause-btn');
  if (btn.classList.contains('paused')) {
    btn.classList.remove('paused');
    btn.textContent = 'PAUSE';
    document.getElementById('status-text').textContent = "Activating...";
    if (invoke) {
      try { await tauriInvoke('resume_activation'); } catch (e) { console.error(e); }
    }
  } else {
    btn.classList.add('paused');
    btn.textContent = 'RESUME';
    document.getElementById('status-text').textContent = "Paused";
    if (invoke) {
      try { await tauriInvoke('pause_activation'); } catch (e) { console.error(e); }
    }
  }
}

function showCancelModal() {
  // Pause in backend
  if (invoke) {
    try { tauriInvoke('pause_activation'); } catch (e) { console.error(e); }
  }
  const btn = document.getElementById('pause-btn');
  btn.classList.add('paused');
  btn.textContent = 'RESUME';
  document.getElementById('status-text').textContent = "Paused";

  // Update modal progress
  document.getElementById('cancel-progress-fill').style.width = lastProgressPercent + '%';
  document.getElementById('cancel-progress-pct').textContent = lastProgressPercent + '%';
  document.getElementById('cancel-progress-meta').textContent = lastProgressLabel;
  document.getElementById('cancel-vmain').textContent = document.getElementById('vmain').textContent;

  document.getElementById('cancel-modal').classList.add('active');
}

let currentCancelChoice = 'resume';
function selectCancelChoice(choice) {
  currentCancelChoice = choice;
  const options = document.querySelectorAll('#cancel-modal .choice-option');
  options.forEach(opt => opt.classList.remove('selected'));
  // Find the clicked element by traversing DOM but it's easier to just match index
  if (choice === 'resume') options[0].classList.add('selected');
  else if (choice === 'keep') options[1].classList.add('selected');
  else if (choice === 'delete') options[2].classList.add('selected');
}
window.selectCancelChoice = selectCancelChoice;

async function submitCancelChoice() {
  document.getElementById('cancel-modal').classList.remove('active');
  
  if (currentCancelChoice === 'resume') {
    const btn = document.getElementById('pause-btn');
    btn.classList.remove('paused');
    btn.textContent = 'PAUSE';
    document.getElementById('status-text').textContent = "Activating...";
    if (invoke) {
      try { await tauriInvoke('resume_activation'); } catch (e) { console.error(e); }
    }
  } else if (currentCancelChoice === 'keep') {
    if (invoke) { try { await tauriInvoke('cancel_activation'); } catch (e) { console.error(e); } }
    if (isExiting && invoke) {
      await tauriInvoke('exit_app');
    } else {
      setActivating(false);
      window.location.href = 'home_page_v2.html';
    }
  } else if (currentCancelChoice === 'delete') {
    if (invoke) { 
      try { await tauriInvoke('cancel_activation'); } catch (e) { console.error(e); }
      try { await tauriInvoke('clean_temp_files'); } catch (e) { console.error(e); }
    }
    if (isExiting && invoke) {
      await tauriInvoke('exit_app');
    } else {
      setActivating(false);
      window.location.href = 'home_page_v2.html';
    }
  }
}
window.submitCancelChoice = submitCancelChoice;

async function cancelActivation() {
  showCancelModal();
}
window.cancelActivation = cancelActivation;

// ===== Event listeners for backend =====
let isExiting = false;
async function setupEventListeners() {
  if (!listen) return;

  await listen('activation-progress', (event) => {
    const { percent, label } = event.payload || {};
    const pct = typeof percent === 'number' ? Math.floor(percent) : 0;
    updateProgress(pct, label || 'Working');
  });

  await listen('activation-done', (event) => {
    const { success, message } = event.payload || {};
    if (success) {
      updateProgress(100, `Complete (100%)`);
      showSuccess(message);
    } else {
      showFailure(message);
    }
  });

  await listen('show-exit-modal', () => {
    isExiting = true;
    const isActivating = document.getElementById('start-btn').disabled; // true if active
    if (isActivating) {
      showCancelModal();
    } else {
      // Just close the app, no download is active
      if (invoke) tauriInvoke('exit_app');
    }
  });
}

window.onload = async () => {
  resumeTourIfActive();
  renderMode();

  // Setup Tauri event listeners
  setupEventListeners();

  // Fetch game version
  if (invoke) {
    let exeDir = null;
    try { exeDir = sessionStorage.getItem('ta_exe_dir'); } catch (e) {}
    if (!exeDir) {
      try { exeDir = await tauriInvoke('get_exe_dir'); } catch (e) { console.error(e); }
      if (exeDir) { try { sessionStorage.setItem('ta_exe_dir', exeDir); } catch (e) {} }
    }

    if (exeDir) {
      try {
        const result = await tauriInvoke('get_game_version', { exeDir });
        // Rust returns tuple (version1, version2) which arrives as array [v1, v2]
        const [v1, v2] = Array.isArray(result) ? result : [result, ''];
        document.getElementById('vmain').textContent = v1 || 'Unknown';
        document.getElementById('vsem').textContent = v2 || 'Unknown';
      } catch (e) {
        console.error("Failed to get game version:", e);
        document.getElementById('vmain').textContent = 'Unknown';
        document.getElementById('vsem').textContent = 'Unknown';
      }
    }
  } else {
    // Browser fallback demo values
    document.getElementById('vmain').textContent = "1.0.136.44486";
    document.getElementById('vsem').textContent = "1.6.1";
  }
};

// Global exports for inline HTML handlers
window.goBack = goBack;
window.closeResultModal = closeResultModal;
window.startActivation = startActivation;
window.togglePause = togglePause;
window.cancelActivation = cancelActivation;
window.openExternal = openExternal;

// Prevent mouse back/forward navigation
window.addEventListener('mouseup', (e) => {
    if (e.button === 3 || e.button === 4) {
        e.preventDefault();
        e.stopPropagation();
    }
});
window.addEventListener('mousedown', (e) => {
    if (e.button === 3 || e.button === 4) {
        e.preventDefault();
        e.stopPropagation();
    }
});

window.exitApp = async () => { try { await tauriInvoke('exit_app'); } catch(e){ window.close(); } };
