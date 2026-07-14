import { startTour, resumeTourIfActive } from './tour.js';

// ===== Tauri v2 API helpers =====
const TAURI = window.__TAURI__;
const invoke = TAURI ? TAURI.core.invoke : null;

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

// ===== Update System =====
let pendingUpdate = null;

async function checkForUpdate() {
  if (!TAURI) return null;
  try {
    const { check } = await TAURI.updater;
    const update = await check();
    return update;
  } catch (e) {
    console.log('Update check:', e);
    return null;
  }
}

function showUpdateModal(update) {
  pendingUpdate = update;
  const version = update.version || '0.0.0';
  const currentVersion = window.APP_VERSION || '1.0.0';
  const versionChip = document.querySelector('#update-modal .version-chip');
  if (versionChip) {
    versionChip.innerHTML = `<div class="ver">V ${currentVersion}</div><div class="arrow">→</div><div class="ver new">V ${version}</div>`;
    versionChip.style.direction = 'ltr';
  }
  
  document.getElementById('update-modal').classList.add('active');
}

async function doUpdate() {
  if (!pendingUpdate) return;
  const progress = document.getElementById('update-progress');
  const bar = document.getElementById('update-bar');
  const status = document.getElementById('update-status');
  progress.style.display = 'block';

  if (invoke) {
    // We no longer set relocate_target here to avoid auto-relocate bugs
  }

  try {
    const version = pendingUpdate.version;
    const unlisten = await TAURI.event.listen('update-download-progress', (event) => {
      const payload = event.payload;
      if (payload && payload.percent !== undefined) {
        bar.style.width = payload.percent + '%';
        status.textContent = payload.label || `Downloading... ${payload.percent}%`;
      }
    });

    await tauriInvoke('portable_update', { version: version });
    
    // The Rust command exits the process, so this won't be reached
  } catch (e) {
    status.textContent = 'Update failed: ' + e;
    status.style.color = 'var(--red-1)';
  }
}

// ===== State =====
let selectedMode = null;
let exeDir = null;

// Called when user clicks NEGLECT — skips update and continues
async function continueAfterUpdateCheck() {
  const update = await checkForUpdate();
  if (update && update.available) {
    showUpdateModal(update);
    return;
  }

  try { sessionStorage.setItem('ta_mode', selectedMode); } catch (e) {}
  if (exeDir) {
    try { sessionStorage.setItem('ta_exe_dir', exeDir); } catch (e) {}
  }

  if (invoke) {
    try {
      const ok = await tauriInvoke('check_game_folder', { exeDir });
      if (ok) {
        window.location.href = 'version_page_v2.html';
      } else {
        await showErrorModalWithDiagnostics();
      }
    } catch (e) {
      console.error(e);
      await showErrorModalWithDiagnostics();
    }
  } else {
    window.location.href = 'version_page_v2.html';
  }
}

function selectMode(mode) {
  if (mode === 'FMM' || mode === 'Live Editor') {
    selectedMode = mode;
    const fmmEl = document.getElementById('opt-fmm');
    const liveEl = document.getElementById('opt-live');
    if (fmmEl) fmmEl.classList.toggle('selected', mode === 'FMM');
    if (liveEl) liveEl.classList.toggle('selected', mode === 'Live Editor');
  } else {
    window.gameMode = mode;
    const onlineEl = document.getElementById('mode-online');
    const offlineEl = document.getElementById('mode-offline');
    if (onlineEl) onlineEl.classList.toggle('selected', mode === 'online');
    if (offlineEl) offlineEl.classList.toggle('selected', mode === 'offline');
  }
}

function selectModeModal(mode) {
  selectedMode = mode;
  const fmmEl = document.getElementById('modal-opt-fmm');
  const liveEl = document.getElementById('modal-opt-live');
  if (fmmEl) fmmEl.classList.toggle('selected', mode === 'FMM');
  if (liveEl) liveEl.classList.toggle('selected', mode === 'Live Editor');
  
  const mainFmmEl = document.getElementById('opt-fmm');
  const mainLiveEl = document.getElementById('opt-live');
  if (mainFmmEl) mainFmmEl.classList.toggle('selected', mode === 'FMM');
  if (mainLiveEl) mainLiveEl.classList.toggle('selected', mode === 'Live Editor');
  
  const errEl = document.getElementById('mode-error');
  if (errEl) errEl.style.display = 'none';
}
window.selectModeModal = selectModeModal;

async function submitMode() {
  if (!selectedMode) {
    const errEl = document.getElementById('mode-error');
    if (errEl) errEl.style.display = 'block';
    return;
  }
  document.getElementById('mode-warning-modal').classList.remove('active');
  await continueToVersion();
}


