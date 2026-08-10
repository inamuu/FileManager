use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
}

impl FileEntry {
    pub fn icon(&self) -> &'static str {
        if self.is_dir {
            "📁"
        } else {
            match self
                .path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
            {
                "png" | "jpg" | "jpeg" | "gif" | "heic" => "🖼",
                "mp3" | "wav" | "m4a" => "♫",
                "mp4" | "mov" | "mkv" => "▶",
                "zip" | "tar" | "gz" | "7z" => "◫",
                "pdf" => "PDF",
                _ => "▤",
            }
        }
    }

    pub fn formatted_size(&self) -> String {
        if self.is_dir {
            "—".into()
        } else if self.size >= 1_000_000_000 {
            format!("{:.1} GB", self.size as f64 / 1_000_000_000.0)
        } else if self.size >= 1_000_000 {
            format!("{:.1} MB", self.size as f64 / 1_000_000.0)
        } else if self.size >= 1_000 {
            format!("{:.1} KB", self.size as f64 / 1_000.0)
        } else {
            format!("{} B", self.size)
        }
    }
}

pub fn read_directory(path: &Path) -> anyhow::Result<Vec<FileEntry>> {
    let mut entries = fs::read_dir(path)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            Some(FileEntry {
                path: entry.path(),
                name,
                is_dir: metadata.is_dir(),
                size: metadata.len(),
                modified: metadata
                    .modified()
                    .ok()?
                    .duration_since(UNIX_EPOCH)
                    .ok()?
                    .as_secs(),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| (!entry.is_dir, entry.name.to_lowercase()));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_file_sizes() {
        let mut entry = FileEntry {
            path: PathBuf::from("movie.mov"),
            name: "movie.mov".into(),
            is_dir: false,
            size: 1_500_000,
            modified: 0,
        };
        assert_eq!(entry.formatted_size(), "1.5 MB");
        entry.is_dir = true;
        assert_eq!(entry.formatted_size(), "—");
    }
}
