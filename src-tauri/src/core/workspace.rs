use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

fn path(root: &Path, key: &str) -> Result<PathBuf, String> {
    if !matches!(key, "proofread" | "compose") {
        return Err("Invalid workspace key".into());
    }
    Ok(root.join(format!("{key}-workspace.json")))
}

pub fn load(root: &Path, key: &str) -> Result<Option<String>, String> {
    let path = path(root, key)?;
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(path).map_err(|error| error.to_string())?;
    validate(&data)?;
    Ok(Some(data))
}

fn validate(data: &str) -> Result<(), String> {
    if data.len() > 100 * 1024 * 1024 {
        return Err("Workspace draft exceeds 100 MB".into());
    }
    let value: serde_json::Value = serde_json::from_str(data).map_err(|error| error.to_string())?;
    if !value.is_object() {
        return Err("Workspace draft must be an object".into());
    }
    Ok(())
}

pub fn save(root: &Path, key: &str, data: &str) -> Result<(), String> {
    let path = path(root, key)?;
    validate(data)?;
    fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let temp = root.join(format!(".{key}-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(data.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result.map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn draft_roundtrip_and_invalid_write_preserve_existing() {
        let root = tempfile::tempdir().unwrap();
        save(
            root.path(),
            "proofread",
            r#"{"version":1,"cues":["draft"]}"#,
        )
        .unwrap();
        assert!(save(root.path(), "proofread", "not-json").is_err());
        assert!(load(root.path(), "proofread")
            .unwrap()
            .unwrap()
            .contains("draft"));
        assert!(save(root.path(), "../settings", "{}").is_err());
        assert!(save(root.path(), "compose", "[]").is_err());
        assert_eq!(load(root.path(), "compose").unwrap(), None);
    }
}