async function showErrorModalWithDiagnostics() {
  const setVal = (id, text, cls) => {
    const el = document.getElementById(id);
    if (!el) return;
    el.textContent = text || '—';
    el.classList.remove('ok', 'miss');
    if (cls) el.classList.add(cls);
  };

  setVal('err-path', exeDir || 'UNKNOWN');

  document.getElementById('error-modal').classList.add('active');

  if (invoke && exeDir) {
    try {
      const diag = await tauriInvoke('get_folder_diagnostics', { exeDir });
      setVal('err-path', diag.exe_dir);
    } catch (e) {
      console.error("diagnostics failed:", e);
    }
  }
}

async function continueToVersion() {
  if (!selectedMode) {
    document.getElementById('mode-warning-modal').classList.add('active');
    return;
  }

  // 1. Check Folder first!
  if (invoke) {
    try {
      const ok = await tauriInvoke('check_game_folder', { exeDir });
      if (!ok) {
        await showErrorModalWithDiagnostics();
        return; // Don't check for updates if folder is wrong
      }
    } catch (e) {
      console.error(e);
      await showErrorModalWithDiagnostics();
      return;
    }
  }

  // 2. Check for update only if folder is correct
  const update = await checkForUpdate();
  if (update && update.available) {
    showUpdateModal(update);
    return;
  }

  // 3. Save state and navigate
  try { sessionStorage.setItem('ta_mode', selectedMode); } catch (e) {}
  if (exeDir) {
    try { sessionStorage.setItem('ta_exe_dir', exeDir); } catch (e) {}
  }

  window.location.href = 'version_page_v2.html';
}

async function continueToVersionSkippingUpdate() {
  document.getElementById('update-modal').classList.remove('active');
  try { sessionStorage.setItem('ta_mode', selectedMode); } catch (e) {}
  if (exeDir) {
    try { sessionStorage.setItem('ta_exe_dir', exeDir); } catch (e) {}
  }

  window.location.href = 'version_page_v2.html';
}
window.continueToVersionSkippingUpdate = continueToVersionSkippingUpdate;

function closeModal(id) {
  document.getElementById(id).classList.remove('active');
}

function closeStartupModal() {
  document.getElementById('startup-modal').classList.remove('active');
  try { sessionStorage.setItem('ta_startup_seen', '1'); } catch (e) {}
}

window.onload = async () => {
  resumeTourIfActive();
  // Check if we need to auto-relocate after update
  if (invoke) {
    // Relocation logic removed to prevent auto-relocate loop on startup
  }

  // Tauri window close intercept for temp files
  if (TAURI) {
    TAURI.event.listen('show-exit-modal', () => {
      const exitModal = document.getElementById('exit-modal');
      if (exitModal) exitModal.classList.add('active');
    });
  }

  const btnKeepExit = document.getElementById('btn-keep-exit');
  if (btnKeepExit) {
    btnKeepExit.onclick = async () => {
      if (invoke) await tauriInvoke('exit_app');
    };
  }

  const btnCleanExit = document.getElementById('btn-clean-exit');
  if (btnCleanExit) {
    btnCleanExit.onclick = async () => {
      if (invoke) {
        await tauriInvoke('clean_temp_files');
        await tauriInvoke('exit_app');
      }
    };
  }

  // Welcome modal logic
  let seen = false;
  try {
    seen = sessionStorage.getItem('ta_startup_seen') === '1';
  } catch (e) {}

  if (!seen) {
    document.getElementById('startup-modal').classList.add('active');
  }

  // Fetch exe dir once
  if (invoke) {
    try {
      exeDir = await tauriInvoke('get_exe_dir');
      try { sessionStorage.setItem('ta_exe_dir', exeDir); } catch (e) {}
    } catch (e) {
      console.error("Failed to get exe dir:", e);
    }
    
    // Set version badge
    try {
      const version = await tauriInvoke('get_app_version');
      window.APP_VERSION = version; // Store for global use
      const badge = document.getElementById('version-badge');
      if (badge && version) {
        badge.innerHTML = `<i class="ti ti-info-circle"></i> V ${version}`;
      }
      const welcomeBadge = document.getElementById('welcome-version-badge');
      if (welcomeBadge && version) {
        welcomeBadge.innerText = version;
      }
    } catch (e) {
      console.error("Failed to set app version:", e);
    }
  }
};

// Global exports for inline HTML handlers
window.openExternal = openExternal;
window.selectMode = selectMode;
window.continueToVersion = continueToVersion;
window.closeModal = closeModal;
window.closeStartupModal = closeStartupModal;
window.doUpdate = doUpdate;
window.continueAfterUpdateCheck = continueAfterUpdateCheck;

window.startTourFromWelcome = () => {
  closeStartupModal();
  startTour();
};


