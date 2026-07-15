use keyring::Entry;
use sha2::{Digest, Sha256};

const KEYRING_SERVICE: &str = "com.gravitypoet.finalsub";
const ACCOUNT_PREFIX: &str = "translate:v2:";

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

struct KeyringStore;

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

pub fn get_provider_secret(
    provider_id: &str,
    endpoint: &str,
    field: &str,
) -> Result<Option<String>, String> {
    get_provider_secret_from(&KeyringStore, provider_id, endpoint, field)
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
    set_provider_secret_in(&KeyringStore, provider_id, endpoint, field, value)
}

pub fn delete_provider_secret(
    provider_id: &str,
    endpoint: &str,
    field: &str,
) -> Result<(), String> {
    delete_provider_secret_from(&KeyringStore, provider_id, endpoint, field)
}

fn get_provider_secret_from(
    store: &impl SecretStore,
    provider_id: &str,
    endpoint: &str,
    field: &str,
) -> Result<Option<String>, String> {
    let account = provider_secret_account(provider_id, endpoint, field)?;
    store.get(&account)
}

fn set_provider_secret_in(
    store: &impl SecretStore,
    provider_id: &str,
    endpoint: &str,
    field: &str,
    value: &str,
) -> Result<(), String> {
    if value.is_empty() {
        return Err("secret value must not be empty".into());
    }

    let account = provider_secret_account(provider_id, endpoint, field)?;
    store.set(&account, value)?;

    // Legacy provider-only credentials are never read. Remove them after the user
    // explicitly saves a replacement for the current endpoint.
    let legacy_account = legacy_provider_secret_account(provider_id, field)?;
    store.delete(&legacy_account)
}

fn delete_provider_secret_from(
    store: &impl SecretStore,
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
}
