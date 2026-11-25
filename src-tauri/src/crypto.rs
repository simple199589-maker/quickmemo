use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;

const MAGIC_V4: &[u8] = b"QMEMO4";
const MAGIC_V3: &[u8] = b"QMEMO3";
const LEGACY_MAGIC_V1: &[u8] = b"QMEMO1";
// 基础模式的内置密钥：保持固定，修改会导致所有基础模式文件无法打开。
// 这是软件级轻加密，能阻止其他工具直接读取，但不能抵御逆向提取。
const APP_SECRET: [u8; 32] = [
    0x41, 0x18, 0x8c, 0x7a, 0xa7, 0xff, 0xde, 0xe3, 0x45, 0x92, 0xd7, 0xa6, 0x58, 0xb9, 0x0a,
    0x61, 0xf8, 0xd4, 0x79, 0xb6, 0xbb, 0xb2, 0x16, 0x07, 0xe3, 0xce, 0x8e, 0xf0, 0xf1, 0x77,
    0xad, 0xac,
];

pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 12;
pub const KEY_LEN: usize = 32;
const LEGACY_WRAPPED_KEY_LEN: usize = KEY_LEN + 16;
const ARGON_MEMORY_KIB: u32 = 64 * 1024;
const ARGON_TIME: u32 = 3;
const ARGON_LANES: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileMode {
    Software,
    Password,
}

pub struct EncryptedFile {
    pub version: u8,
    pub mode: FileMode,
    pub salt: [u8; SALT_LEN],
    pub file_nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
    pub legacy_key_nonce: Option<[u8; NONCE_LEN]>,
    pub legacy_wrapped_key: Option<Vec<u8>>,
}

pub fn new_salt() -> [u8; SALT_LEN] {
    random_bytes()
}

/// 基础模式：用软件内置密钥和文件级随机盐派生，任何一份本软件都能打开。
pub fn derive_basic_key(salt: &[u8; SALT_LEN]) -> Result<[u8; KEY_LEN], String> {
    let hkdf = Hkdf::<Sha256>::new(Some(salt.as_slice()), &APP_SECRET);
    let mut key = [0u8; KEY_LEN];
    hkdf.expand(b"quickmemo-basic-v1", &mut key)
        .map_err(|_| "生成软件密钥失败".to_string())?;
    Ok(key)
}

/// 数据密码模式：密码和文件级随机盐派生，只有知道密码才能打开。
pub fn derive_password_key(
    password: &str,
    salt: &[u8; SALT_LEN],
) -> Result<[u8; KEY_LEN], String> {
    let params = Params::new(ARGON_MEMORY_KIB, ARGON_TIME, ARGON_LANES, Some(KEY_LEN))
        .map_err(|err| format!("初始化密钥派生失败：{err}"))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; KEY_LEN];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|err| format!("生成密钥失败：{err}"))?;
    Ok(key)
}

pub fn encrypt_file(
    plaintext: &str,
    key: &[u8; KEY_LEN],
    mode: FileMode,
    salt: &[u8; SALT_LEN],
) -> Result<Vec<u8>, String> {
    let file_nonce = random_bytes::<NONCE_LEN>();
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| "初始化加密器失败".to_string())?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&file_nonce), plaintext.as_bytes())
        .map_err(|_| "加密失败".to_string())?;

    let mut output = Vec::with_capacity(128 + ciphertext.len());
    output.extend_from_slice(MAGIC_V4);
    output.push(match mode {
        FileMode::Software => 0,
        FileMode::Password => 1,
    });
    output.extend_from_slice(salt);
    output.extend_from_slice(&file_nonce);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

pub fn parse_file(bytes: &[u8]) -> Result<EncryptedFile, String> {
    if bytes.starts_with(LEGACY_MAGIC_V1) {
        return Err("检测到最早的密码加密文件，请先用旧版本关闭加密后重试".to_string());
    }
    if bytes.starts_with(MAGIC_V4) {
        return parse_v4(bytes);
    }
    if bytes.starts_with(MAGIC_V3) {
        return parse_v3(bytes);
    }
    Err("不是有效的备忘文件".to_string())
}

fn parse_v4(bytes: &[u8]) -> Result<EncryptedFile, String> {
    let mut position = MAGIC_V4.len();
    if bytes.len() < position + 1 + SALT_LEN + NONCE_LEN {
        return Err("加密文件已损坏".to_string());
    }
    let mode = match bytes[position] {
        0 => FileMode::Software,
        1 => FileMode::Password,
        _ => return Err("未知的加密模式".to_string()),
    };
    position += 1;

    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&bytes[position..position + SALT_LEN]);
    position += SALT_LEN;

    let mut file_nonce = [0u8; NONCE_LEN];
    file_nonce.copy_from_slice(&bytes[position..position + NONCE_LEN]);
    position += NONCE_LEN;

    Ok(EncryptedFile {
        version: 4,
        mode,
        salt,
        file_nonce,
        ciphertext: bytes[position..].to_vec(),
        legacy_key_nonce: None,
        legacy_wrapped_key: None,
    })
}

