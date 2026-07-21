use sha2::{Digest, Sha256};
use std::path::Path;

#[cfg(not(target_os = "macos"))]
use keyring::Entry;

#[cfg(target_os = "macos")]
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
#[cfg(target_os = "macos")]
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
#[cfg(target_os = "macos")]
use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

#[cfg(not(target_os = "macos"))]
const KEYRING_SERVICE: &str = "com.gravitypoet.finalsub";
const ACCOUNT_PREFIX: &str = "translate:v2:";
const MAX_SECRET_BYTES: usize = 64 * 1024;

#[cfg(target_os = "macos")]
const VAULT_VERSION: u32 = 1;
#[cfg(target_os = "macos")]
const VAULT_AAD: &[u8] = b"FinalSub local secret vault v1";
#[cfg(target_os = "macos")]
const MAX_VAULT_BYTES: u64 = 2 * 1024 * 1024;
#[cfg(target_os = "macos")]
const MAX_VAULT_ENTRIES: usize = 1024;

pub fn normalize_endpoint(endpoint: &str) -> String {
    endpoint.trim().trim_end_matches('/').to_string()
}

fn normalized_identity_part(value: &str, label: &str) -> Result<String, String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    Ok(normalized.to_string())
}

fn provider_secret_account(
    provider_id: &str,
    endpoint: &str,
    field: &str,
) -> Result<String, String> {
    let provider_id = normalized_identity_part(provider_id, "provider ID")?;
    let field = normalized_identity_part(field, "secret field")?;
    let endpoint = normalize_endpoint(endpoint);
    let identity = format!("{provider_id}\0{field}\0{endpoint}");
    let digest = Sha256::digest(identity.as_bytes());
    Ok(format!("{ACCOUNT_PREFIX}{}", hex::encode(digest)))
}

fn legacy_provider_secret_account(provider_id: &str, field: &str) -> Result<String, String> {
    let provider_id = normalized_identity_part(provider_id, "provider ID")?;
    let field = normalized_identity_part(field, "secret field")?;
    Ok(format!("translate:{provider_id}:{field}"))
}

trait SecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

#[cfg(not(target_os = "macos"))]
struct KeyringStore;

#[cfg(not(target_os = "macos"))]
impl SecretStore for KeyringStore {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        let entry = Entry::new(KEYRING_SERVICE, account).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        let entry = Entry::new(KEYRING_SERVICE, account).map_err(|e| e.to_string())?;
        entry.set_password(value).map_err(|e| e.to_string())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        let entry = Entry::new(KEYRING_SERVICE, account).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(not(target_os = "macos"))]
static KEYRING_STORE: KeyringStore = KeyringStore;

#[cfg(target_os = "macos")]
#[derive(Debug, Default, Deserialize, Serialize)]
struct VaultPayload {
    secrets: BTreeMap<String, String>,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Deserialize, Serialize)]
struct VaultEnvelope {
    version: u32,
    nonce: String,
    ciphertext: String,
}

#[cfg(target_os = "macos")]
struct EncryptedFileStore {
    root: PathBuf,
    key: [u8; 32],
    io_lock: Mutex<()>,
}

#[cfg(target_os = "macos")]
impl EncryptedFileStore {
    fn new(root: PathBuf) -> Result<Self, String> {
        ensure_private_directory(&root)?;
        let key = load_or_create_vault_key(&root)?;
        Ok(Self {
            root,
            key,
            io_lock: Mutex::new(()),
        })
    }

    fn vault_path(&self) -> PathBuf {
        self.root.join("vault.v1.json")
    }

