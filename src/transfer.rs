use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferState {
    Preparing,
    Running,
    Completed,
    Failed(String),
}

impl TransferState {
    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed(_))
    }
}

#[derive(Clone, Debug)]
pub struct TransferProgress {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub copied_bytes: u64,
    pub total_bytes: u64,
    pub state: TransferState,
}

impl TransferProgress {
    pub fn fraction(&self) -> f32 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.copied_bytes as f32 / self.total_bytes as f32).clamp(0.0, 1.0)
        }
    }
}

pub type SharedProgress = Arc<Mutex<TransferProgress>>;

pub fn start_copy(source: PathBuf, destination_dir: PathBuf) -> SharedProgress {
    let destination = destination_dir.join(source.file_name().unwrap_or_default());
    let progress = Arc::new(Mutex::new(TransferProgress {
        source: source.clone(),
        destination: destination.clone(),
        copied_bytes: 0,
        total_bytes: 0,
        state: TransferState::Preparing,
    }));
    let worker_progress = Arc::clone(&progress);
    thread::spawn(move || {
        let total_bytes = match total_size(&source) {
            Ok(total_bytes) => total_bytes,
            Err(error) => {
                if let Ok(mut value) = worker_progress.lock() {
                    value.state = TransferState::Failed(error.to_string());
                }
                return;
            }
        };
        if let Ok(mut value) = worker_progress.lock() {
            value.total_bytes = total_bytes;
            value.state = TransferState::Running;
        }
        let result = copy_path(&source, &destination, &worker_progress);
        if let Ok(mut value) = worker_progress.lock() {
            value.state = match result {
                Ok(()) => TransferState::Completed,
                Err(error) => TransferState::Failed(error.to_string()),
            };
        }
    });
    progress
}

fn total_size(path: &Path) -> anyhow::Result<u64> {
    let metadata = fs::metadata(path)?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    let mut total = 0;
    for entry in fs::read_dir(path)? {
        total += total_size(&entry?.path())?;
    }
    Ok(total)
}

fn copy_path(source: &Path, destination: &Path, progress: &SharedProgress) -> anyhow::Result<()> {
    if source.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_path(
                &entry.path(),
                &destination.join(entry.file_name()),
                progress,
            )?;
        }
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut reader = fs::File::open(source)?;
    let mut writer = fs::File::create(destination)?;
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        writer.write_all(&buffer[..count])?;
        if let Ok(mut value) = progress.lock() {
            value.copied_bytes += count as u64;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn copies_a_file_and_reports_completion() {
        let root = std::env::temp_dir().join(format!("filemanager-test-{}", std::process::id()));
        let source_dir = root.join("source");
        let destination_dir = root.join("destination");
        fs::create_dir_all(&source_dir).unwrap();
        fs::create_dir_all(&destination_dir).unwrap();
        fs::write(source_dir.join("hello.txt"), b"hello").unwrap();
        let progress = start_copy(source_dir.join("hello.txt"), destination_dir.clone());
        let started = Instant::now();
        loop {
            let state = progress.lock().unwrap().state.clone();
            if state.is_finished() {
                assert_eq!(state, TransferState::Completed);
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            fs::read(destination_dir.join("hello.txt")).unwrap(),
            b"hello"
        );
        let final_progress = progress.lock().unwrap();
        assert_eq!(final_progress.copied_bytes, final_progress.total_bytes);
        assert_eq!(final_progress.fraction(), 1.0);
        let _ = fs::remove_dir_all(root);
    }
}
