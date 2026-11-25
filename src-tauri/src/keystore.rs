use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::crypto::KEY_LEN;
use crate::platform;

/// 软件自己维护的密钥文件，放在数据目录之外，避免跟随备忘文件同步。
pub struct KeyScope {
    data_key_path: PathBuf,
    totp_secret_path: PathBuf,
}

impl KeyScope {
    pub fn new(data_dir: &Path) -> Result<Self, String> {
        let mut hasher = Sha256::new();
        hasher.update(data_dir.to_string_lossy().as_bytes());
        let digest = hasher.finalize();
        let suffix = hex::encode(&digest[..6]);
        let config_dir = platform::config_dir()?;

        Ok(Self {
            data_key_path: config_dir.join(format!("data-key-{suffix}.key")),
            totp_secret_path: config_dir.join(format!("totp-{suffix}.secret")),
        })
    }

    pub fn store_data_key(&self, key: &[u8; KEY_LEN]) -> Result<(), String> {
        write_private_file(&self.data_key_path, &hex::encode(key))
    }

    pub fn load_data_key(&self) -> Result<Option<[u8; KEY_LEN]>, String> {
        if !self.data_key_path.exists() {
            return Ok(None);
        }
        let encoded = fs::read_to_string(&self.data_key_path)
            .map_err(|err| format!("读取软件密钥失败：{err}"))?;
        decode_key(encoded.trim()).map(Some)
    }

    pub fn delete_data_key(&self) {
        let _ = fs::remove_file(&self.data_key_path);
    }

    pub fn store_totp_secret(&self, secret: &str) -> Result<(), String> {
        write_private_file(&self.totp_secret_path, secret)
    }

    pub fn load_totp_secret(&self) -> Result<Option<String>, String> {
        if !self.totp_secret_path.exists() {
            return Ok(None);
        }
        fs::read_to_string(&self.totp_secret_path)
            .map(|value| Some(value.trim().to_string()))
            .map_err(|err| format!("读取应用锁密钥失败：{err}"))
    }

    pub fn delete_totp_secret(&self) {
        let _ = fs::remove_file(&self.totp_secret_path);
    }
}

fn decode_key(encoded: &str) -> Result<[u8; KEY_LEN], String> {
    let bytes = hex::decode(encoded).map_err(|_| "软件密钥格式无效".to_string())?;
    bytes
        .try_into()
        .map_err(|_| "软件密钥长度无效".to_string())
}

fn write_private_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建密钥目录失败：{err}"))?;
    }

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|err| format!("写入软件密钥失败：{err}"))?;
        file.write_all(content.as_bytes())
            .map_err(|err| format!("写入软件密钥失败：{err}"))?;
    }

    #[cfg(not(unix))]
    {
        fs::write(path, content).map_err(|err| format!("写入软件密钥失败：{err}"))?;
    }

    Ok(())
}