    fn load_payload(&self) -> Result<VaultPayload, String> {
        let path = self.vault_path();
        if !path.exists() {
            return Ok(VaultPayload::default());
        }
        reject_symlink(&path, "secret vault")?;
        let metadata =
            fs::metadata(&path).map_err(|error| format!("无法读取密钥库状态：{error}"))?;
        if metadata.len() > MAX_VAULT_BYTES {
            return Err("本地密钥库超过大小上限".into());
        }
        enforce_private_file_permissions(&path)?;

        let mut serialized = String::new();
        OpenOptions::new()
            .read(true)
            .open(&path)
            .and_then(|mut file| file.read_to_string(&mut serialized))
            .map_err(|error| format!("无法读取本地密钥库：{error}"))?;
        let envelope: VaultEnvelope =
            serde_json::from_str(&serialized).map_err(|_| "本地密钥库格式无效".to_string())?;
        if envelope.version != VAULT_VERSION {
            return Err(format!("不支持的本地密钥库版本：{}", envelope.version));
        }
        let nonce = BASE64
            .decode(envelope.nonce)
            .map_err(|_| "本地密钥库 nonce 无效".to_string())?;
        let ciphertext = BASE64
            .decode(envelope.ciphertext)
            .map_err(|_| "本地密钥库密文无效".to_string())?;
        if nonce.len() != 24 {
            return Err("本地密钥库 nonce 长度无效".into());
        }
        let cipher = XChaCha20Poly1305::new((&self.key).into());
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: VAULT_AAD,
                },
            )
            .map_err(|_| "本地密钥库无法解密或已被篡改".to_string())?;
        let payload: VaultPayload =
            serde_json::from_slice(&plaintext).map_err(|_| "本地密钥库内容无效".to_string())?;
        validate_payload(&payload)?;
        Ok(payload)
    }

    fn save_payload(&self, payload: &VaultPayload) -> Result<(), String> {
        validate_payload(payload)?;
        ensure_private_directory(&self.root)?;
        let plaintext = serde_json::to_vec(payload)
            .map_err(|error| format!("无法序列化本地密钥库：{error}"))?;
        let nonce = random_vault_nonce();
        let cipher = XChaCha20Poly1305::new((&self.key).into());
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &plaintext,
                    aad: VAULT_AAD,
                },
            )
            .map_err(|_| "无法加密本地密钥库".to_string())?;
        let envelope = VaultEnvelope {
            version: VAULT_VERSION,
            nonce: BASE64.encode(nonce),
            ciphertext: BASE64.encode(ciphertext),
        };
        let serialized = serde_json::to_vec(&envelope)
            .map_err(|error| format!("无法封装本地密钥库：{error}"))?;
        if serialized.len() as u64 > MAX_VAULT_BYTES {
            return Err("本地密钥库超过大小上限".into());
        }

        let path = self.vault_path();
        let temp_path = self
            .root
            .join(format!(".vault.{}.tmp", uuid::Uuid::new_v4()));
        let write_result = (|| -> Result<(), String> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp_path)
                .map_err(|error| format!("无法创建本地密钥库临时文件：{error}"))?;
            file.write_all(&serialized)
                .and_then(|_| file.sync_all())
                .map_err(|error| format!("无法写入本地密钥库：{error}"))?;
            fs::rename(&temp_path, &path)
                .map_err(|error| format!("无法原子保存本地密钥库：{error}"))?;
            enforce_private_file_permissions(&path)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        write_result
    }
}

