//! OS locations are resolved at runtime. Adobe caches are discovery sources, never write targets.
use serde::Serialize;
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, Serialize)]
pub struct Location {
    pub source: &'static str,
    pub path: PathBuf,
    pub status: String,
}
fn location(source: &'static str, path: PathBuf) -> Location {
    Location {
        source,
        path,
        status: "pending".into(),
    }
}
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum Platform {
    Windows,
    Mac,
    Linux,
}
pub struct Environment {
    pub home: PathBuf,
    pub local: Option<PathBuf>,
    pub roaming: Option<PathBuf>,
    pub windows: Option<PathBuf>,
    pub data: Option<PathBuf>,
    pub data_dirs: Vec<PathBuf>,
}
pub fn locations(os: Platform, e: &Environment) -> Vec<Location> {
    match os {
        Platform::Windows => vec![
            location(
                "user",
                e.local
                    .clone()
                    .unwrap_or_else(|| e.home.join("AppData/Local"))
                    .join("Microsoft/Windows/Fonts"),
            ),
            location(
                "system",
                e.windows
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
                    .join("Fonts"),
            ),
            location(
                "adobe",
                e.roaming
                    .clone()
                    .unwrap_or_else(|| e.home.join("AppData/Roaming"))
                    .join("Adobe/CoreSync/plugins/livetype"),
            ),
        ],
        Platform::Mac => vec![
            location("user", e.home.join("Library/Fonts")),
            location("system", "/Library/Fonts".into()),
            location("system", "/System/Library/Fonts".into()),
            location(
                "system",
                "/System/Library/AssetsV2/com_apple_MobileAsset_Font7".into(),
            ),
            location(
                "adobe",
                e.home
                    .join("Library/Application Support/Adobe/CoreSync/plugins/livetype"),
            ),
        ],
        Platform::Linux => {
            let mut roots = vec![
                location(
                    "user",
                    e.data
                        .clone()
                        .unwrap_or_else(|| e.home.join(".local/share"))
                        .join("fonts"),
                ),
                location("user", e.home.join(".fonts")),
            ];
            for p in e.data_dirs.iter().chain(
                [
                    PathBuf::from("/usr/local/share"),
                    PathBuf::from("/usr/share"),
                ]
                .iter(),
            ) {
                let p = p.join("fonts");
                if !roots.iter().any(|r| r.path == p) {
                    roots.push(location("system", p));
                }
            }
            roots
        }
    }
}
pub fn current() -> Result<Vec<Location>, String> {
    let home = dirs::home_dir().ok_or("font_home_unavailable")?;
    #[cfg(target_os = "windows")]
    let platform = Platform::Windows;
    #[cfg(target_os = "macos")]
    let platform = Platform::Mac;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let platform = Platform::Linux;
    let absolute_env = |key| {
        std::env::var_os(key)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    Ok(locations(
        platform,
        &Environment {
            home,
            local: absolute_env("LOCALAPPDATA"),
            roaming: absolute_env("APPDATA"),
            windows: absolute_env("WINDIR"),
            data: absolute_env("XDG_DATA_HOME"),
            data_dirs: std::env::var_os("XDG_DATA_DIRS")
                .map(|v| {
                    std::env::split_paths(&v)
                        .filter(|p| p.is_absolute())
                        .collect()
                })
                .unwrap_or_default(),
        },
    ))
}
pub fn user_directory() -> Result<PathBuf, String> {
    current()?
        .into_iter()
        .find(|r| r.source == "user")
        .map(|r| r.path)
        .ok_or("font_home_unavailable".into())
}
pub fn under(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn env() -> Environment {
        Environment {
            home: "/people/A".into(),
            local: Some("/local".into()),
            roaming: Some("/roaming".into()),
            windows: Some("/windows".into()),
            data: Some("/data".into()),
            data_dirs: vec!["/custom".into()],
        }
    }
    #[test]
    fn resolves_all_platforms_without_literal_username() {
        let w = locations(Platform::Windows, &env());
        assert_eq!(w[0].path, PathBuf::from("/local/Microsoft/Windows/Fonts"));
        assert_eq!(
            w[2].path,
            PathBuf::from("/roaming/Adobe/CoreSync/plugins/livetype")
        );
        let m = locations(Platform::Mac, &env());
        assert_eq!(m[0].path, PathBuf::from("/people/A/Library/Fonts"));
        assert_eq!(m.last().unwrap().source, "adobe");
        let l = locations(Platform::Linux, &env());
        assert!(l.iter().any(|r| r.path == Path::new("/data/fonts")));
        assert!(l.iter().any(|r| r.path == Path::new("/custom/fonts")));
        assert!(l.iter().all(|r| r.source != "adobe"));
    }
}
