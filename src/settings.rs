use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct SavedServer {
    pub name: String,
    pub url: String,
    pub mounted_path: PathBuf,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Settings {
    pub servers: Vec<SavedServer>,
}

impl Settings {
    pub fn load() -> Self {
        fs::read(Self::path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn remember_server(&mut self, server: SavedServer) -> anyhow::Result<()> {
        self.servers.retain(|saved| saved.url != server.url);
        self.servers.push(server);
        self.save()
    }

    pub fn path() -> PathBuf {
        let base = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(".").to_path_buf());
        base.join("Library/Application Support/FileManager/settings.json")
    }
}
