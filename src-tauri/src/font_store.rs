use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedFont {
    pub id: String,
    pub family: String,
    pub active: bool,
}
pub struct Store {
    pub root: PathBuf,
    pub user: PathBuf,
}
impl Store {
    pub fn list(&self) -> Result<Vec<ManagedFont>, String> {
        let path = self.root.join("manifest.json");
        if !path.exists() {
            return Ok(vec![]);
        }
        let data = fs::read(&path).map_err(|e| e.to_string())?;
        if data.len() > 4 * 1024 * 1024 {
            return Err("font_manifest_invalid".into());
        }
        let fonts: Vec<ManagedFont> =
            serde_json::from_slice(&data).map_err(|_| "font_manifest_invalid")?;
        let mut ids = std::collections::BTreeSet::new();
        if fonts.iter().any(|f| {
            f.id.len() != 64
                || !f.id.bytes().all(|b| b.is_ascii_hexdigit())
                || !ids.insert(f.id.clone())
        }) {
            return Err("font_manifest_invalid".into());
        }
        Ok(fonts)
    }
    fn save(&self, fonts: &[ManagedFont]) -> Result<(), String> {
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        font_core::storage::write_bytes(
            &self.root.join("manifest.json"),
            &serde_json::to_vec(fonts).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
    fn archive(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.ttf"))
    }
    fn target(&self, id: &str) -> PathBuf {
        self.user.join(format!("Fonts-{id}.ttf"))
    }
    pub fn add(&self, bytes: &[u8], family: String) -> Result<String, String> {
        let id = font_id(bytes);
        let mut fonts = self.list()?;
        if fonts.iter().any(|f| f.id == id) {
            return Ok(id);
        }
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        font_core::storage::write_bytes(&self.archive(&id), bytes).map_err(|e| e.to_string())?;
        fonts.push(ManagedFont {
            id: id.clone(),
            family,
            active: false,
        });
        self.save(&fonts)?;
        Ok(id)
    }
    pub fn set_active(
        &self,
        id: &str,
        enable: bool,
        activate: impl Fn(&Path, bool) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut fonts = self.list()?;
        let item = fonts
            .iter_mut()
            .find(|f| f.id == id)
            .ok_or("font_not_found")?;
        if item.active == enable {
            return Ok(());
        }
        let target = self.target(id);
        let archive = self.archive(id);
        let data = font_core::font_io::read(&archive)?;
        if font_id(&data) != id {
            return Err("font_changed_externally".into());
        }
        if enable {
            fs::create_dir_all(&self.user).map_err(|e| e.to_string())?;
            let mut f = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)
                .map_err(|e| e.to_string())?;
            if let Err(e) = f.write_all(&data).and_then(|_| f.sync_all()) {
                drop(f);
                let _ = fs::remove_file(&target);
                return Err(e.to_string());
            }
            drop(f);
            if let Err(e) = activate(&target, true) {
                let _ = fs::remove_file(&target);
                return Err(e);
            }
            item.active = true;
            if let Err(e) = self.save(&fonts) {
                let _ = activate(&target, false);
                let _ = fs::remove_file(&target);
                return Err(e);
            }
        } else {
            if !target.exists() {
                item.active = false;
                return self.save(&fonts);
            }
            let installed = font_core::font_io::read(&target)?;
            if font_id(&installed) != id {
                return Err("font_changed_externally".into());
            }
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            activate(&target, false)?;
            // Keep an exact recovery copy in the archive; never touch unrelated installed fonts.
            if let Err(e) = fs::remove_file(&target) {
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                let _ = activate(&target, true);
                return Err(e.to_string());
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            if let Err(e) = activate(&target, false) {
                let _ = fs::write(&target, &data);
                let _ = activate(&target, true);
                return Err(e);
            }
            item.active = false;
            if let Err(e) = self.save(&fonts) {
                let _ = fs::write(&target, &data);
                let _ = activate(&target, true);
                return Err(e);
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reversible_install_and_external_change_protection() {
        let dir = std::env::temp_dir().join(format!("fonts-store-{}", std::process::id()));
        let store = Store {
            root: dir.join("archive"),
            user: dir.join("user"),
        };
        let id = store.add(b"test-font", "Test".into()).unwrap();
        store.set_active(&id, true, |_, _| Ok(())).unwrap();
        assert!(store.list().unwrap()[0].active);
        store.set_active(&id, false, |_, _| Ok(())).unwrap();
        assert!(!store.target(&id).exists());
        assert!(store.archive(&id).exists());
        assert!(store
            .set_active(&id, true, |_, _| Err("registration".into()))
            .is_err());
        assert!(!store.target(&id).exists());
        store.set_active(&id, true, |_, _| Ok(())).unwrap();
        fs::write(store.target(&id), b"modified").unwrap();
        assert_eq!(
            store.set_active(&id, false, |_, _| Ok(())),
            Err("font_changed_externally".into())
        );
        assert_eq!(fs::read(store.target(&id)).unwrap(), b"modified");
        fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(all(test, target_os = "macos"))]
mod native_test {
    use super::*;
    #[test]
    #[ignore = "Temporarily registers a unique test font in macOS; requires desktop permission"]
    fn coretext_register_and_unregister_managed_font() {
        let directory =
            std::env::temp_dir().join(format!("fonts-native-activation-{}", std::process::id()));
        let store = Store {
            root: directory.join("archive"),
            user: directory.join("user-fonts"),
        };
        let name = format!("Fonts Native QA {}", std::process::id());
        let document = font_core::FontDocument::new(&name, "A😀").unwrap();
        let bytes = font_core::font_io::export(&document).unwrap();
        let id = store.add(&bytes, name).unwrap();
        let enabled = store.set_active(&id, true, crate::font_activation::change);
        if enabled.is_ok() {
            let disabled = store.set_active(&id, false, crate::font_activation::change);
            // Retry cleanup before reporting any failure; the test never touches existing fonts.
            if disabled.is_err() {
                let _ = crate::font_activation::change(&store.target(&id), false);
            }
            assert!(disabled.is_ok(), "{disabled:?}");
            assert!(!store.list().unwrap()[0].active);
        }
        let _ = fs::remove_dir_all(directory);
        assert!(enabled.is_ok(), "{enabled:?}");
    }
}

// Preserve the lowercase SHA-256 IDs used by existing manifests and filenames.
fn font_id(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut id = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut id, "{byte:02x}").expect("writing to String cannot fail");
    }
    id
}

#[cfg(test)]
mod dependency_compatibility_tests {
    #[test]
    fn sha256_ids_keep_the_existing_manifest_format() {
        assert_eq!(
            super::font_id(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