#[cfg(target_os = "macos")]
impl SecretStore for EncryptedFileStore {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        let _guard = self
            .io_lock
            .lock()
            .map_err(|_| "本地密钥库锁已损坏".to_string())?;
        Ok(self.load_payload()?.secrets.get(account).cloned())
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        if value.len() > MAX_SECRET_BYTES {
            return Err("secret value exceeds 64 KiB".into());
        }
        let _guard = self
            .io_lock
            .lock()
            .map_err(|_| "本地密钥库锁已损坏".to_string())?;
        let mut payload = self.load_payload()?;
        payload
            .secrets
            .insert(account.to_string(), value.to_string());
        self.save_payload(&payload)
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        let _guard = self
            .io_lock
            .lock()
            .map_err(|_| "本地密钥库锁已损坏".to_string())?;
        let mut payload = self.load_payload()?;
        if payload.secrets.remove(account).is_some() {
            self.save_payload(&payload)?;
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn validate_payload(payload: &VaultPayload) -> Result<(), String> {
    if payload.secrets.len() > MAX_VAULT_ENTRIES {
        return Err("本地密钥库条目超过上限".into());
    }
    for (account, secret) in &payload.secrets {
        if !account.starts_with(ACCOUNT_PREFIX) || account.len() != ACCOUNT_PREFIX.len() + 64 {
            return Err("本地密钥库包含无效账户标识".into());
        }
        if secret.is_empty() || secret.len() > MAX_SECRET_BYTES {
            return Err("本地密钥库包含无效密钥值".into());
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn ensure_private_directory(path: &Path) -> Result<(), String> {
    if path.exists() {
        reject_symlink(path, "secret directory")?;
        if !path.is_dir() {
            return Err("本地密钥库路径不是目录".into());
        }
    } else {
        fs::create_dir_all(path).map_err(|error| format!("无法创建本地密钥库目录：{error}"))?;
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("无法限制本地密钥库目录权限：{error}"))
}

#[cfg(target_os = "macos")]
fn reject_symlink(path: &Path, label: &str) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("无法检查{label}：{error}"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("拒绝使用符号链接形式的{label}"));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn enforce_private_file_permissions(path: &Path) -> Result<(), String> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("无法限制本地密钥库文件权限：{error}"))
}

#[cfg(target_os = "macos")]
fn random_vault_key() -> [u8; 32] {
    let first = *uuid::Uuid::new_v4().as_bytes();
    let second = *uuid::Uuid::new_v4().as_bytes();
    let mut key = [0_u8; 32];
    key[..16].copy_from_slice(&first);
    key[16..].copy_from_slice(&second);
    key
}

#[cfg(target_os = "macos")]
fn random_vault_nonce() -> [u8; 24] {
    let first = *uuid::Uuid::new_v4().as_bytes();
    let second = *uuid::Uuid::new_v4().as_bytes();
    let mut nonce = [0_u8; 24];
    nonce[..16].copy_from_slice(&first);
    nonce[16..].copy_from_slice(&second[..8]);
    nonce
}

#[cfg(target_os = "macos")]
fn load_or_create_vault_key(root: &Path) -> Result<[u8; 32], String> {
    let path = root.join("vault.key");
    if path.exists() {
        reject_symlink(&path, "vault key")?;
        enforce_private_file_permissions(&path)?;
        let bytes =
            fs::read(&path).map_err(|error| format!("无法读取本地密钥库主密钥：{error}"))?;
        if bytes.len() != 32 {
            return Err("本地密钥库主密钥长度无效".into());
        }
        let mut key = [0_u8; 32];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }

    if root.join("vault.v1.json").exists() {
        return Err("本地密钥库主密钥缺失，拒绝覆盖现有密文".into());
    }
    let key = random_vault_key();
    let temp_path = root.join(format!(".vault-key.{}.tmp", uuid::Uuid::new_v4()));
    let write_result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp_path)
            .map_err(|error| format!("无法创建本地密钥库主密钥：{error}"))?;
        file.write_all(&key)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("无法写入本地密钥库主密钥：{error}"))?;
        fs::rename(&temp_path, &path)
            .map_err(|error| format!("无法原子保存本地密钥库主密钥：{error}"))?;
        enforce_private_file_permissions(&path)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result?;
    Ok(key)
}

#[cfg(target_os = "macos")]
static MACOS_SECRET_STORE: OnceLock<EncryptedFileStore> = OnceLock::new();

pub fn initialize_secret_store(app_config_dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let root = app_config_dir.join("secrets");
        if let Some(existing) = MACOS_SECRET_STORE.get() {
            return if existing.root == root {
                Ok(())
            } else {
                Err("本地密钥库已使用其他目录初始化".into())
            };
        }
        let store = EncryptedFileStore::new(root)?;
        MACOS_SECRET_STORE
            .set(store)
            .map_err(|_| "本地密钥库重复初始化".to_string())?;
    }
    Ok(())
}

