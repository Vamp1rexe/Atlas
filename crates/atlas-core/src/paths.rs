use std::path::PathBuf;

/// `%APPDATA%\Atlas` on Windows, the matching data dir elsewhere.
pub fn root() -> PathBuf {
    let p = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Atlas");
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn versions() -> PathBuf {
    root().join("versions")
}
pub fn libraries() -> PathBuf {
    root().join("libraries")
}
pub fn assets() -> PathBuf {
    root().join("assets")
}
pub fn instances() -> PathBuf {
    root().join("instances")
}
pub fn runtime() -> PathBuf {
    root().join("runtime")
}
pub fn cosmetics() -> PathBuf {
    root().join("cosmetics")
}
