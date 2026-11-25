use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// 数据目录优先使用 QUICKMEMO_DIR，便于放到同步盘或自定义位置。
pub fn data_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("QUICKMEMO_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }

    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return Ok(PathBuf::from(appdata).join("quickmemo"));
        }
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            return Ok(PathBuf::from(profile).join("AppData/Roaming/quickmemo"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Ok(PathBuf::from(home).join("Library/Application Support/quickmemo"));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return Ok(PathBuf::from(xdg).join("quickmemo"));
            }
        }
        if let Some(home) = std::env::var_os("HOME") {
            return Ok(PathBuf::from(home).join(".local/share/quickmemo"));
        }
    }

    Err("无法确定数据目录，请设置 QUICKMEMO_DIR 环境变量".to_string())
}

/// 密钥文件等本机私有数据的目录，刻意放在数据目录之外，避免跟随备忘文件同步。
pub fn config_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return Ok(PathBuf::from(local).join("quickmemo"));
        }
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            return Ok(PathBuf::from(profile).join("AppData/Local/quickmemo"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Ok(PathBuf::from(home).join("Library/Application Support/quickmemo"));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return Ok(PathBuf::from(xdg).join("quickmemo"));
            }
        }
        if let Some(home) = std::env::var_os("HOME") {
            return Ok(PathBuf::from(home).join(".config/quickmemo"));
        }
    }

    Err("无法确定配置目录".to_string())
}

#[cfg(windows)]
pub fn set_autostart(enabled: bool, hidden: bool) -> Result<(), String> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let run = hkcu
        .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .map(|(key, _)| key)
        .map_err(|err| format!("打开开机启动注册表失败：{err}"))?;

    if enabled {
        let exe = std::env::current_exe().map_err(|err| format!("获取程序路径失败：{err}"))?;
        let mut value = format!("\"{}\"", exe.display());
        if hidden {
            value.push_str(" --hidden");
        }
        run.set_value("quickmemo", &value)
            .map_err(|err| format!("写入开机启动项失败：{err}"))?;
    } else {
        match run.delete_value("quickmemo") {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(format!("关闭开机启动失败：{err}")),
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn set_autostart(enabled: bool, hidden: bool) -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or("无法确定用户主目录")?;
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home).join(".config"));
    let path = config_home.join("autostart/quickmemo.desktop");

    if enabled {
        let exe = std::env::current_exe().map_err(|err| format!("获取程序路径失败：{err}"))?;
        let hidden_arg = if hidden { " --hidden" } else { "" };
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=Quick Memo\nName[zh_CN]=备忘快贴\nExec=\"{}\"{}\nIcon=quickmemo\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            exe.display(),
            hidden_arg
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("创建自启目录失败：{err}"))?;
        }
        std::fs::write(&path, content).map_err(|err| format!("写入开机启动文件失败：{err}"))?;
    } else if path.exists() {
        std::fs::remove_file(&path).map_err(|err| format!("关闭开机启动失败：{err}"))?;
    }
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn set_autostart(_enabled: bool, _hidden: bool) -> Result<(), String> {
    Err("当前平台暂不支持开机启动".to_string())
}

#[allow(unreachable_code)]
pub fn open_path(path: &Path, is_dir: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let spawned = if is_dir {
            Command::new("explorer").arg(path).spawn()
        } else {
            Command::new("cmd")
                .args(["/C", "start", ""])
                .arg(path)
                .spawn()
        };
        return spawned
            .map(|_| ())
            .map_err(|err| format!("无法打开：{err}"));
    }

    #[cfg(target_os = "linux")]
    {
        if !is_dir {
            if let Ok(editor) = std::env::var("EDITOR") {
                let mut parts = editor.split_whitespace();
                if let Some(command) = parts.next() {
                    if Command::new(command).args(parts).arg(path).spawn().is_ok() {
                        return Ok(());
                    }
                }
            }
            for command in ["gnome-text-editor", "gedit", "xdg-open"] {
                match Command::new(command).arg(path).spawn() {
                    Ok(_) => return Ok(()),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(err) => return Err(format!("无法打开：{err}")),
                }
            }
            return Err("未找到可用的编辑器".to_string());
        }
        return Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("无法打开：{err}"));
    }

    #[cfg(target_os = "macos")]
    {
        return Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("无法打开：{err}"));
    }

    Err("当前平台不支持打开文件".to_string())
}

#[allow(unreachable_code)]
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::io::Write;

        let mut child = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", "$input | Set-Clipboard"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("复制失败：{err}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|err| format!("复制失败：{err}"))?;
        }
        let status = child.wait().map_err(|err| format!("复制失败：{err}"))?;
        return if status.success() {
            Ok(())
        } else {
            Err("复制失败".to_string())
        };
    }

    #[cfg(target_os = "linux")]
    {
        use std::io::Write;

        let candidates: [(&str, &[&str]); 2] = [
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
        ];
        for (command, args) in candidates {
            let spawned = Command::new(command)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if let Ok(mut child) = spawned {
                if let Some(mut stdin) = child.stdin.take() {
                    stdin
                        .write_all(text.as_bytes())
                        .map_err(|err| format!("复制失败：{err}"))?;
                }
                child.wait().map_err(|err| format!("复制失败：{err}"))?;
                return Ok(());
            }
        }
        return Err("复制失败，请安装 wl-clipboard 或 xclip".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        use std::io::Write;

        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|err| format!("复制失败：{err}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|err| format!("复制失败：{err}"))?;
        }
        child.wait().map_err(|err| format!("复制失败：{err}"))?;
        return Ok(());
    }

    Err("当前平台不支持剪贴板操作".to_string())
}
