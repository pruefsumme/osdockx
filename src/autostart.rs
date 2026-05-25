use directories::BaseDirs;
use std::fs;
use std::path::{Path, PathBuf};

pub const APP_ID: &str = "dev.osdockx.OSDockX";
pub const DESKTOP_FILE_NAME: &str = "dev.pruefsumme.OSDockX.desktop";

pub fn set_enabled(enabled: bool) -> anyhow::Result<()> {
    let path = autostart_path()?;
    if enabled {
        let exec = current_executable_exec()?;
        write_desktop_file(&path, &desktop_entry(&exec, true))?;
    } else if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn autostart_path() -> anyhow::Result<PathBuf> {
    let dirs =
        BaseDirs::new().ok_or_else(|| anyhow::anyhow!("could not resolve XDG config directory"))?;
    Ok(dirs.config_dir().join("autostart").join(DESKTOP_FILE_NAME))
}

pub fn launcher_desktop_entry(exec: &str) -> String {
    desktop_entry(exec, false)
}

fn current_executable_exec() -> anyhow::Result<String> {
    Ok(desktop_exec_arg(&std::env::current_exe()?))
}

fn write_desktop_file(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn desktop_entry(exec: &str, autostart: bool) -> String {
    let autostart_line = if autostart {
        "X-GNOME-Autostart-enabled=true\n"
    } else {
        ""
    };
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=OSDockX\n\
         Comment=A lightweight OSX-inspired dock for Linux/X11\n\
         Exec={exec}\n\
         Terminal=false\n\
         Categories=Utility;\n\
         StartupNotify=false\n\
         {autostart_line}"
    )
}

fn desktop_exec_arg(path: &Path) -> String {
    let value = path.to_string_lossy();
    if value
        .chars()
        .all(|ch| !ch.is_whitespace() && ch != '"' && ch != '\\')
    {
        return value.into_owned();
    }

    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_desktop_entry_omits_icon() {
        let entry = launcher_desktop_entry("osdockx");

        assert!(entry.contains("Exec=osdockx\n"));
        assert!(!entry.contains("Icon="));
        assert!(!entry.contains("Autostart"));
    }

    #[test]
    fn autostart_desktop_entry_marks_autostart_enabled() {
        let entry = desktop_entry("osdockx", true);

        assert!(entry.contains("Exec=osdockx\n"));
        assert!(entry.contains("X-GNOME-Autostart-enabled=true\n"));
        assert!(!entry.contains("Icon="));
    }

    #[test]
    fn desktop_exec_arg_quotes_paths_with_spaces() {
        let arg = desktop_exec_arg(Path::new("/home/test user/.local/bin/osdockx"));

        assert_eq!(arg, "\"/home/test user/.local/bin/osdockx\"");
    }
}
