use crate::settings::SavedServer;
use std::{path::PathBuf, process::Command};

/// Shows the macOS server-address prompt, mounts the SMB/NFS/WebDAV volume using
/// the system credential flow, and returns the mounted path for the sidebar.
pub fn connect_with_macos_prompt() -> anyhow::Result<Option<SavedServer>> {
    let script = r#"
set serverUrl to text returned of (display dialog "サーバーアドレスを入力してください" default answer "smb://" with title "サーバへ接続" buttons {"キャンセル", "接続"} default button "接続" cancel button "キャンセル")
set mountedDisk to mount volume serverUrl
set mountedPath to POSIX path of (mountedDisk as alias)
return serverUrl & linefeed & mountedPath
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
    let text = String::from_utf8(output.stdout)?;
    let mut lines = text.lines();
    let url = lines.next().unwrap_or_default().trim().to_owned();
    let mounted_path = PathBuf::from(lines.next().unwrap_or_default().trim());
    if url.is_empty() || !mounted_path.is_dir() {
        anyhow::bail!("サーバーは接続されましたが、マウント先を取得できませんでした");
    }
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
