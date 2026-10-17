//! Embedded, immutable reference images. All pixels and document mutations stay in Rust.
//! Call image loading on a worker; only a fully decoded and validated asset may be committed.
use crate::{Change, Editor, Error, Glyph, GlyphId, Point, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
    path::Path,
    sync::Arc,
};

pub const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_DIMENSION: u32 = 8192;
pub const MAX_PIXELS: u64 = 16_000_000;
pub const MAX_ASSET_BYTES: usize = 96 * 1024 * 1024;
pub const MAX_RGBA_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_ASSETS: usize = 256;
pub const MAX_PLACEMENTS: usize = 8192;
pub const MAX_PER_GLYPH: usize = 64;
const MAX_ID: u64 = (1 << 53) - 1;
const MAX_COORDINATE: f64 = 1_000_000.;

struct AssetData {
    id: String,
    encoded: Vec<u8>,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}
/// Cheap clones share both source bytes and decoded RGBA pixels, including across undo history.
#[derive(Clone)]
pub struct ImageAsset(Arc<AssetData>);
impl std::fmt::Debug for ImageAsset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageAsset")
            .field("id", &self.id())
            .field("width", &self.width())
            .field("height", &self.height())
            .field("encoded_bytes", &self.encoded().len())
            .finish()
    }
}
impl PartialEq for ImageAsset {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.id() == other.id()
    }
}
fn asset_id(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn image_error(error: image::ImageError) -> Error {
    if matches!(error, image::ImageError::Limits(_)) {
        Error::TooLarge
    } else {
        Error::InvalidFile
    }
}
fn reader(bytes: &[u8]) -> Result<ImageReader<Cursor<&[u8]>>> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Error::TooLarge);
    }
    let format = image::guess_format(bytes).map_err(image_error)?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg) {
        return Err(Error::InvalidFile);
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader)
}
fn dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    let (w, h) = reader(bytes)?.into_dimensions().map_err(image_error)?;
    if w == 0 || h == 0 {
        return Err(Error::InvalidFile);
    }
    if w > MAX_DIMENSION || h > MAX_DIMENSION || u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(Error::TooLarge);
    }
    Ok((w, h))
}
impl ImageAsset {
    pub fn load(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(|_| Error::Io)?;
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Io)?;
        Self::from_bytes(bytes)
    }
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let (width, height) = dimensions(&bytes)?;
        // Header dimensions are checked before allocating/decoding the raster.
        let pixels = reader(&bytes)?.decode().map_err(image_error)?.to_rgba8();
        if pixels.dimensions() != (width, height) {
            return Err(Error::InvalidFile);
        }
        Ok(Self(Arc::new(AssetData {
            id: asset_id(&bytes),
            encoded: bytes,
            width,
            height,
            rgba: pixels.into_raw(),
        })))
    }
    pub fn id(&self) -> &str {
        &self.0.id
    }
    pub fn width(&self) -> u32 {
        self.0.width
    }
    pub fn height(&self) -> u32 {
        self.0.height
    }
    pub fn encoded(&self) -> &[u8] {
        &self.0.encoded
    }
    pub fn rgba(&self) -> &[u8] {
        &self.0.rgba
    }
}
impl Serialize for ImageAsset {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(self.encoded()))
    }
}

