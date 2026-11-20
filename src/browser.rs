//! macOS Chrome automation through a bounded, cancellation-owned JXA helper.
//! JSON serialization is the only interpolation; never invokes a shell.
use serde_json::{json, Value};
use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

pub fn script(input: &Value) -> String {
    format!("const input = {};\n{}", input, include_str!("chrome.js"))
}
pub async fn run(input: Value) -> Result<Value, String> {
    if !cfg!(target_os = "macos") {
        return Err("Google Chrome automation is supported on macOS only".into());
    }
    let mut cmd = Command::new("/usr/bin/osascript");
    cmd.args(["-l", "JavaScript", "-e", &script(&input)])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Cannot start Chrome automation: {e}"))?;
    let mut out = child.stdout.take().ok_or("Missing helper stdout")?;
    let mut err = child.stderr.take().ok_or("Missing helper stderr")?;
    let work = async {
        let mut a = Vec::new();
        let mut b = Vec::new();
        let mut limited_out = (&mut out).take(2 * 1024 * 1024 + 1);
        let mut limited_err = (&mut err).take(64 * 1024 + 1);
        tokio::try_join!(
            limited_out.read_to_end(&mut a),
            limited_err.read_to_end(&mut b)
        )
        .map_err(|e| e.to_string())?;
        if a.len() > 2 * 1024 * 1024 || b.len() > 64 * 1024 {
            return Err("Chrome automation output exceeded its limit".into());
        }
        let status = child.wait().await.map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("Chrome automation failed. Check Chrome installation and macOS Automation/Accessibility permissions. {}",String::from_utf8_lossy(&b)));
        }
        serde_json::from_slice(&a)
            .map_err(|_| "Chrome automation returned invalid JSON".to_string())
    };
    tokio::time::timeout(Duration::from_secs(10), work)
        .await
        .map_err(|_| {
            "Chrome automation timed out; its action outcome may be uncertain".to_string()
        })?
}
pub async fn open_cli(args: &[String]) -> Result<(), String> {
    if args.is_empty()
        || args.len() > 2
        || !(args[0] == "--guest" || args[0].starts_with("--profile-directory="))
        || args
            .get(1)
            .is_some_and(|s| s.starts_with('-') || s.contains('\0'))
    {
        return Err("Invalid Chrome launch arguments".into());
    }
    if !cfg!(target_os = "macos") {
        return Err("Google Chrome automation is supported on macOS only".into());
    }
    let mut cmd = Command::new("/usr/bin/open");
    cmd.args(["-na", "Google Chrome", "--args"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .map_err(|_| "Chrome launch timed out; its outcome may be uncertain")?
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Could not launch Google Chrome".into())
    }
}
pub async fn new_tab(
    url: &str,
    behavior: &str,
    current: &str,
    original: &str,
) -> Result<(), String> {
    if behavior == "profile_current" || behavior == "profile_original" {
        let profile = if behavior == "profile_current" {
            current
        } else {
            original
        };
        if !crate::data::profile_id(profile) {
            return Err("Invalid Chrome profile".into());
        }
        // -- is deliberately omitted: Chrome consumes the single URL as a
        // positional argument. Disallow option-shaped strings at this boundary.
        if url.starts_with('-') {
            return Err("Invalid browser URL".into());
        }
        open_cli(&[format!("--profile-directory={profile}"), url.to_owned()]).await
    } else {
        run(json!({"kind":"new-tab","url":url})).await.map(|_| ())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_text_is_only_json_literal() {
        let p = json!({"url":"\"; throw new Error('injected') //\n\\","kind":"new-tab"});
        let s = script(&p);
        let line = s
            .lines()
            .next()
            .unwrap()
            .strip_prefix("const input = ")
            .unwrap()
            .trim_end_matches(';');
        assert_eq!(serde_json::from_str::<Value>(line).unwrap(), p);
    }
}
