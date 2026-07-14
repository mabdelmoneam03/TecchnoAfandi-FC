/**
 * TechnoAfandi FC — Interactive Tour System
 * Runs a guided spotlight tour across home and version pages.
 */

// ───────────────────────── helpers ─────────────────────────
const SS_KEY = 'ta_tour_step';

function setStep(n) { try { sessionStorage.setItem(SS_KEY, n); } catch (_) {} }
function getStep()  { try { return parseInt(sessionStorage.getItem(SS_KEY) ?? '-1'); } catch (_) { return -1; } }
function clearTour(){ try { sessionStorage.removeItem(SS_KEY); } catch (_) {} }

// ───────────────────────── spotlight UI ─────────────────────
let overlay, spotlight, tooltip;

function buildOverlay() {
  if (document.getElementById('ta-tour-overlay')) return;

  overlay = document.createElement('div');
  overlay.id = 'ta-tour-overlay';
  Object.assign(overlay.style, {
    position: 'fixed', inset: '0', zIndex: '9999',
    pointerEvents: 'none',
  });

  // SVG spotlight mask
  spotlight = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  spotlight.setAttribute('id', 'ta-tour-spotlight');
  Object.assign(spotlight.style, {
    position: 'absolute', inset: '0', width: '100%', height: '100%',
    pointerEvents: 'none',
  });
  spotlight.innerHTML = `
    <defs>
      <mask id="ta-hole-mask">
        <rect width="100%" height="100%" fill="white"/>
        <rect id="ta-hole" rx="12" ry="12" fill="black"/>
      </mask>
    </defs>
    <rect width="100%" height="100%" fill="rgba(0,0,0,0.72)" mask="url(#ta-hole-mask)"/>
    <rect id="ta-hole-border" rx="12" ry="12" fill="none" stroke="#7CFFE1" stroke-width="2" opacity="0.85"/>
  `;

  // Tooltip
  tooltip = document.createElement('div');
  tooltip.id = 'ta-tour-tooltip';
  Object.assign(tooltip.style, {
    position: 'absolute',
    background: 'linear-gradient(180deg,rgba(18,28,26,0.96) 0%,rgba(10,12,11,0.98) 100%)',
    border: '1.5px solid rgba(124,255,225,0.45)',
    borderRadius: '14px',
    padding: '16px 18px 14px',
    maxWidth: '320px',
    minWidth: '220px',
    boxShadow: '0 0 40px rgba(124,255,225,0.2), 0 8px 32px rgba(0,0,0,0.6)',
    fontFamily: "'Inter', sans-serif",
    color: '#F5F5F7',
    pointerEvents: 'all',
    transition: 'opacity 0.3s ease, transform 0.3s ease',
    opacity: '0',
    transform: 'translateY(8px)',
  });

  overlay.appendChild(spotlight);
  overlay.appendChild(tooltip);
  document.body.appendChild(overlay);
}

function showSpotlight(el, pad = 10) {
  const r = el.getBoundingClientRect();
  const hole   = document.getElementById('ta-hole');
  const border = document.getElementById('ta-hole-border');
  const attrs = {
    x: r.left - pad, y: r.top - pad,
    width: r.width + pad * 2, height: r.height + pad * 2,
  };
  ['x','y','width','height'].forEach(k => {
    hole.setAttribute(k, attrs[k]);
    border.setAttribute(k, attrs[k]);
  });

  // Position tooltip
  const tt = tooltip;
  const ttW = 280, ttH = 120;
  let top = r.bottom + pad + 14;
  let left = r.left + r.width / 2 - ttW / 2;

  if (top + ttH > window.innerHeight - 20) top = r.top - ttH - pad - 14;
  if (left < 10) left = 10;
  if (left + ttW > window.innerWidth - 10) left = window.innerWidth - ttW - 10;

  tt.style.top  = top  + 'px';
  tt.style.left = left + 'px';
}

