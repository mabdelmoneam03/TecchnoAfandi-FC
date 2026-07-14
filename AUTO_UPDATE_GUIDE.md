# 🔄 TechnoAfandi - FC | Auto-Update System
## Full Guide: من `git push` لحد ما اليوزر ياخد آخر نسخة أوتوماتيك

---

## 📋 الخطة الكاملة

```
Developer يعمل تعديل
        ↓
    git push + git tag v1.1.0
        ↓
    GitHub Actions بيبني الـ .exe تلقائي
        ↓
    بيوقّع الملف بتوقيع رقمي
        ↓
    بيرفعه على GitHub Releases + latest.json
        ↓
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        ↓
    المستخدم يفتح الأداة
        ↓
    يدوس CONTINUE
        ↓
    الأداة بتتشيك على GitHub Releases
        ↓
    لقى إصدار أحدث؟
        │
   ┌────┴────┐
   لأ        آه
   ↓          ↓
  يكمل    ┌──────────────────────────┐
  عادي    │  TechnoAfandi - FC | UPDATE  │
          │  📢                          │
          │  ┌────────────────────┐     │
          │  │ رسالة التحديث      │     │
          │  │ (أنت بتكتبها)      │     │
          │  └────────────────────┘     │
          │                             │
          │  [ UPDATE ]  [ NEGLECT ]    │
          └──────────────────────────────┘
               ↓              ↓
           بيحمل          يكمل عادي
           ويثبت
           ويعمل restart
```

---

## 🛠️ خطوات التنفيذ

### الخطوة 1: توليد مفتاح التوقيع (مرة واحدة بس)

```cmd
cd C:\Users\D A W L Y\Downloads\techno-afandi-tool
npx @tauri-apps/cli signer generate -w ~/.tauri/myapp.key
```

ده هيسألك على password. هيولّد ملفين:
- `~/.tauri/myapp.key` ← المفتاح الخاص (سري — متشاركوش!)
- `~/.tauri/myapp.key.pub` ← المفتاح العام (ده اللي بيتحط في tauri.conf.json)

**احفظ المفتاح العام** — هتحتاجه في الخطوة 3.

### الخطوة 2: تثبيت الـ Plugin

```cmd
cd techno-afandi-tool
npm run tauri add updater
```

أو يدوي:
```toml
# Cargo.toml — أضف في [dependencies]
tauri-plugin-updater = "2"
```

### الخطوة 3: تعديل tauri.conf.json

أضف الـ `plugins` section:

```json
{
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/mabdelmoneam03/EA-SPORTS-FC-26/releases/latest/download/latest.json"
      ],
      "pubkey": "PASTE_YOUR_PUBLIC_KEY_HERE"
    }
  }
}
```

**ملاحظة:** استبدل `PASTE_YOUR_PUBLIC_KEY_HERE` بالمفتاح العام من الخطوة 1.

### الخطوة 4: تسجيل الـ Plugin في main.rs

```rust
fn main() {
    let activator = include_bytes!("../assets/activator.exe");

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())  // ← أضف ده
        .manage(activator.to_vec())
        // ... باقي الكود
}
```

### الخطوة 5: إضافة Permission في capabilities/default.json

```json
{
  "permissions": [
    "core:default",
    "core:event:default",
    "core:window:default",
    "core:webview:default",
    "updater:default"
  ]
}
```

### الخطوة 6: إضافة Update Check في الـ Frontend

في `home_page_v2.html`، لما اليوزر يدوس CONTINUE:

```javascript
async function checkForUpdate() {
  if (!TAURI) return null;
  
  try {
    const { check } = await import('@tauri-apps/plugin-updater');
    const update = await check();
    return update; // null = no update, object = update available
  } catch (e) {
    console.error('Update check failed:', e);
    return null;
  }
}

async function continueToVersion() {
  // ... mode selection check ...
  
  // Check for updates before continuing
  const update = await checkForUpdate();
  if (update) {
    showUpdateModal(update);
    return;
  }
  
  // No update — continue normally
  // ... existing code ...
}
```