/// Font units, Y-up, counterclockwise degrees. Vector order is back-to-front.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceImage {
    pub id: u64,
    pub asset_id: String,
    pub center: Point,
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    pub opacity: f64,
    pub visible: bool,
    pub locked: bool,
}
impl ReferenceImage {
    pub fn validate(&self) -> Result<()> {
        if self.id == 0
            || self.id >= MAX_ID
            || self
                .center
                .iter()
                .any(|v| !v.is_finite() || v.abs() > MAX_COORDINATE)
            || [self.width, self.height]
                .iter()
                .any(|v| !v.is_finite() || *v < 0.001 || *v > MAX_COORDINATE)
            || !self.angle.is_finite()
            || self.angle.abs() > 360.
            || !self.opacity.is_finite()
            || !(0. ..=1.).contains(&self.opacity)
        {
            return Err(Error::InvalidGeometry);
        }
        Ok(())
    }
    /// Normalized image coordinates: (-.5,-.5) bottom-left, (.5,.5) top-right.
    pub fn to_document(&self, local: Point) -> Point {
        let (s, c) = self.angle.to_radians().sin_cos();
        let [x, y] = [local[0] * self.width, local[1] * self.height];
        [
            self.center[0] + c * x - s * y,
            self.center[1] + s * x + c * y,
        ]
    }
    pub fn to_local(&self, point: Point) -> Point {
        let (s, c) = self.angle.to_radians().sin_cos();
        let [x, y] = [point[0] - self.center[0], point[1] - self.center[1]];
        [(c * x + s * y) / self.width, (-s * x + c * y) / self.height]
    }
    pub fn hit_test(&self, point: Point) -> bool {
        self.visible && !self.locked && self.to_local(point).iter().all(|v| v.abs() <= 0.5)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReferenceImages {
    version: u32,
    next_id: u64,
    assets: BTreeMap<String, ImageAsset>,
    placements: BTreeMap<GlyphId, Vec<ReferenceImage>>,
}
impl Default for ReferenceImages {
    fn default() -> Self {
        Self {
            version: 1,
            next_id: 1,
            assets: BTreeMap::new(),
            placements: BTreeMap::new(),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredImages {
    version: u32,
    next_id: u64,
    assets: BTreeMap<String, String>,
    placements: BTreeMap<GlyphId, Vec<ReferenceImage>>,
}
impl<'de> Deserialize<'de> for ReferenceImages {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let stored = StoredImages::deserialize(deserializer)?;
        Self::from_stored(stored).map_err(serde::de::Error::custom)
    }
}
impl ReferenceImages {
    fn from_stored(stored: StoredImages) -> Result<Self> {
        if stored.version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        if stored.assets.len() > MAX_ASSETS
            || stored
                .assets
                .values()
                .any(|s| s.len() > MAX_FILE_BYTES.div_ceil(3) * 4)
            || stored.assets.values().map(String::len).sum::<usize>()
                > MAX_ASSET_BYTES.div_ceil(3) * 4 + MAX_ASSETS * 4
        {
            return Err(Error::TooLarge);
        }
        let mut result = Self {
            version: stored.version,
            next_id: stored.next_id,
            assets: BTreeMap::new(),
            placements: stored.placements,
        };
        // Preflight every compressed asset and the aggregate pixel budget before raster decoding.
        let mut decoded = BTreeMap::new();
        let mut raw_bytes = 0;
        let mut rgba_bytes = 0;
        for (id, text) in stored.assets {
            let bytes = STANDARD.decode(text).map_err(|_| Error::InvalidFile)?;
            let (w, h) = dimensions(&bytes)?;
            raw_bytes += bytes.len();
            rgba_bytes += u64::from(w) * u64::from(h) * 4;
            validate_budget(decoded.len() + 1, raw_bytes, rgba_bytes)?;
            if id != asset_id(&bytes) {
                return Err(Error::InvalidFile);
            }
            decoded.insert(id, bytes);
        }
        for (id, bytes) in decoded {
            result.assets.insert(id, ImageAsset::from_bytes(bytes)?);
        }
        result.validate_structure()?;
        Ok(result)
    }
    pub(crate) fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn is_empty(&self) -> bool {
        self.placements.is_empty() && self.assets.is_empty()
    }
    pub fn assets(&self) -> &BTreeMap<String, ImageAsset> {
        &self.assets
    }
    pub fn for_glyph(&self, glyph: GlyphId) -> &[ReferenceImage] {
        self.placements
            .get(&glyph)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    fn validate_structure(&self) -> Result<()> {
        if self.version != 1 {
            return Err(Error::UnsupportedVersion);
        }
        if self.next_id == 0 || self.next_id > MAX_ID {
            return Err(Error::InvalidDocument);
        }
        if self.placements.values().map(Vec::len).sum::<usize>() > MAX_PLACEMENTS
            || self.placements.values().any(|v| v.len() > MAX_PER_GLYPH)
        {
            return Err(Error::TooLarge);
        }
        let mut ids = BTreeSet::new();
        let mut used = BTreeSet::new();
        for images in self.placements.values() {
            if images.is_empty() {
                return Err(Error::InvalidDocument);
            }
            for image in images {
                image.validate()?;
                if !ids.insert(image.id)
                    || image.id >= self.next_id
                    || !self.assets.contains_key(&image.asset_id)
                {
                    return Err(Error::InvalidDocument);
                }
                used.insert(&image.asset_id);
            }
        }
        if used.len() != self.assets.len() {
            return Err(Error::InvalidDocument);
        }
        check_budget(self.assets.values())
    }
    pub(crate) fn validate(&self, glyphs: &BTreeMap<GlyphId, Glyph>) -> Result<()> {
        self.validate_structure()?;
        if self.placements.keys().any(|id| !glyphs.contains_key(id)) {
            return Err(Error::MissingGlyph);
        }
        Ok(())
    }
    fn prune(&mut self) {
        self.placements.retain(|_, images| !images.is_empty());
        let used: BTreeSet<_> = self
            .placements
            .values()
            .flatten()
            .map(|p| &p.asset_id)
            .collect();
        self.assets.retain(|id, _| used.contains(id));
    }
}
fn check_budget<'a>(assets: impl Iterator<Item = &'a ImageAsset>) -> Result<()> {
    let unique: BTreeMap<_, _> = assets.map(|a| (a.id(), a)).collect();
    validate_budget(
        unique.len(),
        unique.values().map(|a| a.encoded().len()).sum(),
        unique.values().map(|a| a.rgba().len() as u64).sum(),
    )
}
fn validate_budget(count: usize, encoded: usize, rgba: u64) -> Result<()> {
    if count > MAX_ASSETS || encoded > MAX_ASSET_BYTES || rgba > MAX_RGBA_BYTES {
        Err(Error::TooLarge)
    } else {
        Ok(())
    }
}

impl Editor {
    /// Fits to glyph advance and vertical font metrics, preserving image aspect ratio.
    pub fn place_reference_image(
        &mut self,
        expected: u64,
        glyph: GlyphId,
        asset: ImageAsset,
    ) -> Result<u64> {
        self.check_revision(expected)?;
        let g = self
            .document
            .glyphs
            .get(&glyph)
            .ok_or(Error::MissingGlyph)?;
        let mut references = self.document.reference_images.clone();
        let id = references.next_id;
        references.next_id = id.checked_add(1).ok_or(Error::TooLarge)?;
        let fit_width = if g.advance > 0. {
            g.advance
        } else {
            f64::from(self.document.units_per_em)
        };
        let fit_height = self.document.ascender - self.document.descender;
        let scale =
            (fit_width / f64::from(asset.width())).min(fit_height / f64::from(asset.height()));
        let image = ReferenceImage {
            id,
            asset_id: asset.id().into(),
            center: [
                fit_width / 2.,
                (self.document.ascender + self.document.descender) / 2.,
            ],
            width: f64::from(asset.width()) * scale,
            height: f64::from(asset.height()) * scale,
            angle: 0.,
            opacity: 0.5,
            visible: true,
            locked: false,
        };
        // Reuse the original Arc when the same file is placed more than once.
        references.assets.entry(asset.id().into()).or_insert(asset);
        references.placements.entry(glyph).or_default().push(image);
        self.commit_references(expected, references)?;
        Ok(id)
    }
    /// Commit a completed gesture or numeric edit once; drafts are not document changes.
    pub fn update_reference_image(
        &mut self,
        expected: u64,
        glyph: GlyphId,
        image: ReferenceImage,
    ) -> Result<u64> {
        self.check_revision(expected)?;
        let mut references = self.document.reference_images.clone();
        let old = references
            .placements
            .get_mut(&glyph)
            .and_then(|v| v.iter_mut().find(|p| p.id == image.id))
            .ok_or(Error::InvalidDocument)?;
        if old.asset_id != image.asset_id {
            return Err(Error::InvalidDocument);
        }
        if old.locked
            && (old.center != image.center
                || old.width != image.width
                || old.height != image.height
                || old.angle != image.angle)
        {
            return Err(Error::Conflict);
        }
        *old = image;
        self.commit_references(expected, references)
    }
    pub fn remove_reference_image(
        &mut self,
        expected: u64,
        glyph: GlyphId,
        id: u64,
    ) -> Result<u64> {
        self.check_revision(expected)?;
        let mut references = self.document.reference_images.clone();
        let images = references
            .placements
            .get_mut(&glyph)
            .ok_or(Error::InvalidDocument)?;
        let index = images
            .iter()
            .position(|p| p.id == id)
            .ok_or(Error::InvalidDocument)?;
        images.remove(index);
        references.prune();
        self.commit_references(expected, references)
    }
    pub fn reorder_reference_image(
        &mut self,
        expected: u64,
        glyph: GlyphId,
        id: u64,
        index: usize,
    ) -> Result<u64> {
        self.check_revision(expected)?;
        let mut references = self.document.reference_images.clone();
        let images = references
            .placements
            .get_mut(&glyph)
            .ok_or(Error::InvalidDocument)?;
        if index >= images.len() {
            return Err(Error::InvalidDocument);
        }
        let previous = images
            .iter()
            .position(|p| p.id == id)
            .ok_or(Error::InvalidDocument)?;
        let image = images.remove(previous);
        images.insert(index, image);
        self.commit_references(expected, references)
    }
    fn commit_references(&mut self, expected: u64, references: ReferenceImages) -> Result<u64> {
        let revision = self.check_revision(expected)?;
        references.validate(&self.document.glyphs)?;
        if references == self.document.reference_images {
            return Ok(self.revision);
        }
        // Include assets retained by undo. The redo branch is discarded by this commit.
        let retained = self
            .undo
            .iter()
            .skip(usize::from(self.undo.len() == self.history_limit))
            .flat_map(|c| c.before_references.iter().chain(c.after_references.iter()))
            .chain(std::iter::once(&self.document.reference_images))
            .chain(std::iter::once(&references));
        check_budget(retained.flat_map(|r| r.assets.values()))?;
        let before = std::mem::replace(&mut self.document.reference_images, references.clone());
        self.undo.push_back(Change {
            before_references: Some(before),
            after_references: Some(references),
            ..Default::default()
        });
        if self.undo.len() > self.history_limit {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.revision = revision;
        Ok(revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, RgbaImage};
    fn asset(value: u32) -> ImageAsset {
        let mut buffer = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, value.to_le_bytes().to_vec()).unwrap())
            .write_to(&mut buffer, ImageFormat::Png)
            .unwrap();
        ImageAsset::from_bytes(buffer.into_inner()).unwrap()
    }
    #[test]
    fn aggregate_encoded_and_rgba_budgets_have_independent_boundaries() {
        assert_eq!(
            validate_budget(MAX_ASSETS, MAX_ASSET_BYTES, MAX_RGBA_BYTES),
            Ok(())
        );
        assert_eq!(validate_budget(MAX_ASSETS + 1, 1, 4), Err(Error::TooLarge));
        assert_eq!(
            validate_budget(1, MAX_ASSET_BYTES + 1, 4),
            Err(Error::TooLarge)
        );
        assert_eq!(
            validate_budget(1, 1, MAX_RGBA_BYTES + 1),
            Err(Error::TooLarge)
        );
    }
    #[test]
    fn oversized_headers_are_rejected_before_pixel_allocation() {
        // Replace the IHDR dimensions and checksum on a tiny valid PNG.
        for (width, height) in [(8193u32, 1u32), (4096, 4096), (u32::MAX, u32::MAX)] {
            let mut bytes = asset(0).encoded().to_vec();
            bytes[16..20].copy_from_slice(&width.to_be_bytes());
            bytes[20..24].copy_from_slice(&height.to_be_bytes());
            let mut crc = !0u32;
            for byte in &bytes[12..29] {
                crc ^= u32::from(*byte);
                for _ in 0..8 {
                    crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
                }
            }
            bytes[29..33].copy_from_slice(&(!crc).to_be_bytes());
            assert_eq!(ImageAsset::from_bytes(bytes), Err(Error::TooLarge));
        }
    }
    #[test]
    fn project_pixel_budget_is_checked_before_decoding_any_raster() {
        let mut assets = BTreeMap::new();
        for value in 0..5 {
            let mut bytes = asset(value).encoded().to_vec();
            bytes[16..20].copy_from_slice(&4000u32.to_be_bytes());
            bytes[20..24].copy_from_slice(&4000u32.to_be_bytes());
            let mut crc = !0u32;
            for byte in &bytes[12..29] {
                crc ^= u32::from(*byte);
                for _ in 0..8 {
                    crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
                }
            }
            bytes[29..33].copy_from_slice(&(!crc).to_be_bytes());
            // These headers advertise 320 MB in total. Their tiny bodies are invalid for
            // that size: decoding even the first one would fail with InvalidFile instead.
            assets.insert(asset_id(&bytes), STANDARD.encode(bytes));
        }
        assert_eq!(
            ReferenceImages::from_stored(StoredImages {
                version: 1,
                next_id: 1,
                assets,
                placements: BTreeMap::new(),
            }),
            Err(Error::TooLarge)
        );
    }
    #[test]
    fn deleted_assets_still_count_against_the_history_budget() {
        let mut editor = Editor::new(crate::FontDocument::new("Budget", "A").unwrap()).unwrap();
        editor.history_limit = 1024;
        for value in 0..MAX_ASSETS {
            let id = editor
                .place_reference_image(editor.revision(), 1, asset(value as u32))
                .unwrap();
            editor
                .remove_reference_image(editor.revision(), 1, id)
                .unwrap();
        }
        assert!(editor.document().reference_images.is_empty());
        let before = editor.document().clone();
        let revision = editor.revision();
        assert_eq!(
            editor.place_reference_image(revision, 1, asset(MAX_ASSETS as u32)),
            Err(Error::TooLarge)
        );
        assert_eq!(editor.document(), &before);
        assert_eq!(editor.revision(), revision);
        editor.undo(revision).unwrap();
        assert_eq!(editor.document().reference_images.assets.len(), 1);
    }
    #[test]
    fn abandoned_redo_and_expired_undo_release_assets() {
        let mut editor = Editor::new(crate::FontDocument::new("Lifetime", "A").unwrap()).unwrap();
        let image = asset(1);
        let weak = Arc::downgrade(&image.0);
        editor.place_reference_image(0, 1, image).unwrap();
        editor.undo(1).unwrap();
        assert!(weak.upgrade().is_some());
        let mut glyph = editor.document().glyphs[&1].clone();
        glyph.advance = 700.;
        editor.replace_glyph(2, glyph).unwrap();
        assert!(weak.upgrade().is_none());
        let image = asset(2);
        let weak = Arc::downgrade(&image.0);
        let id = editor.place_reference_image(3, 1, image).unwrap();
        editor.remove_reference_image(4, 1, id).unwrap();
        assert!(weak.upgrade().is_some());
        for index in 0..128 {
            let mut glyph = editor.document().glyphs[&1].clone();
            glyph.advance = 800. + index as f64;
            editor.replace_glyph(editor.revision(), glyph).unwrap();
        }
        assert!(weak.upgrade().is_none());
    }
}