function showTooltip({ ar, en, step, total, onNext, onSkip, nextLabel = 'Continue →', color = '#7CFFE1' }) {
  tooltip.style.opacity = '0';
  tooltip.style.transform = 'translateY(8px)';

  tooltip.innerHTML = `
    <div style="display:flex;align-items:center;gap:6px;margin-bottom:10px">
      <span style="width:6px;height:6px;border-radius:50%;background:${color};box-shadow:0 0 8px ${color};flex-shrink:0;display:inline-block"></span>
      <span style="font-family:'JetBrains Mono',monospace;font-size:10px;letter-spacing:0.14em;color:${color};text-transform:uppercase">TOUR · ${step}/${total}</span>
    </div>
    <p style="font-size:13px;line-height:1.65;margin-bottom:6px;direction:rtl;color:#F5F5F7">${ar}</p>
    <p style="font-size:10.5px;line-height:1.5;margin-bottom:14px;color:#A6A4B8;font-style:italic;direction:ltr">${en}</p>
    <div style="display:flex;gap:8px">
      <button id="ta-tour-next" style="flex:1;padding:9px 12px;border-radius:9px;border:none;cursor:pointer;font-family:'Inter',sans-serif;font-size:12.5px;font-weight:600;background:linear-gradient(90deg,${color},#9B6EF3);color:#03100c;box-shadow:0 4px 14px rgba(124,255,225,0.3)">${nextLabel}</button>
      <button id="ta-tour-skip" style="padding:9px 12px;border-radius:9px;border:1px solid rgba(255,255,255,0.1);cursor:pointer;font-family:'Inter',sans-serif;font-size:12px;background:rgba(255,255,255,0.04);color:#605E70">Skip</button>
    </div>
  `;

  document.getElementById('ta-tour-next').onclick = onNext;
  document.getElementById('ta-tour-skip').onclick = () => { endTour(); if (onSkip) onSkip(); };

  requestAnimationFrame(() => {
    tooltip.style.opacity = '1';
    tooltip.style.transform = 'translateY(0)';
  });
}

function endTour() {
  clearTour();
  if (overlay) overlay.remove();
  overlay = spotlight = tooltip = null;
}

// ───────────────────────── scroll to element ────────────────
function scrollIntoViewAndWait(el) {
  return new Promise(res => {
    el.scrollIntoView({ behavior: 'smooth', block: 'center' });
    setTimeout(res, 450);
  });
}

// ───────────────────────── Home Page Tour Steps ─────────────
const HOME_STEPS = [
  // Step 1 – overview of the home page panel
  async function step1() {
    const panel = document.querySelector('.app-panel');
    await scrollIntoViewAndWait(panel);
    showSpotlight(panel, 0);
    showTooltip({
      ar: 'مرحباً! هذه هي الصفحة الرئيسية للأداة — من هنا بتتحكم في كل حاجة 🎮',
      en: 'Welcome! This is the main home screen of the tool.',
      step: 1, total: TOTAL_HOME_STEPS,
      color: '#7CFFE1',
      onNext: () => runHomeStep(2),
    });
  },

  // Step 2 – mode options
  async function step2() {
    const modes = document.querySelector('.modes');
    await scrollIntoViewAndWait(modes);
    showSpotlight(modes, 6);
    showTooltip({
      ar: 'هنا بتختار الوضع اللي هتشغل بيه اللعبة — FMM أو Live Editor. لازم تختار واحد قبل ما تكمل.',
      en: 'Choose your activation mode here — FMM or Live Editor. One must be selected to proceed.',
      step: 2, total: TOTAL_HOME_STEPS,
      color: '#7CFFE1',
      onNext: () => runHomeStep(3),
    });
  },

  // Step 3 – continue button
  async function step3() {
    const btn = document.querySelector('.btn-start');
    await scrollIntoViewAndWait(btn);
    showSpotlight(btn, 6);
    showTooltip({
      ar: 'لما تختار الوضع، اضغط على الزر ده وهيوديك لصفحة الإصدار للتحقق من إصدار لعبتك.',
      en: 'After selecting a mode, press this button to go to the version review page.',
      step: 3, total: TOTAL_HOME_STEPS,
      color: '#7CFFE1',
      onNext: () => runHomeStep(4),
    });
  },

  // Step 4 – link buttons
  async function step4() {
    const links = document.querySelector('.links');
    await scrollIntoViewAndWait(links);
    showSpotlight(links, 6);
    showTooltip({
      ar: '📎 روابط السيرفر والمجتمع — فيها Discord وYouTube والسوشيال ميديا وصفحة FC 26.',
      en: 'Quick links to the TechnoAfandi community: Discord, YouTube, Social & FC 26 page.',
      step: 4, total: TOTAL_HOME_STEPS,
      color: '#FFD54A',
      onNext: () => runHomeStep(5),
    });
  },

  // Step 5 – requirements notice, then navigate to version page
  async function step5() {
    const panel = document.querySelector('.app-panel');
    showSpotlight(panel, 0);
    showTooltip({
      ar: '⚠️ متطلبات ضرورية: ①  اتصال بالإنترنت أثناء التشغيل. ② الأداة لازم تكون موجودة جوه فولدر اللعبة.',
      en: '⚠️ Requirements: ① Active internet connection. ② Tool must be inside the game folder.',
      step: 5, total: TOTAL_HOME_STEPS,
      color: '#FFD54A',
      nextLabel: 'Next Page →',
      onNext: () => {
        // Select FMM for the tour so version page loads
        try { sessionStorage.setItem('ta_mode', 'FMM'); sessionStorage.setItem('ta_exe_dir', '.'); } catch(_) {}
        setStep(10); // jump to version page steps
        window.location.href = 'version_page_v2.html';
      },
    });
  },
];
const TOTAL_HOME_STEPS = HOME_STEPS.length;

