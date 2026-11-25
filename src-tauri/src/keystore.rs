use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::crypto::{self, KEY_LEN};
use crate::platform;

const CACHE_MAGIC: &[u8] = b"QMCACHE1";

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
        store_secret(&self.data_key_path, key)
    }

    pub fn load_data_key(&self) -> Result<Option<[u8; KEY_LEN]>, String> {
        let Some(bytes) = read_private_file(&self.data_key_path)? else {
            return Ok(None);
        };

        // 旧版本把密钥以明文十六进制保存，读取后自动升级为加密格式。
        if !bytes.starts_with(CACHE_MAGIC) {
            let key = std::str::from_utf8(&bytes)
                .ok()
                .and_then(|text| decode_key(text.trim()).ok());
            return match key {
                Some(key) => {
                    let _ = self.store_data_key(&key);
                    Ok(Some(key))
                }
                None => {
                    let _ = fs::remove_file(&self.data_key_path);
                    Ok(None)
                }
            };
        }

        let plain = match decode_secret(&bytes) {
            Ok(plain) if plain.len() == KEY_LEN => plain,
            _ => {
                // 设备标识变化或缓存损坏时删除缓存，退回输入数据密码。
                let _ = fs::remove_file(&self.data_key_path);
                return Ok(None);
            }
        };
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&plain);
        Ok(Some(key))
    }

    pub fn delete_data_key(&self) {
        let _ = fs::remove_file(&self.data_key_path);
    }

    pub fn store_totp_secret(&self, secret: &str) -> Result<(), String> {
        store_secret(&self.totp_secret_path, secret.as_bytes())
    }

    pub fn load_totp_secret(&self) -> Result<Option<String>, String> {
        let Some(bytes) = read_private_file(&self.totp_secret_path)? else {
            return Ok(None);
        };

        if !bytes.starts_with(CACHE_MAGIC) {
            let secret = String::from_utf8(bytes)
                .map_err(|_| "应用锁密钥格式无效".to_string())?
                .trim()
                .to_string();
            let _ = self.store_totp_secret(&secret);
            return Ok(Some(secret));
        }

        let plain = decode_secret(&bytes).map_err(|err| format!("应用锁密钥无法解密：{err}"))?;
        String::from_utf8(plain)
            .map(Some)
            .map_err(|_| "应用锁密钥格式无效".to_string())
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

/// 缓存文件格式：QMCACHE1 + 随机盐 + nonce + 密文。
/// 密钥由内置密钥、当前设备标识和缓存盐派生，避免密钥以明文落盘。
fn store_secret(path: &Path, content: &[u8]) -> Result<(), String> {
    let salt = crypto::new_salt();
    let key = crypto::derive_cache_key(&salt, &device_identity())?;
    let blob = crypto::encrypt_blob(content, &key)?;

    let mut output = Vec::with_capacity(CACHE_MAGIC.len() + crypto::SALT_LEN + blob.len());
    output.extend_from_slice(CACHE_MAGIC);
    output.extend_from_slice(&salt);
    output.extend_from_slice(&blob);
    write_private_file(path, &output)
}

fn decode_secret(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let payload = bytes
        .strip_prefix(CACHE_MAGIC)
        .ok_or_else(|| "缓存数据格式无效".to_string())?;
    if payload.len() < crypto::SALT_LEN {
        return Err("缓存数据已损坏".to_string());
    }
    let (salt_bytes, blob) = payload.split_at(crypto::SALT_LEN);
    let mut salt = [0u8; crypto::SALT_LEN];
    salt.copy_from_slice(salt_bytes);
    let key = crypto::derive_cache_key(&salt, &device_identity())?;
    crypto::decrypt_blob(blob, &key)
}

/// 用设备标识做基础绑定：缓存文件复制到其他机器或账号后无法直接解密。
/// 设备标识可被同用户读取，因此这只能防明文落盘和随手拷贝，不能防本机恶意软件。
fn device_identity() -> Vec<u8> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        use winreg::RegKey;

        if let Ok(key) =
            RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Cryptography")
        {
            if let Ok(guid) = key.get_value::<String, _>("MachineGuid") {
                let guid = guid.trim();
                if !guid.is_empty() {
                    return guid.as_bytes().to_vec();
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
            if let Ok(value) = fs::read_to_string(path) {
                let value = value.trim();
                if !value.is_empty() {
                    return value.as_bytes().to_vec();
                }
            }
        }
    }

    let mut identity = Vec::new();
    for key in ["USERNAME", "USER", "COMPUTERNAME", "HOSTNAME"] {
        if let Some(value) = std::env::var_os(key) {
            identity.extend_from_slice(value.to_string_lossy().as_bytes());
            identity.push(0);
        }
    }
    identity
}

fn read_private_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    if !path.exists() {
        return Ok(None);
    }
    fs::read(path)
        .map(Some)
        .map_err(|err| format!("读取软件密钥失败：{err}"))
}

fn write_private_file(path: &Path, content: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建密钥目录失败：{err}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|err| format!("写入软件密钥失败：{err}"))?;
        file.write_all(content)
            .map_err(|err| format!("写入软件密钥失败：{err}"))?;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }

    #[cfg(not(unix))]
    {
        fs::write(path, content).map_err(|err| format!("写入软件密钥失败：{err}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_file_is_encrypted_and_roundtrips() {
        let dir = std::env::temp_dir().join(format!("quickmemo-keystore-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cache.key");

        store_secret(&path, b"hello cache").unwrap();
        let stored = fs::read(&path).unwrap();
        assert!(stored.starts_with(CACHE_MAGIC));
        assert!(!stored.windows(11).any(|window| window == b"hello cache"));
        assert_eq!(decode_secret(&stored).unwrap(), b"hello cache");

        let _ = fs::remove_dir_all(&dir);
    }
}
