# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project overview

Quick Memo is a small memo/snippet picker built with Rust + Tauri 2. The Rust backend and Tauri configuration live in [src-tauri/](src-tauri/); the frontend is vanilla HTML/CSS/JS in [ui/](ui/) and is embedded into the executable at build time. It runs on Windows and Linux and stores an encrypted JSON database of groups → tags → content cards.

## Commands

- Install JS dependencies: `npm install`
- Run in development: `npm run dev`
- Build installers: `npm run build`
- Build a single executable without bundles: `npm run build -- --no-bundle`
- Rust tests: `cargo test --manifest-path src-tauri/Cargo.toml`
- Validate Rust only: `cargo check --manifest-path src-tauri/Cargo.toml`

On Windows, [scripts/tauri.mjs](scripts/tauri.mjs) loads the project-local `.toolchain` Rust installation and the Visual Studio MSVC environment before invoking the Tauri CLI.

Linux build dependencies:

```bash
sudo apt install build-essential curl wget file libssl-dev libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev libxdo-dev libdbus-1-dev \
  wl-clipboard xclip
```

## Architecture

- [ui/index.html](ui/index.html), [ui/styles.css](ui/styles.css), and [ui/app.js](ui/app.js) contain the whole Tauri UI. It uses `window.__TAURI__.core.invoke` (`withGlobalTauri` is enabled) and has no bundler.
- The UI is a three-pane hierarchy: sidebar tree (全部 → 分组 → 标签), content cards, and a right-side settings drawer. The drawer holds autostart, shortcut recording, TOTP, data password, lock, updater, and data-directory actions. Production builds disable the WebView context menu and intercept Ctrl+S/Ctrl+P/Ctrl+U.
- The default window is 920×620; keep the layout usable down to 760×520.
- [src-tauri/src/main.rs](src-tauri/src/main.rs) registers Tauri commands, manages the tray icon, registers the configurable global shortcut (default `Alt+B`), exposes the updater `check_update` / `install_update` commands, and keeps a `Mutex<Session>` in managed state.
- [src-tauri/src/storage.rs](src-tauri/src/storage.rs) owns the encrypted JSON database (`Database { groups: [Group { tags: [Tag { entries: [Entry { title, lines }] }] }] }`), CRUD commands, config persistence, encryption mode state, unlock policy, and legacy Markdown migration (`#` → tag, `##` → entry title, all tags into `默认分组`).
- [src-tauri/src/crypto.rs](src-tauri/src/crypto.rs) implements the `QMEMO4` encrypted file format: AES-256-GCM data encryption, an embedded app secret plus per-file salt for basic mode (portable across devices running this app), and Argon2id password + per-file salt for password mode. It can parse/migrate legacy `QMEMO3` files.
- [src-tauri/src/keystore.rs](src-tauri/src/keystore.rs) stores the password-mode key cache and TOTP secret in app-owned local files outside the data directory. Both are encrypted at rest with a device-bound key derived from the embedded app secret plus a per-file salt, and old plaintext caches are migrated on read. This is basic obfuscation, not protection against same-user malware; it does not use OS credential managers or system-level encryption.
- [src-tauri/src/totp.rs](src-tauri/src/totp.rs) generates TOTP secrets/QR codes and verifies 6-digit codes for the optional app lock.
- [src-tauri/src/platform.rs](src-tauri/src/platform.rs) handles platform data/config directories, autostart registration, opening folders, and clipboard fallbacks.
- Windows stores data in `%APPDATA%\quickmemo`; Linux uses `~/.local/share/quickmemo`. `QUICKMEMO_DIR` overrides the location. Tauri config lives in the data directory as `config.json`.
- [scripts/tauri.mjs](scripts/tauri.mjs) wraps the Tauri CLI so Windows builds can load a project-local Rust toolchain and MSVC environment.
- Plaintext `memos.md` is migrated to `memos.enc` and deleted on first run. Deleting/updating memos re-encrypts the whole file.
- Basic mode (no data password) is portable: any install of this app can open `memos.enc`; the embedded app secret is extractable, so this is light obfuscation, not strong secrecy.
- Password mode is not portable without the password: a new device has no cache and must verify the data password on first open. The cache is per data directory and outside the data directory.
- The app lock is TOTP-based and independent of file encryption. After `totp_ttl_days` (default 7) without launching, or after a manual lock, the UI asks for a TOTP code. If the data password is enabled and TOTP is disabled, the data password is re-requested instead.
- Password-mode keys are cached per data directory so normal launches do not prompt. A data password is requested when that cache is missing (new device) or the configured inactivity period has elapsed while it is the active lock.

## Development notes

- The app uses an encrypted JSON database, not Markdown. Keep the JSON schema versioned and preserve unknown data where practical.
- The parser migrates legacy Markdown on first read; do not reintroduce line-range editing.
- `--hidden` is used for hidden autostart behavior. Manual launches should show the window.
- Do not introduce OS keychain/keyring dependencies for basic encryption; the user explicitly wants app-level encryption, not system-level encryption.
- Basic mode is intentionally portable across devices but only readable by this app. Do not change it back to a device-bound random key.
- Global shortcuts are registered by `tauri-plugin-global-shortcut`. The settings recorder temporarily suspends the old shortcut, then restores/registers the new one. Invalid or conflicting shortcuts must surface an error and keep the old binding.
- The tray icon is created in `setup_tray`; the window close button hides to the tray when the tray exists, while `Esc`/the global shortcut/the tray toggle hide it. Exit only through the tray menu.
- Auto-update uses `tauri-plugin-updater` against GitHub Releases. `tauri.conf.json` holds `plugins.updater.pubkey` and the `releases/latest/download/latest.json` endpoint; replace the `REPLACE_WITH_...` and `OWNER/REPO` placeholders before releasing. `src-tauri/tauri.release.conf.json` enables updater artifacts only in CI; the private key lives in the `TAURI_SIGNING_PRIVATE_KEY` secret and must never be committed. Only installer-based builds auto-update; the first updater-enabled release must be installed manually.
- Releases are built by [.github/workflows/release.yml](.github/workflows/release.yml) on `v*` tags. Keep `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml` versions in sync with the tag.
