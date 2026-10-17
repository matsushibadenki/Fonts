//! Atomic recovery checkpoints, isolated and locked per running app instance.
use font_core::{storage, FontDocument};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub family: String,
    pub updated: u64,
    pub origin: Option<PathBuf>,
    pub epoch: u64,
    pub revision: u64,
    pub file: String,
    pub version: u32,
}
pub struct Store {
    root: PathBuf,
    id: String,
    _lock: File,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 100 && id.bytes().all(|b| b.is_ascii_digit() || b == b'-')
}
fn manifest(dir: &Path) -> Result<Candidate, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    File::open(dir.join("checkpoint.json"))
        .map_err(|e| e.to_string())?
        .take(16385)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16384 {
        return Err("recovery_invalid".into());
    }
    let c: Candidate = serde_json::from_slice(&bytes).map_err(|_| "recovery_invalid")?;
    if c.version != 1
        || !valid_id(&c.id)
        || c.file != format!("{}-{}.fontproj.json", c.epoch, c.revision)
        || c.family.len() > 1024
    {
        return Err("recovery_invalid".into());
    }
    Ok(c)
}
fn lock(dir: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("session.lock"))
        .map_err(|e| e.to_string())?;
    file.try_lock().map_err(|_| "recovery_in_use".to_string())?;
    Ok(file)
}
impl Store {
    pub fn new(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        for n in 0..1000 {
            let id = format!("{}-{}-{n}", now(), std::process::id());
            let dir = root.join(&id);
            match fs::create_dir(&dir) {
                Ok(()) => {
                    return Ok(Self {
                        _lock: lock(&dir)?,
                        root,
                        id,
                    })
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        Err("recovery_unavailable".into())
    }
    pub fn checkpoint(
        &self,
        doc: &FontDocument,
        epoch: u64,
        revision: u64,
        origin: Option<PathBuf>,
    ) -> Result<(), String> {
        let dir = self.root.join(&self.id);
        let previous = manifest(&dir).ok();
        let c = Candidate {
            id: self.id.clone(),
            family: doc.family.clone(),
            updated: now(),
            origin,
            epoch,
            revision,
            file: format!("{epoch}-{revision}.fontproj.json"),
            version: 1,
        };
        storage::save(&dir.join(&c.file), doc).map_err(|e| e.to_string())?;
        storage::write_bytes(
            &dir.join("checkpoint.json"),
            &serde_json::to_vec(&c).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if let Some(old) = previous.filter(|p| p.file != c.file) {
            let _ = fs::remove_file(dir.join(old.file));
        }
        Ok(())
    }
    pub fn clear(&self) -> Result<(), String> {
        Self::clear_dir(&self.root.join(&self.id))
    }
    fn clear_dir(dir: &Path) -> Result<(), String> {
        let old = manifest(dir).ok();
        match fs::remove_file(dir.join("checkpoint.json")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        if let Some(old) = old {
            let _ = fs::remove_file(dir.join(old.file));
        }
        Ok(())
    }
    pub fn candidates(&self) -> Result<(Vec<Candidate>, usize), String> {
        let mut found = Vec::new();
        let mut invalid = 0;
        for entry in fs::read_dir(&self.root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let id = entry.file_name().to_string_lossy().into_owned();
            if id == self.id
                || !valid_id(&id)
                || !entry.file_type().map_err(|e| e.to_string())?.is_dir()
            {
                continue;
            }
            let dir = entry.path();
            let Ok(_lock) = lock(&dir) else {
                continue;
            };
            if !dir.join("checkpoint.json").exists() {
                continue;
            }
            match manifest(&dir) {
                Ok(c) if c.id == id => found.push(c),
                _ => invalid += 1,
            }
        }
        found.sort_by_key(|c| std::cmp::Reverse(c.updated));
        Ok((found, invalid))
    }
    pub fn candidate(&self, id: &str) -> Result<(FontDocument, File), String> {
        let (dir, guard) = self.access(id)?;
        let c = manifest(&dir)?;
        if c.id != id {
            return Err("recovery_invalid".into());
        }
        Ok((
            storage::load(&dir.join(c.file)).map_err(|e| e.to_string())?,
            guard,
        ))
    }
    fn access(&self, id: &str) -> Result<(PathBuf, File), String> {
        if !valid_id(id) || id == self.id {
            return Err("recovery_invalid".into());
        }
        let dir = self.root.join(id);
        if fs::symlink_metadata(&dir)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("recovery_invalid".into());
        }
        let guard = lock(&dir)?;
        Ok((dir, guard))
    }
    pub fn dismiss(&self, id: &str) -> Result<(), String> {
        let (dir, _guard) = self.access(id)?;
        Self::clear_dir(&dir)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "fonts-recovery-{}-{}-{label}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn embedded_reference_image_survives_crash_without_original_file() {
        use font_core::{reference_images::ImageAsset, Editor};
        let root = root("images");
        let store = Store::new(root.clone()).unwrap();
        let source = root.join("source.png");
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[255, 0, 0, 255, 0, 255, 0, 64])
                .unwrap();
        }
        fs::write(&source, bytes).unwrap();
        let mut editor = Editor::new(FontDocument::new("Image recovery", "A").unwrap()).unwrap();
        editor
            .place_reference_image(0, 1, ImageAsset::load(&source).unwrap())
            .unwrap();
        let mut image = editor.document().reference_images.for_glyph(1)[0].clone();
        image.center = [123., -50.];
        image.angle = 37.;
        image.opacity = 0.;
        image.visible = false;
        image.locked = true;
        editor.update_reference_image(1, 1, image).unwrap();
        store.checkpoint(editor.document(), 2, 2, None).unwrap();
        fs::remove_file(source).unwrap();
        let mut invalid = editor.document().clone();
        invalid.glyphs.remove(&1);
        assert!(store.checkpoint(&invalid, 2, 3, None).is_err());
        let id = store.id.clone();
        drop(store);
        let next = Store::new(root.clone()).unwrap();
        assert_eq!(next.candidates().unwrap().0[0].revision, 2);
        let (recovered, lease) = next.candidate(&id).unwrap();
        assert_eq!(&recovered, editor.document());
        assert_eq!(
            recovered
                .reference_images
                .assets()
                .values()
                .next()
                .unwrap()
                .rgba(),
            &[255, 0, 0, 255, 0, 255, 0, 64]
        );
        drop(lease);
        drop(next);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn crashes_leave_candidates_live_instances_are_excluded_and_sources_are_not_written() {
        let root = root("crash");
        let a = Store::new(root.clone()).unwrap();
        let b = Store::new(root.clone()).unwrap();
        let doc = FontDocument::new("Recover 日本語 中文", "A中").unwrap();
        a.checkpoint(&doc, 1, 4, Some(root.join("original.json")))
            .unwrap();
        assert!(b.candidates().unwrap().0.is_empty());
        let id = a.id.clone();
        drop(a);
        let candidates = b.candidates().unwrap().0;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].revision, 4);
        assert!(!root.join("original.json").exists());
        let (restored, lease) = b.candidate(&id).unwrap();
        assert_eq!(restored, doc);
        assert!(b.dismiss(&id).is_err());
        drop(lease);
        b.dismiss(&id).unwrap();
        assert!(b.candidates().unwrap().0.is_empty());
        drop(b);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn atomic_generation_clear_invalid_and_failed_checkpoint() {
        let root = root("atomic");
        let a = Store::new(root.clone()).unwrap();
        let doc = FontDocument::new("Recovery", "A").unwrap();
        a.checkpoint(&doc, 2, 1, None).unwrap();
        let mut bad = doc.clone();
        bad.family.clear();
        assert!(a.checkpoint(&bad, 2, 2, None).is_err());
        assert_eq!(manifest(&root.join(&a.id)).unwrap().revision, 1);
        a.checkpoint(&doc, 2, 2, None).unwrap();
        assert!(!root.join(&a.id).join("2-1.fontproj.json").exists());
        assert!(a.candidate("../escape").is_err());
        a.clear().unwrap();
        assert!(!root.join(&a.id).join("checkpoint.json").exists());
        drop(a);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn damaged_payload_and_manifest_never_replace_or_delete_the_source() {
        let root = root("corrupt");
        let a = Store::new(root.clone()).unwrap();
        let source = root.join("original.fontproj.json");
        fs::write(&source, b"original unchanged").unwrap();
        a.checkpoint(
            &FontDocument::new("Corrupt QA", "A").unwrap(),
            3,
            8,
            Some(source.clone()),
        )
        .unwrap();
        let id = a.id.clone();
        let dir = root.join(&id);
        drop(a);
        let b = Store::new(root.clone()).unwrap();
        fs::write(dir.join("3-8.fontproj.json"), b"truncated").unwrap();
        assert!(b.candidate(&id).is_err());
        assert_eq!(b.candidates().unwrap().0.len(), 1);
        let mut c = manifest(&dir).unwrap();
        c.file = "../../original.fontproj.json".into();
        fs::write(dir.join("checkpoint.json"), serde_json::to_vec(&c).unwrap()).unwrap();
        assert!(b.candidate(&id).is_err());
        assert_eq!(b.candidates().unwrap().1, 1);
        assert_eq!(fs::read(source).unwrap(), b"original unchanged");
        drop(b);
        fs::remove_dir_all(root).unwrap();
    }
}