fn parse_v3(bytes: &[u8]) -> Result<EncryptedFile, String> {
    let mut position = MAGIC_V3.len();
    if bytes.len() < position + 1 {
        return Err("加密文件已损坏".to_string());
    }
    let mode = match bytes[position] {
        0 => FileMode::Software,
        1 => FileMode::Password,
        _ => return Err("未知的加密模式".to_string()),
    };
    position += 1;

    let mut salt = [0u8; SALT_LEN];
    let mut legacy_key_nonce = None;
    let mut legacy_wrapped_key = None;

    if mode == FileMode::Password {
        if bytes.len() < position + SALT_LEN + NONCE_LEN + LEGACY_WRAPPED_KEY_LEN + NONCE_LEN {
            return Err("加密文件已损坏".to_string());
        }
        salt.copy_from_slice(&bytes[position..position + SALT_LEN]);
        position += SALT_LEN;

        let mut key_nonce = [0u8; NONCE_LEN];
        key_nonce.copy_from_slice(&bytes[position..position + NONCE_LEN]);
        position += NONCE_LEN;
        legacy_key_nonce = Some(key_nonce);

        legacy_wrapped_key =
            Some(bytes[position..position + LEGACY_WRAPPED_KEY_LEN].to_vec());
        position += LEGACY_WRAPPED_KEY_LEN;
    }

    if bytes.len() < position + NONCE_LEN {
        return Err("加密文件已损坏".to_string());
    }
    let mut file_nonce = [0u8; NONCE_LEN];
    file_nonce.copy_from_slice(&bytes[position..position + NONCE_LEN]);
    position += NONCE_LEN;

    Ok(EncryptedFile {
        version: 3,
        mode,
        salt,
        file_nonce,
        ciphertext: bytes[position..].to_vec(),
        legacy_key_nonce,
        legacy_wrapped_key,
    })
}

pub fn decrypt_file(file: &EncryptedFile, key: &[u8; KEY_LEN]) -> Result<String, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| "初始化解密器失败".to_string())?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&file.file_nonce),
            file.ciphertext.as_slice(),
        )
        .map_err(|_| "数据密钥错误或文件已损坏".to_string())?;
    String::from_utf8(plaintext).map_err(|_| "备忘内容不是有效的 UTF-8".to_string())
}

/// 旧版 QMEMO3 密码模式：数据密钥被密码派生的密钥包装。
pub fn unwrap_legacy_key(
    file: &EncryptedFile,
    password: &str,
) -> Result<[u8; KEY_LEN], String> {
    if file.version != 3 || file.mode != FileMode::Password {
        return Err("不是旧版密码加密文件".to_string());
    }
    let key_nonce = file
        .legacy_key_nonce
        .ok_or_else(|| "旧版密码文件缺少密钥信息".to_string())?;
    let wrapped = file
        .legacy_wrapped_key
        .as_ref()
        .ok_or_else(|| "旧版密码文件缺少密钥信息".to_string())?;

    let kek = derive_password_key(password, &file.salt)?;
    let cipher = Aes256Gcm::new_from_slice(&kek).map_err(|_| "初始化解密器失败".to_string())?;
    let key = cipher
        .decrypt(Nonce::from_slice(&key_nonce), wrapped.as_slice())
        .map_err(|_| "数据密码错误".to_string())?;
    key.try_into()
        .map_err(|_| "旧版数据密钥无效".to_string())
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_roundtrip_is_portable() {
        let salt = new_salt();
        let key = derive_basic_key(&salt).unwrap();
        let data =
            encrypt_file("# 备忘\n\n## 示例\nhello\n", &key, FileMode::Software, &salt).unwrap();
        assert_eq!(&data[..6], MAGIC_V4);
        let parsed = parse_file(&data).unwrap();
        assert_eq!(parsed.version, 4);
        assert_eq!(parsed.mode, FileMode::Software);
        assert_eq!(parsed.salt, salt);
        // 同一份软件在另一台设备上重新派生，仍能打开。
        let another_device_key = derive_basic_key(&parsed.salt).unwrap();
        assert_eq!(
            decrypt_file(&parsed, &another_device_key).unwrap(),
            "# 备忘\n\n## 示例\nhello\n"
        );
    }

    #[test]
    fn password_roundtrip_and_wrong_password() {
        let salt = new_salt();
        let key = derive_password_key("123456", &salt).unwrap();
        let data = encrypt_file("secret memo", &key, FileMode::Password, &salt).unwrap();
        let parsed = parse_file(&data).unwrap();
        assert_eq!(parsed.mode, FileMode::Password);
        assert_eq!(decrypt_file(&parsed, &key).unwrap(), "secret memo");
        let wrong = derive_password_key("654321", &salt).unwrap();
        assert!(decrypt_file(&parsed, &wrong).is_err());
    }

    #[test]
    fn legacy_magic_is_rejected() {
        assert!(parse_file(b"QMEMO1legacy").is_err());
    }
}
