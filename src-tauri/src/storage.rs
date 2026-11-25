use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::crypto::{self, FileMode};
use crate::keystore::KeyScope;
use crate::platform;
use crate::totp;

const PLAIN_FILE: &str = "memos.md";
const ENC_FILE: &str = "memos.enc";
const CONFIG_FILE: &str = "config.json";
const DEFAULT_TTL_DAYS: u64 = 7;
const DEFAULT_GROUP: &str = "默认分组";

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct Config {
    autostart: bool,
    start_hidden: bool,
    data_password: bool,
    totp_enabled: bool,
    totp_ttl_days: u64,
    last_active: i64,
    toggle_shortcut: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            autostart: false,
            start_hidden: false,
            data_password: false,
            totp_enabled: false,
            totp_ttl_days: DEFAULT_TTL_DAYS,
            last_active: 0,
            toggle_shortcut: "Alt+B".to_string(),
        }
    }
}

impl Config {
    fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) -> Result<(), String> {
        let content =
            serde_json::to_string_pretty(self).map_err(|err| format!("序列化配置失败：{err}"))?;
        write_atomic(path, format!("{content}\n").as_bytes())
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Entry {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub lines: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Tag {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub tags: Vec<Tag>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Database {
    #[serde(default = "database_version")]
    version: u32,
    #[serde(default)]
    groups: Vec<Group>,
}

fn database_version() -> u32 {
    1
}

#[derive(Serialize)]
pub struct Snapshot {
    pub groups: Vec<Group>,
    pub locked: bool,
    pub need_totp: bool,
    pub need_data_password: bool,
    pub totp_enabled: bool,
    pub data_password: bool,
    pub ttl_days: u64,
    pub last_active: i64,
    pub data_dir: String,
    pub version: String,
    pub autostart: bool,
    pub start_hidden: bool,
    pub toggle_shortcut: String,
    pub shortcut_hint: Option<String>,
    pub debug: bool,
    pub warning: Option<String>,
}

struct PreparedFile {
    mode: FileMode,
    key: Option<[u8; crypto::KEY_LEN]>,
    salt: [u8; crypto::SALT_LEN],
}

pub struct Session {
    dir: PathBuf,
    config_path: PathBuf,
    config: Config,
    scope: KeyScope,
    key: Option<[u8; crypto::KEY_LEN]>,
    mode: FileMode,
    salt: [u8; crypto::SALT_LEN],
    need_totp: bool,
    need_data_password: bool,
    shortcut_warning: Option<String>,
    warning: Option<String>,
}

impl Session {
    pub fn load() -> Result<Self, String> {
        let dir = platform::data_dir()?;
        fs::create_dir_all(&dir).map_err(|err| format!("创建数据目录失败：{err}"))?;

        let config_path = dir.join(CONFIG_FILE);
        let mut config = Config::load(&config_path);
        if config.totp_ttl_days == 0 {
            config.totp_ttl_days = DEFAULT_TTL_DAYS;
        }
        let scope = KeyScope::new(&dir)?;
        let prepared = prepare_file(&dir, &scope)?;

        let mut warning = None;
        let mut totp_enabled = config.totp_enabled;
        if totp_enabled {
            match scope.load_totp_secret() {
                Ok(Some(_)) => {}
                Ok(None) => {
                    totp_enabled = false;
                    warning = Some("应用锁密钥不存在，已停用应用锁".to_string());
                }
                Err(err) => {
                    totp_enabled = false;
                    warning = Some(format!("应用锁密钥不可用，已停用应用锁：{err}"));
                }
            }
        }

        let data_password = prepared.mode == FileMode::Password;
        let mut config_dirty = false;
        if config.totp_enabled != totp_enabled {
            config.totp_enabled = totp_enabled;
            config_dirty = true;
        }
        if config.data_password != data_password {
            config.data_password = data_password;
            config_dirty = true;
        }

        let now = now_secs();
        let stale = config.last_active <= 0
            || now - config.last_active > (config.totp_ttl_days as i64) * 86_400;
        let need_totp = totp_enabled && stale;
        let mut need_data_password = data_password && prepared.key.is_none();
        if data_password && !totp_enabled && stale {
            need_data_password = true;
        }
        let locked = need_totp || need_data_password;

        if !locked {
            config.last_active = now;
            config_dirty = true;
        }
        if config_dirty {
            config.save(&config_path)?;
        }

        Ok(Self {
            dir,
            config_path,
            config,
            scope,
            key: prepared.key,
            mode: prepared.mode,
            salt: prepared.salt,
            need_totp,
            need_data_password,
            shortcut_warning: None,
            warning,
        })
    }

    fn locked(&self) -> bool {
        self.need_totp || self.need_data_password
    }

    pub fn ensure_unlocked(&self) -> Result<(), String> {
        if self.locked() || self.key.is_none() {
            return Err("请先解锁".to_string());
        }
        Ok(())
    }

    pub fn snapshot(&mut self) -> Result<Snapshot, String> {
        let locked = self.locked();
        let groups = if locked {
            Vec::new()
        } else {
            self.read_database()?.groups
        };
        let warning = self.warning.take();
        let wayland_hint = if cfg!(target_os = "linux")
            && std::env::var("XDG_SESSION_TYPE")
                .map(|value| value.eq_ignore_ascii_case("wayland"))
                .unwrap_or(false)
        {
            Some(
                "Wayland 下全局快捷键可能不可靠，可在系统设置里把快捷键绑定到 `quickmemo --toggle`；GNOME 需启用 AppIndicator 扩展才能看到托盘图标。"
                    .to_string(),
            )
        } else {
            None
        };
        let shortcut_hint = self.shortcut_warning.clone().or(wayland_hint);

        Ok(Snapshot {
            groups,
            locked,
            need_totp: self.need_totp,
            need_data_password: self.need_data_password,
            totp_enabled: self.config.totp_enabled,
            data_password: self.mode == FileMode::Password,
            ttl_days: self.config.totp_ttl_days,
            last_active: self.config.last_active,
            data_dir: self.dir.display().to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            autostart: self.config.autostart,
            start_hidden: self.config.start_hidden,
            toggle_shortcut: self.config.toggle_shortcut.clone(),
            shortcut_hint,
            debug: cfg!(debug_assertions),
            warning,
        })
    }

    pub fn unlock(
        &mut self,
        totp_code: Option<String>,
        data_password: Option<String>,
    ) -> Result<(), String> {
        let mut pending: Option<([u8; crypto::KEY_LEN], FileMode, [u8; crypto::SALT_LEN])> = None;

        if self.need_data_password {
            let password = data_password.unwrap_or_default();
            if password.is_empty() {
                return Err("请输入数据密码".to_string());
            }

            let data = fs::read(self.dir.join(ENC_FILE))
                .map_err(|err| format!("读取加密文件失败：{err}"))?;
            let file = crypto::parse_file(&data)?;

            if file.version == 3 && file.mode == FileMode::Password {
                let old_key = crypto::unwrap_legacy_key(&file, &password)?;
                let text = crypto::decrypt_file(&file, &old_key)?;
                let salt = crypto::new_salt();
                let key = crypto::derive_password_key(&password, &salt)?;
                let migrated = crypto::encrypt_file(&text, &key, FileMode::Password, &salt)?;
                write_atomic(&self.dir.join(ENC_FILE), &migrated)?;
                pending = Some((key, FileMode::Password, salt));
            } else if file.mode == FileMode::Password {
                let key = crypto::derive_password_key(&password, &file.salt)?;
                crypto::decrypt_file(&file, &key)?;
                pending = Some((key, FileMode::Password, file.salt));
            } else if file.mode == FileMode::Software {
                let key = crypto::derive_basic_key(&file.salt)?;
                pending = Some((key, FileMode::Software, file.salt));
            } else {
                return Err("不支持的数据文件格式".to_string());
            }
        }

        if self.need_totp {
            let code = totp_code.unwrap_or_default();
            let secret = self
                .scope
                .load_totp_secret()?
                .ok_or_else(|| "应用锁密钥不存在，无法验证".to_string())?;
            totp::verify(&secret, &code)?;
        }

        if let Some((key, mode, salt)) = pending {
            if mode == FileMode::Password {
                if let Err(err) = self.scope.store_data_key(&key) {
                    self.warning = Some(format!(
                        "无法保存数据密钥缓存，下次启动仍需输入数据密码：{err}"
                    ));
                }
            } else {
                self.scope.delete_data_key();
            }
            self.key = Some(key);
            self.mode = mode;
            self.salt = salt;
        } else if self.key.is_none() {
            if self.mode == FileMode::Software {
                self.key = Some(crypto::derive_basic_key(&self.salt)?);
            } else {
                match self.scope.load_data_key()? {
                    Some(key) => self.key = Some(key),
                    None => return Err("软件密钥不可用".to_string()),
                }
            }
        }

        if self.key.is_none() {
            return Err("软件密钥不可用".to_string());
        }

        self.need_totp = false;
        self.need_data_password = false;
        self.config.last_active = now_secs();
        self.save_config()?;
        Ok(())
    }

    pub fn lock(&mut self) -> Result<(), String> {
        if !self.config.totp_enabled && self.mode != FileMode::Password {
            return Err("未启用应用锁或数据密码".to_string());
        }
        self.key = None;
        self.need_totp = self.config.totp_enabled;
        self.need_data_password = self.mode == FileMode::Password;
        self.config.last_active = 0;
        self.save_config()?;
        Ok(())
    }

    pub fn confirm_totp_setup(&mut self, secret: &str, code: &str) -> Result<(), String> {
        self.ensure_unlocked()?;
        totp::verify(secret, code)?;
        self.scope.store_totp_secret(secret)?;
        self.config.totp_enabled = true;
        self.config.last_active = now_secs();
        self.save_config()?;
        Ok(())
    }

    pub fn disable_totp(&mut self) -> Result<(), String> {
        self.ensure_unlocked()?;
        self.scope.delete_totp_secret();
        self.config.totp_enabled = false;
        self.config.last_active = now_secs();
        self.save_config()?;
        Ok(())
    }

    pub fn set_ttl_days(&mut self, days: u64) -> Result<(), String> {
        if !(1..=365).contains(&days) {
            return Err("验证周期必须在 1 到 365 天之间".to_string());
        }
        self.config.totp_ttl_days = days;
        self.save_config()
    }

    pub fn set_data_password(&mut self, password: &str) -> Result<(), String> {
        self.ensure_unlocked()?;
        if password.chars().count() < 6 {
            return Err("数据密码至少 6 位".to_string());
        }

        let text = self.read_text()?;
        let salt = crypto::new_salt();
        let key = crypto::derive_password_key(password, &salt)?;
        let data = crypto::encrypt_file(&text, &key, FileMode::Password, &salt)?;
        write_atomic(&self.dir.join(ENC_FILE), &data)?;

        if let Err(err) = self.scope.store_data_key(&key) {
            self.warning = Some(format!(
                "无法保存数据密钥缓存，下次启动需要输入数据密码：{err}"
            ));
        }

        self.mode = FileMode::Password;
        self.key = Some(key);
        self.salt = salt;
        self.config.data_password = true;
        self.config.last_active = now_secs();
        self.save_config()?;
        Ok(())
    }

    pub fn disable_data_password(&mut self) -> Result<(), String> {
        self.ensure_unlocked()?;
        if self.mode != FileMode::Password {
            return Ok(());
        }

        let text = self.read_text()?;
        let salt = crypto::new_salt();
        let key = crypto::derive_basic_key(&salt)?;
        let data = crypto::encrypt_file(&text, &key, FileMode::Software, &salt)?;
        write_atomic(&self.dir.join(ENC_FILE), &data)?;

        self.scope.delete_data_key();
        self.mode = FileMode::Software;
        self.key = Some(key);
        self.salt = salt;
        self.config.data_password = false;
        self.config.last_active = now_secs();
        self.save_config()?;
        Ok(())
    }

    pub fn start_totp_setup(&self) -> Result<totp::TotpSetup, String> {
        self.ensure_unlocked()?;
        totp::create_setup()
    }

    pub fn create_group(&mut self, name: &str) -> Result<(), String> {
        self.ensure_unlocked()?;
        let name = clean_name(name, "分组")?;
        let mut db = self.read_database()?;
        if db.groups.iter().any(|group| group.name == name) {
            return Err("分组已存在".to_string());
        }
        let id = unique_id(&db, "g");
        db.groups.push(Group {
            id,
            name,
            tags: Vec::new(),
        });
        self.write_database(&db)
    }

    pub fn rename_group(&mut self, id: &str, name: &str) -> Result<(), String> {
        self.ensure_unlocked()?;
        let name = clean_name(name, "分组")?;
        let mut db = self.read_database()?;
        if db
            .groups
            .iter()
            .any(|group| group.name == name && group.id != id)
        {
            return Err("分组已存在".to_string());
        }
        let group = db
            .groups
            .iter_mut()
            .find(|group| group.id == id)
            .ok_or_else(|| "分组不存在".to_string())?;
        group.name = name;
        self.write_database(&db)
    }

    pub fn delete_group(&mut self, id: &str) -> Result<(), String> {
        self.ensure_unlocked()?;
        let mut db = self.read_database()?;
        let index = db
            .groups
            .iter()
            .position(|group| group.id == id)
            .ok_or_else(|| "分组不存在".to_string())?;
        if db.groups.len() <= 1 {
            return Err("至少保留一个分组".to_string());
        }
        if !db.groups[index].tags.is_empty() {
            return Err("分组里还有标签，请先移动或删除标签".to_string());
        }
        db.groups.remove(index);
        self.write_database(&db)
    }

    pub fn create_tag(&mut self, group_id: &str, name: &str) -> Result<(), String> {
        self.ensure_unlocked()?;
        let name = clean_name(name, "标签")?;
        let mut db = self.read_database()?;
        let group = db
            .groups
            .iter()
            .find(|group| group.id == group_id)
            .ok_or_else(|| "分组不存在".to_string())?;
        if group.tags.iter().any(|tag| tag.name == name) {
            return Err("这个分组下已有同名标签".to_string());
        }
        let id = unique_id(&db, "t");
        let group = db
            .groups
            .iter_mut()
            .find(|group| group.id == group_id)
            .ok_or_else(|| "分组不存在".to_string())?;
        group.tags.push(Tag {
            id,
            name,
            entries: Vec::new(),
        });
        self.write_database(&db)
    }

    pub fn rename_tag(&mut self, id: &str, name: &str) -> Result<(), String> {
        self.ensure_unlocked()?;
        let name = clean_name(name, "标签")?;
        let mut db = self.read_database()?;
        let (group_index, tag_index) =
            find_tag(&db, id).ok_or_else(|| "标签不存在".to_string())?;
        if db.groups[group_index]
            .tags
            .iter()
            .any(|tag| tag.name == name && tag.id != id)
        {
            return Err("这个分组下已有同名标签".to_string());
        }
        db.groups[group_index].tags[tag_index].name = name;
        self.write_database(&db)
    }

    pub fn move_tag(&mut self, id: &str, group_id: &str) -> Result<(), String> {
        self.ensure_unlocked()?;
        let mut db = self.read_database()?;
        let (source_index, tag_index) =
            find_tag(&db, id).ok_or_else(|| "标签不存在".to_string())?;
        if db.groups[source_index].id == group_id {
            return Err("标签已经在这个分组".to_string());
        }
        let target_index = db
            .groups
            .iter()
            .position(|group| group.id == group_id)
            .ok_or_else(|| "目标分组不存在".to_string())?;
        let tag_name = db.groups[source_index].tags[tag_index].name.clone();
        if db.groups[target_index]
            .tags
            .iter()
            .any(|tag| tag.name == tag_name)
        {
            return Err("目标分组已有同名标签".to_string());
        }

        let tag = db.groups[source_index].tags.remove(tag_index);
        let target_index = if target_index > source_index {
            target_index - 1
        } else {
            target_index
        };
        db.groups[target_index].tags.push(tag);
        self.write_database(&db)
    }

    pub fn delete_tag(&mut self, id: &str) -> Result<(), String> {
        self.ensure_unlocked()?;
        let mut db = self.read_database()?;
        let (group_index, tag_index) =
            find_tag(&db, id).ok_or_else(|| "标签不存在".to_string())?;
        db.groups[group_index].tags.remove(tag_index);
        self.write_database(&db)
    }

    pub fn create_entry(&mut self, tag_id: &str, title: &str, body: &str) -> Result<(), String> {
        self.ensure_unlocked()?;
        let title = clean_title(title);
        let lines = normalize_lines(body);
        let mut db = self.read_database()?;
        let (group_index, tag_index) =
            find_tag(&db, tag_id).ok_or_else(|| "标签不存在".to_string())?;
        let id = unique_id(&db, "e");
        db.groups[group_index].tags[tag_index]
            .entries
            .push(Entry { id, title, lines });
        self.write_database(&db)
    }

    pub fn update_entry(
        &mut self,
        id: &str,
        title: &str,
        body: &str,
    ) -> Result<(), String> {
        self.ensure_unlocked()?;
        let title = clean_title(title);
        let lines = normalize_lines(body);
        let mut db = self.read_database()?;
        let mut found = false;

        'outer: for group in &mut db.groups {
            for tag in &mut group.tags {
                if let Some(entry) = tag.entries.iter_mut().find(|entry| entry.id == id) {
                    entry.title = title;
                    entry.lines = lines;
                    found = true;
                    break 'outer;
                }
            }
        }

        if !found {
            return Err("内容不存在".to_string());
        }
        self.write_database(&db)
    }

    pub fn delete_entry(&mut self, id: &str) -> Result<(), String> {
        self.ensure_unlocked()?;
        let mut db = self.read_database()?;
        let mut found = false;

        'outer: for group in &mut db.groups {
            for tag in &mut group.tags {
                if let Some(index) = tag.entries.iter().position(|entry| entry.id == id) {
                    tag.entries.remove(index);
                    found = true;
                    break 'outer;
                }
            }
        }

        if !found {
            return Err("内容不存在".to_string());
        }
        self.write_database(&db)
    }

    pub fn set_autostart(&mut self, enabled: bool) -> Result<(), String> {
        platform::set_autostart(enabled, self.config.start_hidden)?;
        self.config.autostart = enabled;
        self.save_config()
    }

    pub fn set_start_hidden(&mut self, enabled: bool) -> Result<(), String> {
        if self.config.autostart {
            platform::set_autostart(true, enabled)?;
        }
        self.config.start_hidden = enabled;
        self.save_config()
    }

    pub fn toggle_shortcut(&self) -> String {
        self.config.toggle_shortcut.clone()
    }

    pub fn set_toggle_shortcut(&mut self, shortcut: &str) -> Result<(), String> {
        self.config.toggle_shortcut = shortcut.trim().to_string();
        self.save_config()
    }

    pub fn set_shortcut_warning(&mut self, message: Option<String>) {
        self.shortcut_warning = message;
    }

    pub fn set_warning(&mut self, message: String) {
        self.warning = Some(message);
    }

    pub fn open_data_dir(&self) -> Result<(), String> {
        platform::open_path(&self.dir, true)
    }

    fn read_database(&mut self) -> Result<Database, String> {
        let text = self.read_text()?;
        if text.trim().is_empty() {
            let db = default_database();
            self.write_database(&db)?;
            return Ok(db);
        }
        if let Ok(db) = serde_json::from_str::<Database>(&text) {
            return Ok(db);
        }
        if text.trim_start().starts_with('{') {
            return Err("备忘数据已损坏，无法解析".to_string());
        }

        let db = migrate_markdown(&text)?;
        self.write_database(&db)?;
        Ok(db)
    }

    fn write_database(&mut self, db: &Database) -> Result<(), String> {
        let text = serde_json::to_string_pretty(db)
            .map_err(|err| format!("序列化备忘失败：{err}"))?;
        self.write_text(&format!("{text}\n"))
    }

    fn read_text(&self) -> Result<String, String> {
        let key = self
            .key
            .as_ref()
            .ok_or_else(|| "备忘已锁定".to_string())?;
        let data = fs::read(self.dir.join(ENC_FILE))
            .map_err(|err| format!("读取加密文件失败：{err}"))?;
        let file = crypto::parse_file(&data)?;
        crypto::decrypt_file(&file, key)
    }

    fn write_text(&self, text: &str) -> Result<(), String> {
        let key = self
            .key
            .as_ref()
            .ok_or_else(|| "备忘已锁定".to_string())?;
        let data = crypto::encrypt_file(text, key, self.mode, &self.salt)?;
        write_atomic(&self.dir.join(ENC_FILE), &data)
    }

    fn save_config(&self) -> Result<(), String> {
        self.config.save(&self.config_path)
    }
}

fn prepare_file(dir: &Path, scope: &KeyScope) -> Result<PreparedFile, String> {
    let enc_path = dir.join(ENC_FILE);
    let md_path = dir.join(PLAIN_FILE);

    if enc_path.exists() {
        let bytes = fs::read(&enc_path).map_err(|err| format!("读取加密文件失败：{err}"))?;
        let file = crypto::parse_file(&bytes)?;

        if file.version == 4 {
            match file.mode {
                FileMode::Software => {
                    let key = crypto::derive_basic_key(&file.salt)?;
                    scope.delete_data_key();
                    if md_path.exists() {
                        let _ = fs::remove_file(&md_path);
                    }
                    Ok(PreparedFile {
                        mode: FileMode::Software,
                        key: Some(key),
                        salt: file.salt,
                    })
                }
                FileMode::Password => {
                    let key = match scope.load_data_key()? {
                        Some(cached) if crypto::decrypt_file(&file, &cached).is_ok() => {
                            Some(cached)
                        }
                        Some(_) => {
                            scope.delete_data_key();
                            None
                        }
                        None => None,
                    };
                    if md_path.exists() {
                        let _ = fs::remove_file(&md_path);
                    }
                    Ok(PreparedFile {
                        mode: FileMode::Password,
                        key,
                        salt: file.salt,
                    })
                }
            }
        } else if file.version == 3 && file.mode == FileMode::Software {
            migrate_legacy_basic(&enc_path, &file, scope, &md_path)
        } else if file.version == 3 && file.mode == FileMode::Password {
            if md_path.exists() {
                let _ = fs::remove_file(&md_path);
            }
            Ok(PreparedFile {
                mode: FileMode::Password,
                key: None,
                salt: file.salt,
            })
        } else {
            Err("不支持的数据文件格式".to_string())
        }
    } else {
        let text = if md_path.exists() {
            let markdown =
                fs::read_to_string(&md_path).map_err(|err| format!("读取备忘失败：{err}"))?;
            let db = if markdown.trim().is_empty() {
                default_database()
            } else if markdown.trim_start().starts_with('{') {
                serde_json::from_str::<Database>(&markdown)
                    .map_err(|err| format!("解析备忘失败：{err}"))?
            } else {
                migrate_markdown(&markdown)?
            };
            serde_json::to_string_pretty(&db)
                .map_err(|err| format!("序列化备忘失败：{err}"))?
        } else {
            serde_json::to_string_pretty(&default_database())
                .map_err(|err| format!("序列化备忘失败：{err}"))?
        };

        let salt = crypto::new_salt();
        let key = crypto::derive_basic_key(&salt)?;
        let data = crypto::encrypt_file(&format!("{text}\n"), &key, FileMode::Software, &salt)?;
        scope.delete_data_key();
        write_atomic(&enc_path, &data)?;
        if md_path.exists() {
            let _ = fs::remove_file(&md_path);
        }

        Ok(PreparedFile {
            mode: FileMode::Software,
            key: Some(key),
            salt,
        })
    }
}

fn migrate_legacy_basic(
    enc_path: &Path,
    file: &crypto::EncryptedFile,
    scope: &KeyScope,
    md_path: &Path,
) -> Result<PreparedFile, String> {
    let old_key = scope
        .load_data_key()?
        .ok_or_else(|| "旧版软件密钥丢失，无法迁移数据".to_string())?;
    let text = crypto::decrypt_file(file, &old_key)?;

    let salt = crypto::new_salt();
    let key = crypto::derive_basic_key(&salt)?;
    let data = crypto::encrypt_file(&text, &key, FileMode::Software, &salt)?;
    write_atomic(enc_path, &data)?;
    scope.delete_data_key();
    if md_path.exists() {
        let _ = fs::remove_file(md_path);
    }

    Ok(PreparedFile {
        mode: FileMode::Software,
        key: Some(key),
        salt,
    })
}

struct LegacyTag {
    name: String,
    entries: Vec<Entry>,
}

fn migrate_markdown(text: &str) -> Result<Database, String> {
    if text.trim().is_empty() {
        return Ok(default_database());
    }

    let mut tags: Vec<LegacyTag> = Vec::new();
    let mut tag: Option<LegacyTag> = None;
    let mut entry: Option<Entry> = None;

    for raw in text.lines() {
        if let Some(rest) = raw.strip_prefix("# ") {
            flush_tag(&mut tags, &mut tag, &mut entry);
            let name = rest.trim();
            tag = Some(LegacyTag {
                name: if name.is_empty() {
                    "未命名".to_string()
                } else {
                    name.to_string()
                },
                entries: Vec::new(),
            });
        } else if let Some(rest) = raw.strip_prefix("## ") {
            if tag.is_none() {
                tag = Some(LegacyTag {
                    name: "未分组".to_string(),
                    entries: Vec::new(),
                });
            }
            flush_entry(&mut tag, &mut entry);
            let title = rest.trim();
            entry = Some(Entry {
                id: new_id("e"),
                title: if title.is_empty() {
                    "未命名".to_string()
                } else {
                    title.to_string()
                },
                lines: Vec::new(),
            });
        } else if tag.is_some() {
            if entry.is_none() && !raw.trim().is_empty() {
                entry = Some(Entry {
                    id: new_id("e"),
                    title: "未命名".to_string(),
                    lines: Vec::new(),
                });
            }
            if let Some(current) = entry.as_mut() {
                current.lines.push(raw.to_string());
            }
        }
    }
    flush_tag(&mut tags, &mut tag, &mut entry);

    if tags.is_empty() {
        return Err("旧的 Markdown 结构无法识别".to_string());
    }

    for legacy_tag in &mut tags {
        for entry in &mut legacy_tag.entries {
            trim_empty_edges(&mut entry.lines);
        }
    }

    let tags = tags
        .into_iter()
        .map(|legacy| Tag {
            id: new_id("t"),
            name: legacy.name,
            entries: legacy.entries,
        })
        .collect();

    Ok(Database {
        version: 1,
        groups: vec![Group {
            id: new_id("g"),
            name: DEFAULT_GROUP.to_string(),
            tags,
        }],
    })
}

fn flush_entry(tag: &mut Option<LegacyTag>, entry: &mut Option<Entry>) {
    if let (Some(tag), Some(entry)) = (tag.as_mut(), entry.take()) {
        tag.entries.push(entry);
    }
}

fn flush_tag(
    tags: &mut Vec<LegacyTag>,
    tag: &mut Option<LegacyTag>,
    entry: &mut Option<Entry>,
) {
    flush_entry(tag, entry);
    if let Some(tag) = tag.take() {
        tags.push(tag);
    }
}

fn default_database() -> Database {
    Database {
        version: 1,
        groups: vec![Group {
            id: new_id("g"),
            name: DEFAULT_GROUP.to_string(),
            tags: vec![Tag {
                id: new_id("t"),
                name: "示例".to_string(),
                entries: vec![Entry {
                    id: new_id("e"),
                    title: "点击单行复制这一行".to_string(),
                    lines: vec!["点击单行复制这一行".to_string()],
                }],
            }],
        }],
    }
}

fn find_tag(db: &Database, tag_id: &str) -> Option<(usize, usize)> {
    for (group_index, group) in db.groups.iter().enumerate() {
        if let Some(tag_index) = group.tags.iter().position(|tag| tag.id == tag_id) {
            return Some((group_index, tag_index));
        }
    }
    None
}

fn unique_id(db: &Database, prefix: &str) -> String {
    loop {
        let id = new_id(prefix);
        let exists = db.groups.iter().any(|group| {
            group.id == id
                || group.tags.iter().any(|tag| {
                    tag.id == id || tag.entries.iter().any(|entry| entry.id == id)
                })
        });
        if !exists {
            return id;
        }
    }
}

fn new_id(prefix: &str) -> String {
    let mut bytes = [0u8; 6];
    OsRng.fill_bytes(&mut bytes);
    format!("{prefix}-{}", hex::encode(bytes))
}

fn clean_name(name: &str, kind: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("{kind}名称不能为空"));
    }
    if name.chars().count() > 60 {
        return Err(format!("{kind}名称不能超过 60 个字符"));
    }
    Ok(name.to_string())
}

