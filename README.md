# Quick Memo / 备忘快贴

Quick Memo 是一个轻量的备忘片段工具，用「分组 → 标签 → 内容卡片」管理命令、代码片段、常用文本和小段备忘。数据保存在加密文件里，不依赖明文 Markdown。

交互目标是替代 uTools「备忘快贴」里最常用的片段检索和复制能力。

## Windows / Linux

源码在 [ui/](ui/) 和 [src-tauri/](src-tauri/)，前端是原生 HTML/CSS/JS，不需要前端构建工具。

### 开发环境

- Node.js 18 及以上（只用于 Tauri CLI）
- Rust stable
- Windows：WebView2 运行时（Windows 10/11 通常自带）、Visual Studio Build Tools（C++ 生成工具）
- Debian：

```bash
sudo apt install build-essential curl wget file libssl-dev libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev libxdo-dev libdbus-1-dev
```

### 运行与构建

```bash
npm install
npm run dev      # 开发模式
npm run build    # 构建安装包
```

Windows 上 `npm run dev` 会自动加载项目内 `.toolchain` 的 Rust 工具链和 Visual Studio 编译环境（如果存在）。

构建产物在 `src-tauri/target/release/bundle/`。

### 发布与更新

仓库带有 [.github/workflows/release.yml](.github/workflows/release.yml)，推送版本标签后由 GitHub Actions 自动打包并创建草稿 Release：

1. 同步修改三处版本号：`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`。
2. 提交后打标签并推送：`git tag v0.3.1 && git push origin v0.3.1`。
3. Actions 会构建 Windows（`setup.exe` / `.msi`）和 Linux（`.deb` / `.AppImage`），上传到草稿 Release。
4. 检查产物无误后，在 GitHub 上发布该 Release。

已安装的用户直接下载新安装包覆盖安装即可，数据保存在 `%APPDATA%\quickmemo` 或 `~/.local/share/quickmemo`，升级不会清除；免安装版替换 `quickmemo.exe` 即可。生产机器不需要拉取仓库或本地编译。

#### 应用内自动更新

应用启动后会静默检查 GitHub Release，发现新版本时弹窗确认，下载安装后自动重启。首次配置：

1. 生成更新签名密钥（私钥不要提交到仓库）：

   ```bash
   npm run tauri -- signer generate -w ~/.tauri/quickmemo.key
   ```

2. 把输出的公钥填入 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`。
3. 把 `plugins.updater.endpoints` 里的 `OWNER/REPO` 替换为实际仓库，例如 `alice/quickmemo`。
4. 在仓库 Secrets 中添加 `TAURI_SIGNING_PRIVATE_KEY`，内容为 `quickmemo.key` 文件的完整文本；生成时如果设置了密码，再添加 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。

注意：

- 只有通过安装包安装的版本才能自动更新，免安装版需要手动替换 `quickmemo.exe`。
- 第一个包含自动更新的版本仍需手动安装一次，之后发布的版本才能应用内升级。
- 私钥丢失后，已安装用户无法验证后续更新，只能手动重新安装。
- CI 通过 `src-tauri/tauri.release.conf.json` 生成更新签名和 `latest.json`，本地普通构建不受影响。

### Debian 运行说明

可以直接在 Debian 13 / Ubuntu 24.04 及以上构建运行（WebKitGTK 4.1）。Windows 的 exe 不能直接在 Linux 运行，需要在 Debian 上构建：

```bash
sudo apt install build-essential curl wget file libssl-dev libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev libxdo-dev libdbus-1-dev \
  wl-clipboard xclip gnome-shell-extension-appindicator