fn platform_secret_store() -> Result<&'static dyn SecretStore, String> {
    #[cfg(target_os = "macos")]
    {
        MACOS_SECRET_STORE
            .get()
            .map(|store| store as &dyn SecretStore)
            .ok_or_else(|| "本地密钥库尚未初始化".to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(&KEYRING_STORE)
    }
}

pub fn get_provider_secret(
    provider_id: &str,
    endpoint: &str,
    field: &str,
) -> Result<Option<String>, String> {
    get_provider_secret_from(platform_secret_store()?, provider_id, endpoint, field)
}

pub fn has_provider_secret(provider_id: &str, endpoint: &str, field: &str) -> Result<bool, String> {
    Ok(get_provider_secret(provider_id, endpoint, field)?
        .filter(|secret| !secret.is_empty())
        .is_some())
}

pub fn set_provider_secret(
    provider_id: &str,
    endpoint: &str,
    field: &str,
    value: &str,
) -> Result<(), String> {
    set_provider_secret_in(
        platform_secret_store()?,
        provider_id,
        endpoint,
        field,
        value,
    )
}

pub fn delete_provider_secret(
    provider_id: &str,
    endpoint: &str,
    field: &str,
) -> Result<(), String> {
    delete_provider_secret_from(platform_secret_store()?, provider_id, endpoint, field)
}

fn get_provider_secret_from(
    store: &dyn SecretStore,
    provider_id: &str,
    endpoint: &str,
    field: &str,
) -> Result<Option<String>, String> {
    let account = provider_secret_account(provider_id, endpoint, field)?;
    store.get(&account)
}

fn set_provider_secret_in(
    store: &dyn SecretStore,
    provider_id: &str,
    endpoint: &str,
    field: &str,
    value: &str,
) -> Result<(), String> {
    if value.is_empty() {
        return Err("secret value must not be empty".into());
    }
    if value.len() > MAX_SECRET_BYTES {
        return Err("secret value exceeds 64 KiB".into());
    }

    let account = provider_secret_account(provider_id, endpoint, field)?;
    store.set(&account, value)?;

    // Legacy provider-only credentials are never read. Remove them after the user
    // explicitly saves a replacement for the current endpoint.
    let legacy_account = legacy_provider_secret_account(provider_id, field)?;
    store.delete(&legacy_account)
}