fn clean_title(title: &str) -> String {
    let title = title.trim();
    if title.is_empty() {
        "未命名".to_string()
    } else {
        title.chars().take(120).collect()
    }
}

fn normalize_lines(body: &str) -> Vec<String> {
    let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
    trim_empty_edges(&mut lines);
    lines
}

fn trim_empty_edges(lines: &mut Vec<String>) {
    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("quickmemo.tmp");
    let tmp_path = path.with_file_name(format!("{file_name}.tmp"));
    fs::write(&tmp_path, bytes).map_err(|err| format!("写入失败：{err}"))?;
    fs::rename(&tmp_path, path).map_err(|err| format!("写入失败：{err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_markdown_to_default_group() {
        let text = "# Git\n\n## 查看分支\ngit branch -r\n\n## 清理\n\ngit remote prune origin\n\n# CSS\n\n## 居中\ndisplay: flex;\n";
        let db = migrate_markdown(text).unwrap();
        assert_eq!(db.groups.len(), 1);
        assert_eq!(db.groups[0].name, DEFAULT_GROUP);
        assert_eq!(db.groups[0].tags.len(), 2);
        assert_eq!(db.groups[0].tags[0].name, "Git");
        assert_eq!(db.groups[0].tags[0].entries.len(), 2);
        assert_eq!(db.groups[0].tags[0].entries[0].title, "查看分支");
        assert_eq!(db.groups[0].tags[0].entries[0].lines, vec!["git branch -r"]);
        assert_eq!(db.groups[0].tags[1].name, "CSS");
        assert_eq!(db.groups[0].tags[1].entries[0].lines, vec!["display: flex;"]);
    }

    #[test]
    fn normalizes_body_edges() {
        let lines = normalize_lines("\n\n第一行\n\n第二行\n\n");
        assert_eq!(lines, vec!["第一行", "", "第二行"]);
    }
}