### الخطوة 7: إنشاء GitHub Actions Workflow

اعمل ملف `.github/workflows/release.yml`:

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  release:
    permissions:
      contents: write
    strategy:
      matrix:
        platform: [windows-latest]
    runs-on: ${{ matrix.platform }}

    steps:
      - uses: actions/checkout@v4

      - name: Setup Node
        uses: actions/setup-node@v4
        with:
          node-version: 20

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install dependencies
        run: npm install

      - name: Build and Release
        uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: 'TechnoAfandi - FC v__VERSION__'
          releaseBody: 'See the assets to download and install this version.'
          releaseDraft: false
          prerelease: false
```

### الخطوة 8: إضافة Secrets في GitHub

روح لـ GitHub Repo → Settings → Secrets and variables → Actions:

1. `TAURI_SIGNING_PRIVATE_KEY` ← محتوى ملف `~/.tauri/myapp.key`
2. `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` ← الباسورد اللي كتبته في الخطوة 1

### الخطوة 9: عمل Release

```bash
# غيّر الإصدار في tauri.conf.json و Cargo.toml
# مثلاً من "1.0.0" لـ "1.1.0"

git add .
git commit -m "v1.1.0 - update description"
git tag v1.1.0
git push origin main --tags
```

GitHub Actions هيبني الـ .exe ويوقعه ويرفعه تلقائياً!

---

## 📱 شكل الـ Update Modal

```
┌──────────────────────────────────┐
│  TechnoAfandi - FC | UPDATE  📢  │
│  ──────────────────────────      │
│                                  │
│  ┌────────────────────────────┐  │
│  │  تم إصدار نسخة جديدة       │  │
│  │  v1.1.0                     │  │
│  │                             │  │
│  │  [رسالة التحديث اللي إنت   │  │
│  │   كاتبها في الـ Release]    │  │
│  └────────────────────────────┘  │
│                                  │
│  [ UPDATE ]     [ NEGLECT ]      │
└──────────────────────────────────┘
```

---

## 🔐 الأمان

- كل update مُوقّع بـ **EdDSA signature** (مش SHA256 عادي)
- المفتاح الخاص **مش موجود في الكود** — بس في GitHub Secrets
- المفتاح العام موجود في `tauri.conf.json` — بيستخدمه اليوزر للتحقق
- لو حد عدّل الملف في الـ Release، التوقيع مش هيمشي والتحديث هيتمنع

---

## 📁 الملفات اللي محتاجة تتعدل

| الملف | التعديل |
|-------|---------|
| `Cargo.toml` | إضافة `tauri-plugin-updater` |
| `tauri.conf.json` | إضافة `plugins.updater` |
| `main.rs` | تسجيل الـ plugin |
| `capabilities/default.json` | إضافة `updater:default` |
| `home_page_v2.html` | إضافة update check + modal |
| `.github/workflows/release.yml` | Workflow جديد |

---

## 🚀 الـ Flow بعد الإعداد

```
أول مرة:
  Developer: git tag v1.0.0 → push
  GitHub Actions: build → sign → upload to Releases
  User: ينزل .exe من Releases → يشغل

لما فيه تحديث:
  Developer: يعدل الكود → git tag v1.1.0 → push
  GitHub Actions: build → sign → upload + latest.json
  User: يفتح الأداة → CONTINUE → popup "تحديث متاح!" → UPDATE → restart
```

---

## ⚠️ ملاحظات مهمة

1. **أول release لازم يكون يدوي** — عشان الـ latest.json يتكون
2. **الإصدار في tauri.conf.json لازم يتغير** مع كل release
3. **الإصدار في Cargo.toml لازم يتغير** برضو (نفس الرقم)
4. **المفتاح الخاص متضيعوش** — لو ضاع مش هتقدر توقع تحديثات جديدة
5. **الـ GitHub repo لازم يكون public** عشان الـ Releases تكون متاحة