// Ambient particles
const pcontainer=document.getElementById('particles');
const positions=[
  {left:'8%',delay:'0s',dur:'7.5s',c:'teal'},{left:'18%',delay:'1.2s',dur:'8.5s',c:'purple'},
  {left:'30%',delay:'2.4s',dur:'7.8s',c:'teal'},{left:'45%',delay:'0.6s',dur:'9s',c:'purple'},
  {left:'55%',delay:'3.1s',dur:'8s',c:'teal'},{left:'65%',delay:'1.8s',dur:'7.2s',c:'purple'},
  {left:'75%',delay:'2.9s',dur:'8.8s',c:'teal'},{left:'85%',delay:'0.3s',dur:'7.9s',c:'purple'},
  {left:'12%',delay:'4.2s',dur:'8.2s',c:'purple'},{left:'60%',delay:'4.8s',dur:'7.4s',c:'teal'},
  {left:'40%',delay:'5.4s',dur:'8.6s',c:'purple'},{left:'80%',delay:'3.6s',dur:'7.6s',c:'teal'}
];
positions.forEach((p)=>{ const el=document.createElement('div'); el.className='particle '+p.c; el.style.left=p.left; el.style.bottom='40px'; el.style.animationDelay=p.delay; el.style.animationDuration=p.dur; pcontainer.appendChild(el); });


window.takeTour = function() {
    // Take tour functionality to be added later
};

window.submitMode = submitMode;

async function handleFoundGameFolder(folderPath) {
  const statusEl = document.getElementById('locate-status');
  if (statusEl) {
    statusEl.style.display = 'block';
    statusEl.style.color = '#32cd32';
    statusEl.innerText = 'Found New Location. Relocating...';
  }
  
  const targetExe = folderPath + '\\TechnoAfandi-FC.exe';
  try {
    await tauriInvoke('copy_and_relaunch', { targetPath: targetExe });
  } catch (err) {
    if (statusEl) {
      statusEl.style.color = '#FF4D4D';
      statusEl.innerText = 'Failed to copy executable: ' + err;
    }
  }
}

async function selectGameFolder() {
  const statusEl = document.getElementById('locate-status');
  if (statusEl) statusEl.style.display = 'none';

  let selected = null;
  try {
    selected = await tauriInvoke('plugin:dialog|open', { options: { directory: true, multiple: false } });
  } catch (e) {
    console.error("Dialog error:", e);
    if (statusEl) {
      statusEl.style.display = 'block';
      statusEl.style.color = '#FF4D4D';
      statusEl.innerText = 'Dialog Error: ' + e;
    }
  }

  if (selected) {
    try {
      // In Tauri v2, dialog return might be an array or object depending on plugin version/options
      let folderPath = selected;
      if (typeof selected === 'object') {
        if (Array.isArray(selected)) folderPath = selected[0];
        else if (selected.path) folderPath = selected.path;
        else if (selected.filePaths) folderPath = selected.filePaths[0];
        else folderPath = JSON.stringify(selected);
      }

      const isValid = await tauriInvoke('check_game_folder', { exeDir: folderPath });
      if (isValid) {
        await handleFoundGameFolder(folderPath);
      } else {
        if (statusEl) {
          statusEl.style.display = 'block';
          statusEl.style.color = '#FF4D4D';
          statusEl.innerText = 'Specified files not found in: ' + folderPath;
        }
      }
    } catch (err) {
      console.error("Check folder error:", err);
      if (statusEl) {
        statusEl.style.display = 'block';
        statusEl.style.color = '#FF4D4D';
        statusEl.innerText = 'Check Error: ' + err;
      }
    }
  }
}
window.selectGameFolder = selectGameFolder;

async function autoLocateGame() {
  const statusEl = document.getElementById('locate-status');
  const btnText = document.getElementById('btn-auto-text');
  
  if (btnText) btnText.innerText = 'Searching...';
  if (statusEl) {
    statusEl.style.display = 'block';
    statusEl.style.color = 'var(--ghost)';
    statusEl.innerText = 'Scanning partitions...';
  }
  
  try {
    const foundPath = await tauriInvoke('auto_locate_game');
    if (foundPath) {
      await handleFoundGameFolder(foundPath);
    } else {
      if (btnText) btnText.innerText = 'Auto Locate';
      if (statusEl) {
        statusEl.style.display = 'block';
        statusEl.style.color = '#FF4D4D';
        statusEl.innerText = 'Game folder not found please manually relocate it';
      }
    }
  } catch (err) {
    if (btnText) btnText.innerText = 'Auto Locate';
    if (statusEl) {
      statusEl.style.display = 'block';
      statusEl.style.color = '#FF4D4D';
      statusEl.innerText = 'Error during search: ' + err;
    }
  }
}
window.autoLocateGame = autoLocateGame;

// Prevent forward history navigation from Home page
window.addEventListener('pageshow', () => {
  history.pushState(null, null, location.href);
});
window.addEventListener('popstate', () => {
  history.pushState(null, null, location.href);
});

// Also prevent default mouse back/forward button behavior on this page
window.addEventListener('mouseup', (e) => {
  if (e.button === 3 || e.button === 4) {
    e.preventDefault();
  }
});
window.addEventListener('mousedown', (e) => {
  if (e.button === 3 || e.button === 4) {
    e.preventDefault();
  }
});