npm install
npm run build -- --no-bundle
./src-tauri/target/release/quickmemo
```

- Debian 12 默认使用 WebKitGTK 4.0；需要 backports，或按 Tauri 的 WebKitGTK 4.0 构建方式调整。
- 复制文本优先使用 WebView 剪贴板，失败时回退 `wl-copy` / `xclip`。
- **开机启动**：在设置里开启后写入 `~/.config/autostart/quickmemo.desktop`，`Exec` 指向当前运行的可执行文件。请用 release 二进制开启，不要用 `npm run dev`，否则自启会指向 `target/debug`；移动二进制后需要重新开关一次。
- **托盘**：GNOME 需要 AppIndicator 扩展。安装 `gnome-shell-extension-appindicator` 并在“扩展”里启用；KDE / XFCE 通常原生支持。托盘创建失败不会阻止程序启动。
- **快捷键**：X11 下程序内 `Alt+B` 可用；Wayland 下全局快捷键可能不可靠，设置里会显示提示。可在 GNOME「设置 → 键盘 → 自定义快捷键」中绑定：
  - 名称：备忘快贴
  - 命令：`/绝对路径/quickmemo --toggle`
  - 快捷键：`Alt+B`

  `--toggle` 会切换已运行实例的显示/隐藏；程序未运行时启动并显示。

### 数据位置

- Windows：`%APPDATA%\quickmemo\memos.enc`
- Linux：`~/.local/share/quickmemo/memos.enc`
- 设置 `QUICKMEMO_DIR` 环境变量可以改用指定目录，例如网盘或同步目录。

配置保存在同一目录下的 `config.json`。基础模式的密钥内置在软件中，不需要额外密钥文件；数据密码模式的解密密钥缓存和应用锁密钥保存在数据目录之外的本地文件（`%LOCALAPPDATA%\quickmemo` 或 `~/.config/quickmemo`），不使用 Windows 凭据管理器、GNOME Keyring、BitLocker 等系统级加密。

### 文件加密

Quick Memo 始终使用软件自己的加密，不会在磁盘上保留明文：

- 文件使用 AES-256-GCM 加密，格式为 `memos.enc`。
- 基础模式：软件内置密钥 + 每个文件自己的随机盐派生密钥。把 `memos.enc` 拷到另一台设备，只要用本软件就能直接打开。
- 设置数据密码后：密钥由数据密码 + 文件随机盐派生（Argon2id）。换设备第一次打开必须校验数据密码。
- 首次启动会把旧的 `memos.md` 自动迁移为 `memos.enc` 并删除明文。
- 应用内编辑、删除、复制都直接操作加密文件。
- 基础模式是“软件级轻加密”：能防止其他工具直接查看，但不能抵御逆向软件提取内置密钥；需要更强保护请设置数据密码。

### 数据结构

加密文件里的明文是版本化 JSON，不再直接保存 Markdown：

```text
全部
└── 分组（默认分组、工作、生活……）
    └── 标签（Git、部署、快捷键……）
        └── 内容卡片（标题 + 多行正文）
```

- 左侧树按「全部 → 分组 → 标签」浏览。
- 分组和标签都可以新建、重命名、删除；标签可以移动到其他分组。
- 标签下是内容卡片，支持新增、编辑、删除、点行复制、复制整条。
- 旧版 Markdown 会自动迁移：`#` 大标题变成标签，`##` 小标题变成内容卡片标题，所有标签放进「默认分组」。

### 数据密码（可选）

设置里的「数据密码」可以在软件加密之外再加一层密码：

- 至少 6 位；密钥用 Argon2id + 随机盐从密码派生。
- 新设备第一次打开会要求输入数据密码；输入正确后在该设备缓存，平时自动打开。
- 在原设备设置密码后不会立刻重复询问；在设置里点「立即锁定」可以马上验证。
- 如果未启用应用锁，超过设定天数未启动程序也会要求输入数据密码。
- 数据密码忘记后无法恢复。
- 6 位数字只能防止随手查看，不能抵御离线暴力破解；如果文件会公开，建议使用更长的密码。

### 应用锁（TOTP）

设置里的「应用锁」使用 Google Authenticator、Microsoft Authenticator 等动态验证码：

- 开启时显示二维码和密钥，扫码绑定后输入一次 6 位验证码确认。
- 应用锁只负责进入软件，和文件加密相互独立。
- 正常启动不需要验证码；超过设定天数（默认 7 天，可改为 1/3/7/30 天）未使用才要求输入验证码。
- 手动点设置里的「立即锁定」会立即要求重新验证。
- TOTP 密钥保存在软件自己的本地密钥文件中；密钥丢失可以重新绑定，但不会影响文件解密。

### 设置、快捷键与托盘

顶部搜索框右侧的齿轮打开设置抽屉：

- 常规：开机启动、启动后隐藏、呼出/隐藏快捷键。
- 安全：应用锁、重验周期、数据密码、立即锁定。
- 数据：打开数据目录、查看加密说明。

全局快捷键：

- 默认 `Alt+B`，在任意应用里都可以呼出或隐藏窗口。
- 在设置里点快捷键按钮后按下新的组合键即可修改，支持字母、数字和 F1-F24，建议带 Ctrl/Alt/Shift。
- 如果快捷键注册失败，设置里会显示原因；Linux Wayland 建议改用系统自定义快捷键执行 `quickmemo --toggle`。
- 托盘图标提供「显示/隐藏」和「退出」；隐藏窗口后程序继续在托盘运行。GNOME 需要启用 AppIndicator 扩展才能看到托盘图标。
- 点窗口关闭按钮（`X`）和 `Esc` 一样只隐藏到托盘，不会退出程序；要真正退出请用托盘菜单里的「退出」。
- Linux Wayland 对全局快捷键支持有限；如果设置里提示注册失败，可以改用桌面环境自带的快捷键绑定启动命令。

### 其他特性

- 文件始终加密，内容为 JSON。
- 使用应用内编辑，不提供外部编辑器打开明文文件。
- 窗口默认 920×620，支持分组 / 标签 / 内容卡片管理。
- 生产版本禁用右键菜单，并拦截 Ctrl+S / Ctrl+P / Ctrl+U，避免随手导出页面。
- 全局快捷键在程序内注册，不需要系统快捷键绑定到启动命令。
- 首次启动会把旧 `memos.md` 或旧 Markdown 数据自动迁移到新的加密 JSON 结构。

## 许可证

MIT License，见 [LICENSE](LICENSE)。
