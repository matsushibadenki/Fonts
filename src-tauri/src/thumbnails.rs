//! Bounded, application-owned preview worker. WebViews request native image resources;
//! neither outlines nor raw frame buffers are serialized through JavaScript IPC.
use crate::AppState;
use font_renderer::{wgpu, Gpu};
use std::{
    collections::{HashMap, VecDeque},
    sync::mpsc,
};
use tauri::Manager;

type Key = (u64, u64, u64);
const CAPACITY: usize = 512;
#[derive(Default)]
struct Cache {
    generation: Option<(u64, u64)>,
    images: HashMap<u64, Vec<u8>>,
    order: VecDeque<u64>,
}
impl Cache {
    fn generation(&mut self, epoch: u64, revision: u64) {
        if self.generation != Some((epoch, revision)) {
            self.generation = Some((epoch, revision));
            self.images.clear();
            self.order.clear();
        }
    }
    fn get(&mut self, id: u64) -> Option<Vec<u8>> {
        let bytes = self.images.get(&id)?.clone();
        self.order.retain(|key| *key != id);
        self.order.push_back(id);
        Some(bytes)
    }
    fn insert(&mut self, id: u64, bytes: Vec<u8>) {
        self.order.retain(|key| *key != id);
        self.images.insert(id, bytes);
        self.order.push_back(id);
        while self.images.len() > CAPACITY {
            if let Some(old) = self.order.pop_front() {
                self.images.remove(&old);
            }
        }
    }
}
struct Job {
    app: tauri::AppHandle,
    key: Key,
    responder: tauri::UriSchemeResponder,
}
pub struct Thumbnails(mpsc::SyncSender<Job>);
impl Thumbnails {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::sync_channel::<Job>(512);
        std::thread::Builder::new()
            .name("glyph-thumbnails".into())
            .spawn(move || {
                let mut gpu: Option<Gpu> = None;
                let mut cache = Cache::default();
                while let Ok(job) = rx.recv() {
                    let result = (|| {
                        let (epoch, revision, id) = job.key;
                        let state = job.app.state::<AppState>();
                        let model = state.0.lock().map_err(|_| 500u16)?;
                        if model.epoch != epoch || model.editor.revision() != revision {
                            return Err(409);
                        }
                        cache.generation(epoch, revision);
                        if let Some(bytes) = cache.get(id) {
                            return Ok(bytes);
                        }
                        let contours = if id & (1 << 63) != 0 {
                            let key = format!(
                                "{}:{}",
                                if id & (1 << 62) != 0 {
                                    "builtin"
                                } else {
                                    "user"
                                },
                                id & ((1 << 62) - 1)
                            );
                            model
                                .snippet(&key)
                                .map_err(|_| 404u16)?
                                .contours
                                .as_ref()
                                .clone()
                        } else {
                            model
                                .editor
                                .document()
                                .glyphs
                                .get(&(u32::try_from(id).map_err(|_| 404u16)?))
                                .ok_or(404u16)?
                                .contours
                                .clone()
                        };
                        drop(model); // GPU work never holds the document lock.
                        if gpu.is_none() {
                            let instance = wgpu::Instance::new(
                                wgpu::InstanceDescriptor::new_without_display_handle(),
                            );
                            gpu = Some(
                                pollster::block_on(Gpu::new(
                                    &instance,
                                    None,
                                    wgpu::TextureFormat::Rgba8Unorm,
                                ))
                                .map_err(|_| 503u16)?,
                            );
                        }
                        let pixels = gpu
                            .as_ref()
                            .unwrap()
                            .thumbnail(&contours)
                            .map_err(|_| 422u16)?;
                        let mut bytes = Vec::new();
                        {
                            let mut encoder = png::Encoder::new(&mut bytes, 128, 128);
                            encoder.set_color(png::ColorType::Rgba);
                            encoder.set_depth(png::BitDepth::Eight);
                            encoder
                                .write_header()
                                .map_err(|_| 500u16)?
                                .write_image_data(&pixels)
                                .map_err(|_| 500u16)?;
                        }
                        cache.insert(id, bytes.clone());
                        Ok(bytes)
                    })();
                    respond(job.responder, result);
                }
            })
            .expect("thumbnail worker could not start");
        Self(tx)
    }
    pub fn request(&self, app: tauri::AppHandle, path: &str, responder: tauri::UriSchemeResponder) {
        let Some(key) = parse_key(path) else {
            respond(responder, Err(400));
            return;
        };
        if let Err(error) = self.0.try_send(Job {
            app,
            key,
            responder,
        }) {
            let (mpsc::TrySendError::Full(job) | mpsc::TrySendError::Disconnected(job)) = error;
            respond(job.responder, Err(503));
        }
    }
}
fn parse_key(path: &str) -> Option<Key> {
    let (path, kind) = if let Some(p) = path.strip_prefix("/snippet-b-") {
        (p, 3u64 << 62)
    } else if let Some(p) = path.strip_prefix("/snippet-u-") {
        (p, 1u64 << 63)
    } else {
        (path.strip_prefix('/')?, 0)
    };
    let mut parts = path.strip_suffix(".png")?.split('-');
    let mut key: Key = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    if parts.next().is_some() {
        return None;
    }
    if key.2
        > if kind == 0 {
            u64::from(u32::MAX)
        } else {
            9_007_199_254_740_991
        }
    {
        return None;
    }
    key.2 |= kind;
    Some(key)
}
fn respond(responder: tauri::UriSchemeResponder, result: Result<Vec<u8>, u16>) {
    let (status, body) = match result {
        Ok(bytes) => (200, bytes),
        Err(status) => (status, Vec::new()),
    };
    responder.respond(
        tauri::http::Response::builder()
            .status(status)
            .header("Content-Type", "image/png")
            // Revalidate with the document generation even if a project is replaced.
            .header("Cache-Control", "no-store")
            .header("X-Content-Type-Options", "nosniff")
            .body(body)
            .unwrap(),
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_paths_are_strict_and_cannot_address_files() {
        assert_eq!(parse_key("/4-9-123.png"), Some((4, 9, 123)));
        assert_eq!(
            parse_key("/snippet-b-4-9-2.png"),
            Some((4, 9, (3u64 << 62) | 2))
        );
        assert_eq!(
            parse_key("/snippet-u-4-9-2.png"),
            Some((4, 9, (1u64 << 63) | 2))
        );
        for invalid in [
            "/snippet-x-4-9-2.png",
            "/snippet-u-4-9-18446744073709551615.png",
            "/etc/passwd",
            "/4-9-123.png/extra",
            "/4/9/../1.png",
            "/1/2/4294967296.png",
        ] {
            assert_eq!(parse_key(invalid), None);
        }
    }
    #[test]
    fn cache_is_bounded_reuses_recent_entries_and_invalidates_undo() {
        let mut cache = Cache::default();
        cache.generation(1, 2);
        for id in 0..CAPACITY as u64 {
            cache.insert(id, vec![1]);
        }
        assert_eq!(cache.get(0), Some(vec![1]));
        cache.insert(CAPACITY as u64, vec![2]);
        assert_eq!(cache.images.len(), CAPACITY);
        assert!(cache.get(1).is_none());
        assert!(cache.get(0).is_some());
        cache.generation(1, 3);
        assert!(cache.images.is_empty());
        cache.insert(0, vec![3]);
        cache.generation(2, 3);
        assert!(cache.images.is_empty());
    }
}
