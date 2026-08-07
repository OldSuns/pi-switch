/// Copy text to the system clipboard using the platform's native CLI.
/// Falls back gracefully when no clipboard tool is available.
pub(super) fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[cfg(target_os = "windows")]
    let (program, args): (&str, Vec<&str>) = ("clip", vec![]);
    #[cfg(target_os = "macos")]
    let (program, args): (&str, Vec<&str>) = ("pbcopy", vec![]);
    #[cfg(all(unix, not(target_os = "macos")))]
    let (program, args): (&str, Vec<&str>) = {
        if std::process::Command::new("which")
            .arg("wl-copy")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
        {
            ("wl-copy", vec!["--"])
        } else if std::process::Command::new("which")
            .arg("xclip")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
        {
            ("xclip", vec!["-selection", "clipboard"])
        } else if std::process::Command::new("which")
            .arg("xsel")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
        {
            ("xsel", vec!["--clipboard", "--input"])
        } else {
            return Err("no clipboard tool found (install xclip, xsel, or wl-copy)".into());
        }
    };

    let mut child = Command::new(program)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("failed to launch {program}: {error}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|error| format!("failed to write to {program}: {error}"))?;
    }
    let status = child
        .wait()
        .map_err(|error| format!("failed to wait for {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}
