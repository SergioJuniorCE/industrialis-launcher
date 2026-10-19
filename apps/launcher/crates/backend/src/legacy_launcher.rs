//! Best-effort guard for released Electron binaries without profile ownership.
//! The installed upgrade still must remove the old entry point: this cannot
//! prevent an uncooperative binary from starting after the admission check.
use std::path::Path;
fn is_electron_launcher(executable: &Path) -> bool {
    let name = executable
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if !name.eq_ignore_ascii_case("industrialis-launcher.exe") && name != "industrialis-launcher" {
        return false;
    }
    let Some(parent) = executable.parent() else {
        return false;
    };
    parent.join("resources/app.asar").is_file()
        || (cfg!(target_os = "macos") && parent.join("../Resources/app.asar").is_file())
}
pub(crate) fn ensure_safe_start(paths: &crate::paths::LauncherPaths) -> anyhow::Result<()> {
    // An isolated test/CLI profile does not share the original launcher's data.
    let default = crate::paths::default_data_dir();
    if paths.base().canonicalize().ok() != default.canonicalize().ok() || !default.exists() {
        return Ok(());
    }
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
    );
    if system
        .processes()
        .values()
        .filter_map(|p| p.exe())
        .any(is_electron_launcher)
    {
        anyhow::bail!("The original Electron launcher is still running. Close it before opening this replacement so both applications cannot change the same data. No existing process was stopped or profile document changed.");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_the_packaged_electron_app_without_blocking_native_or_unrelated_executables() {
        let root =
            std::env::temp_dir().join(format!("industrialis-old-install-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("resources")).unwrap();
        std::fs::write(root.join("resources/app.asar"), b"fixture").unwrap();
        assert!(is_electron_launcher(
            &root.join("industrialis-launcher.exe")
        ));
        assert!(is_electron_launcher(&root.join("industrialis-launcher")));
        assert!(!is_electron_launcher(&root.join("industrialis.exe")));
        assert!(!is_electron_launcher(&root.join("electron.exe")));
        assert!(!is_electron_launcher(
            &root.join("unpacked/industrialis-launcher.exe")
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
}
