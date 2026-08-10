use crate::settings::SavedServer;
use std::{collections::HashSet, fs, path::PathBuf, process::Command};

/// Shows the macOS server-address prompt, mounts the SMB/NFS/WebDAV volume using
/// the system credential flow, and returns the mounted path for the sidebar.
pub fn connect_with_macos_prompt() -> anyhow::Result<Option<SavedServer>> {
    let volumes_before = volume_directories();
    let script = r#"
set serverUrl to text returned of (display dialog "サーバーアドレスを入力してください" default answer "smb://" with title "サーバへ接続" buttons {"キャンセル", "接続"} default button "接続" cancel button "キャンセル")
mount volume serverUrl
return serverUrl
"#;
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        if error.contains("User canceled") || error.contains("-128") {
            return Ok(None);
        }
        anyhow::bail!(error.trim().to_owned());
    }
    let url = String::from_utf8(output.stdout)?.trim().to_owned();
    let volumes_after = volume_directories();
    let mounted_path = resolve_mounted_path(&url, &volumes_before, &volumes_after);
    if url.is_empty() || mounted_path.is_none() {
        anyhow::bail!("サーバーは接続されましたが、マウント先を取得できませんでした");
    }
    let mounted_path = mounted_path.unwrap();
    let name = mounted_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("共有サーバ")
        .to_owned();
    Ok(Some(SavedServer {
        name,
        url,
        mounted_path,
    }))
}

fn volume_directories() -> Vec<PathBuf> {
    fs::read_dir("/Volumes")
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}

fn resolve_mounted_path(url: &str, before: &[PathBuf], after: &[PathBuf]) -> Option<PathBuf> {
    let before = before.iter().collect::<HashSet<_>>();
    let new_volumes = after
        .iter()
        .filter(|path| !before.contains(path))
        .collect::<Vec<_>>();
    if new_volumes.len() == 1 {
        return Some(new_volumes[0].clone());
    }

    let share_name = share_name_from_url(url)?;
    let matching_volume = |path: &&PathBuf| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == share_name || name.starts_with(&format!("{share_name}-")))
    };
    new_volumes
        .into_iter()
        .find(matching_volume)
        .or_else(|| after.iter().find(matching_volume))
        .cloned()
}

fn share_name_from_url(url: &str) -> Option<String> {
    let path = url
        .split_once("://")?
        .1
        .split('/')
        .filter(|part| !part.is_empty())
        .nth(1)?;
    let decoded = percent_decode(path);
    (!decoded.is_empty()).then_some(decoded)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            output.push(high * 16 + low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_newly_mounted_volume() {
        let before = vec![PathBuf::from("/Volumes/Macintosh HD")];
        let after = vec![
            PathBuf::from("/Volumes/Macintosh HD"),
            PathBuf::from("/Volumes/media"),
        ];
        assert_eq!(
            resolve_mounted_path("smb://nas.local/media", &before, &after),
            Some(PathBuf::from("/Volumes/media"))
        );
    }

    #[test]
    fn finds_an_already_mounted_volume_and_decodes_its_name() {
        let volumes = vec![PathBuf::from("/Volumes/共有 フォルダ")];
        assert_eq!(
            resolve_mounted_path(
                "smb://nas.local/%E5%85%B1%E6%9C%89%20%E3%83%95%E3%82%A9%E3%83%AB%E3%83%80",
                &volumes,
                &volumes,
            ),
            Some(PathBuf::from("/Volumes/共有 フォルダ"))
        );
    }

    #[test]
    fn accepts_macos_duplicate_volume_suffixes() {
        let volumes = vec![PathBuf::from("/Volumes/media-1")];
        assert_eq!(
            resolve_mounted_path("smb://nas.local/media", &[], &volumes),
            Some(PathBuf::from("/Volumes/media-1"))
        );
    }
}