function runHomeStep(n) {
  setStep(n);
  HOME_STEPS[n - 1]?.();
}

// ───────────────────────── Version Page Tour Steps ──────────
const VERSION_STEPS = [
  // Step 10 – version card
  async function step10() {
    const card = document.querySelector('.version-card');
    if (!card) { runVersionStep(11); return; }
    await scrollIntoViewAndWait(card);
    showSpotlight(card, 8);
    showTooltip({
      ar: 'دي بطاقة إصدار اللعبة — بتعرضلك الإصدار اللي عندك حالياً بشكل تلقائي.',
      en: 'This card shows the current version of your FC 26 installation, detected automatically.',
      step: 6, total: TOTAL_VERSION_STEPS + TOTAL_HOME_STEPS,
      color: '#7CFFE1',
      onNext: () => runVersionStep(11),
    });
  },

  // Step 11 – start activation button
  async function step11() {
    const btn = document.getElementById('start-btn');
    await scrollIntoViewAndWait(btn);
    showSpotlight(btn, 6);
    showTooltip({
      ar: '🚀 زر START ACTIVATION — اضغط عليه لبدء عملية تنزيل وتفعيل ملفات اللعبة.',
      en: 'Press START ACTIVATION to begin downloading & activating game files.',
      step: 7, total: TOTAL_VERSION_STEPS + TOTAL_HOME_STEPS,
      color: '#7CFFE1',
      nextLabel: 'Simulate →',
      onNext: () => runVersionStep(12),
    });
  },

  // Step 12 – fake activation demo
  async function step12() {
    const container = document.getElementById('progress-container');
    if (!container) { runVersionStep(13); return; }
    container.style.display = 'block';
    await scrollIntoViewAndWait(container);
    showSpotlight(container, 8);

    // Show the tooltip first
    showTooltip({
      ar: '📥 هنا بيبان تقدم عملية التحميل والتفعيل — مع نسبة مئوية وحالة مباشرة.',
      en: 'The progress console shows real-time download status, percentage, and activation steps.',
      step: 8, total: TOTAL_VERSION_STEPS + TOTAL_HOME_STEPS,
      color: '#7CFFE1',
      nextLabel: 'Watch demo →',
      onNext: () => runFakeActivation(container, () => runVersionStep(13)),
    });
  },

  // Step 13 – pause / cancel buttons
  async function step13() {
    const row = document.querySelector('.action-row');
    await scrollIntoViewAndWait(row);
    showSpotlight(row, 6);
    showTooltip({
      ar: '⏸ PAUSE — بتوقف التحميل مؤقتاً. ✖ CANCEL — بتلغي العملية بالكامل.',
      en: 'PAUSE temporarily halts the download. CANCEL stops the process entirely.',
      step: 9, total: TOTAL_VERSION_STEPS + TOTAL_HOME_STEPS,
      color: '#A855F7',
      onNext: () => runVersionStep(14),
    });
  },

  // Step 14 – link buttons on version page
  async function step14() {
    const links = document.querySelector('.links');
    await scrollIntoViewAndWait(links);
    showSpotlight(links, 6);
    showTooltip({
      ar: '🔗 نفس اللينكات من الصفحة الرئيسية — موجودة هنا برضو للوصول السريع أثناء التفعيل.',
      en: 'Same community links are available here too for quick access during activation.',
      step: 10, total: TOTAL_VERSION_STEPS + TOTAL_HOME_STEPS,
      color: '#FFD54A',
      nextLabel: 'Finish Tour ✓',
      onNext: () => {
        endTour();
        // Show a brief "tour done" flash
        showTourDone();
      },
    });
  },
];
const TOTAL_VERSION_STEPS = VERSION_STEPS.length;