fn delete_provider_secret_from(
    store: &dyn SecretStore,
    provider_id: &str,
    endpoint: &str,
    field: &str,
) -> Result<(), String> {
    let account = provider_secret_account(provider_id, endpoint, field)?;
    store.delete(&account)?;

    // Deleting a credential also clears its now-unused legacy provider-only entry.
    let legacy_account = legacy_provider_secret_account(provider_id, field)?;
    store.delete(&legacy_account)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemoryStore {
        values: RefCell<HashMap<String, String>>,
    }

    impl SecretStore for MemoryStore {
        fn get(&self, account: &str) -> Result<Option<String>, String> {
            Ok(self.values.borrow().get(account).cloned())
        }

        fn set(&self, account: &str, value: &str) -> Result<(), String> {
            self.values
                .borrow_mut()
                .insert(account.to_string(), value.to_string());
            Ok(())
        }

        fn delete(&self, account: &str) -> Result<(), String> {
            self.values.borrow_mut().remove(account);
            Ok(())
        }
    }

    #[test]
    fn account_normalizes_endpoint_whitespace_and_trailing_slashes() {
        let canonical =
            provider_secret_account("deepseek", "https://api.example.com/v1", "apiKey").unwrap();
        let equivalent = provider_secret_account(
            " deepseek ",
            "  https://api.example.com/v1///  ",
            " apiKey ",
        )
        .unwrap();

        assert_eq!(canonical, equivalent);
        assert!(canonical.starts_with(ACCOUNT_PREFIX));
        assert_eq!(canonical.len(), ACCOUNT_PREFIX.len() + 64);
    }

    #[test]
    fn account_isolates_endpoint_provider_and_field() {
        let base = provider_secret_account("deepseek", "https://a.example/v1", "apiKey").unwrap();
        let endpoint_b =
            provider_secret_account("deepseek", "https://b.example/v1", "apiKey").unwrap();
        let provider_b =
            provider_secret_account("gemini", "https://a.example/v1", "apiKey").unwrap();
        let field_b =
            provider_secret_account("deepseek", "https://a.example/v1", "region").unwrap();

        assert_ne!(base, endpoint_b);
        assert_ne!(base, provider_b);
        assert_ne!(base, field_b);
    }

    #[test]
    fn legacy_provider_only_secret_is_never_read() {
        let store = MemoryStore::default();
        let legacy = legacy_provider_secret_account("deepseek", "apiKey").unwrap();
        store.set(&legacy, "legacy-secret").unwrap();

        let loaded =
            get_provider_secret_from(&store, "deepseek", "https://api.example.com/v1", "apiKey")
                .unwrap();

        assert_eq!(loaded, None);
    }

    #[test]
    fn saving_endpoint_bound_secret_cleans_legacy_entry() {
        let store = MemoryStore::default();
        let legacy = legacy_provider_secret_account("deepseek", "apiKey").unwrap();
        store.set(&legacy, "legacy-secret").unwrap();

        set_provider_secret_in(
            &store,
            "deepseek",
            "https://api.example.com/v1/",
            "apiKey",
            "current-secret",
        )
        .unwrap();

        assert_eq!(store.get(&legacy).unwrap(), None);
        assert_eq!(
            get_provider_secret_from(&store, "deepseek", "https://api.example.com/v1", "apiKey",)
                .unwrap()
                .as_deref(),
            Some("current-secret")
        );
    }

    #[test]
    fn delete_removes_current_and_legacy_entries_without_touching_other_endpoint() {
        let store = MemoryStore::default();
        let legacy = legacy_provider_secret_account("deepseek", "apiKey").unwrap();
        let endpoint_a =
            provider_secret_account("deepseek", "https://a.example/v1", "apiKey").unwrap();
        let endpoint_b =
            provider_secret_account("deepseek", "https://b.example/v1", "apiKey").unwrap();
        store.set(&legacy, "legacy").unwrap();
        store.set(&endpoint_a, "secret-a").unwrap();
        store.set(&endpoint_b, "secret-b").unwrap();

        delete_provider_secret_from(&store, "deepseek", "https://a.example/v1/", "apiKey").unwrap();

        assert_eq!(store.get(&legacy).unwrap(), None);
        assert_eq!(store.get(&endpoint_a).unwrap(), None);
        assert_eq!(store.get(&endpoint_b).unwrap().as_deref(), Some("secret-b"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn encrypted_file_store_roundtrip_hides_secret_and_locks_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("secrets");
        let account =
            provider_secret_account("deepseek", "https://api.example.com/v1", "apiKey").unwrap();
        let store = EncryptedFileStore::new(root.clone()).unwrap();

        store.set(&account, "super-secret-value").unwrap();
        assert_eq!(
            store.get(&account).unwrap().as_deref(),
            Some("super-secret-value")
        );

        let serialized = std::fs::read_to_string(root.join("vault.v1.json")).unwrap();
        assert!(!serialized.contains("super-secret-value"));
        assert!(!serialized.contains(&account));
        assert_eq!(
            std::fs::metadata(root.join("vault.key"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(root.join("vault.v1.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );

        drop(store);
        let reopened = EncryptedFileStore::new(root).unwrap();
        assert_eq!(
            reopened.get(&account).unwrap().as_deref(),
            Some("super-secret-value")
        );
        reopened.delete(&account).unwrap();
        assert_eq!(reopened.get(&account).unwrap(), None);
    }
}