function runVersionStep(n) {
  setStep(n);
  const idx = n - 10;
  VERSION_STEPS[idx]?.();
}

// ───────────────────────── Fake Activation Demo ─────────────
async function runFakeActivation(container, onDone) {
  const fill = document.getElementById('progress-fill');
  const pct  = document.getElementById('progress-pct');
  const txt  = document.getElementById('progress-text-content');
  if (!fill || !pct || !txt) { onDone(); return; }

  // Disable next button during demo
  const nextBtn = document.getElementById('ta-tour-next');
  if (nextBtn) nextBtn.disabled = true;

  const stages = [
    { p:  5, msg: 'Connecting to server...' },
    { p: 15, msg: 'Authenticating session...' },
    { p: 28, msg: 'Fetching patch manifest...' },
    { p: 42, msg: 'Downloading patch files...' },
    { p: 58, msg: 'Downloading patch files...' },
    { p: 72, msg: 'Verifying integrity...' },
    { p: 85, msg: 'Applying activation patch...' },
    { p: 95, msg: 'Writing configuration...' },
    { p:100, msg: 'Done! ✓' },
  ];

  for (const s of stages) {
    fill.style.width = s.p + '%';
    fill.style.transition = 'width 0.5s ease';
    pct.textContent = s.p + '%';
    txt.textContent = s.msg;
    await new Promise(r => setTimeout(r, 520));
  }

  await new Promise(r => setTimeout(r, 600));
  fill.style.width = '0%';
  pct.textContent  = '0%';
  txt.textContent  = 'Initializing';

  if (nextBtn) nextBtn.disabled = false;
  onDone();
}

// ───────────────────────── Tour Done Banner ─────────────────
function showTourDone() {
  const banner = document.createElement('div');
  Object.assign(banner.style, {
    position: 'fixed', bottom: '28px', left: '50%', transform: 'translateX(-50%) translateY(60px)',
    background: 'linear-gradient(90deg,#7CFFE1,#A855F7)',
    borderRadius: '50px', padding: '12px 24px',
    fontFamily: "'Inter',sans-serif", fontWeight: '700', fontSize: '13px', color: '#04140f',
    boxShadow: '0 4px 28px rgba(124,255,225,0.5)', zIndex: '99999',
    transition: 'transform 0.4s cubic-bezier(.34,1.56,.64,1)',
    whiteSpace: 'nowrap',
  });
  banner.textContent = '✓ Tour Complete! You\'re all set 🎮';
  document.body.appendChild(banner);
  requestAnimationFrame(() => {
    banner.style.transform = 'translateX(-50%) translateY(0)';
  });
  setTimeout(() => {
    banner.style.transform = 'translateX(-50%) translateY(80px)';
    banner.style.opacity = '0';
    setTimeout(() => banner.remove(), 400);
  }, 3500);
}

// ───────────────────────── Public API ───────────────────────
export function startTour() {
  buildOverlay();
  setStep(1);
  runHomeStep(1);
}

export function resumeTourIfActive() {
  const step = getStep();
  if (step < 1) return false;

  buildOverlay();

  if (step >= 10) {
    // We're on the version page
    setTimeout(() => runVersionStep(step < 10 ? 10 : step >= 14 ? 14 : step), 600);
  } else {
    setTimeout(() => runHomeStep(step), 600);
  }
  return true;
}
