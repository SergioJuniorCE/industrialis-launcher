//! Port of `apps/launcher/electron/backend/java.ts`.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use industrialis_core::natural::compare_natural;

const INSPECT_TIMEOUT: Duration = Duration::from_secs(8);

/// Matches the `invoke("detect_java")` JSON shape from the Electron backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaInfo {
    pub path: String,
    pub version: String,
    pub major_version: u32,
    pub architecture: String,
    pub vendor: String,
}

struct JavaPatterns {
    digits: Regex,
    version_line: Regex,
    gui_name: Regex,
}

fn patterns() -> &'static JavaPatterns {
    static CELL: OnceLock<JavaPatterns> = OnceLock::new();
    CELL.get_or_init(|| JavaPatterns {
        digits: Regex::new(r"\d+").unwrap(),
        version_line: Regex::new(r#"(?i)(?:java|openjdk)\s+version\s+"([^"]+)""#).unwrap(),
        gui_name: Regex::new(r"(?i)java\.exe$").unwrap(),
    })
}

fn property_value(output: &str, property: &str) -> Option<String> {
    let escaped = regex::escape(property);
    let re = Regex::new(&format!(r"(?m)^\s*{escaped}\s*=\s*(.+?)\s*$")).ok()?;
    re.captures(output)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
}

/// Legacy `1.8.x` numbering maps to major 8; otherwise the first number wins.
pub fn java_major_version(version: &str) -> Option<u32> {
    let parts: Vec<u32> = patterns()
        .digits
        .find_iter(version)
        .filter_map(|m| m.as_str().parse().ok())
        .collect();
    if parts.is_empty() {
        return None;
    }
    let major = if parts[0] == 1 && parts.len() > 1 {
        parts[1]
    } else {
        parts[0]
    };
    if major == 0 {
        None
    } else {
        Some(major)
    }
}

pub fn normalize_architecture(architecture: Option<&str>) -> String {
    let value = architecture.unwrap_or("").trim().to_lowercase();
    match value.as_str() {
        "x86_64" | "x64" => "amd64".to_string(),
        "i386" | "i486" | "i586" | "i686" => "x86".to_string(),
        "aarch64" => "arm64".to_string(),
        "" => "unknown".to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaRuntimeDetails {
    pub version: String,
    pub major_version: u32,
    pub architecture: String,
    pub vendor: String,
}

/// Mirrors `parseJavaRuntimeDetails`.
pub fn parse_java_runtime_details(output: &str) -> Option<JavaRuntimeDetails> {
    // Fast path for the common `-XshowSettings:properties` layout.
    let mut version = property_value(output, "java.version");
    if version.is_none() {
        version = patterns()
            .version_line
            .captures(output)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string());
    }
    let version = version?;
    let major_version = java_major_version(&version)?;
    Some(JavaRuntimeDetails {
        version,
        major_version,
        architecture: normalize_architecture(property_value(output, "os.arch").as_deref()),
        vendor: property_value(output, "java.vendor")
            .unwrap_or_else(|| "Unknown vendor".to_string()),
    })
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "java.exe"
    } else {
        "java"
    }
}

/// Mirrors `javaFromHome`: `<home>/bin/java[.exe]` (or `<home>` itself when it
/// already ends in `bin`).
pub fn java_from_home(home: &str) -> Option<PathBuf> {
    let trimmed = home.trim();
    if trimmed.is_empty() {
        return None;
    }
    let base = Path::new(trimmed);
    let bin = if base
        .file_name()
        .map(|n| n.eq_ignore_ascii_case("bin"))
        .unwrap_or(false)
    {
        base.to_path_buf()
    } else {
        base.join("bin")
    };
    let primary = bin.join(exe_name());
    if primary.is_file() {
        return Some(primary);
    }
    if cfg!(windows) {
        let fallback = bin.join("java");
        if fallback.is_file() {
            return Some(fallback);
        }
    }
    None
}

/// `JAVA_HOME` first, then `PATH` — mirrors `javaPath()`.
pub fn java_path_from_env(java_home: Option<&str>, path_var: &str) -> Option<PathBuf> {
    if let Some(home) = java_home {
        if let Some(java) = java_from_home(home) {
            return Some(java);
        }
    }
    let separator = if cfg!(windows) { ';' } else { ':' };
    for directory in path_var.split(separator).filter(|d| !d.is_empty()) {
        let candidate = Path::new(directory).join(exe_name());
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn java_path() -> Option<PathBuf> {
    java_path_from_env(
        std::env::var("JAVA_HOME").ok().as_deref(),
        &std::env::var("PATH").unwrap_or_default(),
    )
}

async fn add_java_homes(root: &Path, max_depth: i32, candidates: &mut HashSet<PathBuf>) {
    // Iterative stack instead of async recursion (which would need boxing).
    let mut stack = vec![(root.to_path_buf(), max_depth)];
    while let Some((dir, depth)) = stack.pop() {
        if depth < 0 {
            continue;
        }
        if let Some(java) = dir.to_str().and_then(java_from_home) {
            candidates.insert(java);
            continue;
        }
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if tokio::fs::symlink_metadata(&path)
                .await
                .map(|m| m.is_dir())
                .unwrap_or(false)
            {
                stack.push((path, depth - 1));
            }
        }
    }
}

fn windows_java_roots() -> Vec<(PathBuf, i32)> {
    let mut roots = Vec::new();
    let vendors = [
        "Java",
        "Eclipse Adoptium",
        "Microsoft",
        "Amazon Corretto",
        "BellSoft",
        "Zulu",
        "Semeru",
    ];
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(program_root) = std::env::var(key) {
            if program_root.is_empty() {
                continue;
            }
            roots.extend(
                vendors
                    .iter()
                    .map(|v| (PathBuf::from(&program_root).join(v), 2)),
            );
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        if !local.is_empty() {
            roots.extend(
                vendors
                    .iter()
                    .map(|v| (PathBuf::from(&local).join("Programs").join(v), 2)),
            );
        }
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.is_empty() {
            roots.push((
                PathBuf::from(&appdata).join(".minecraft").join("runtime"),
                5,
            ));
            roots.push((
                PathBuf::from(&appdata).join("PrismLauncher").join("java"),
                3,
            ));
        }
    }
    roots
}

pub async fn discover_java_candidates() -> Vec<PathBuf> {
    let mut candidates = HashSet::new();
    if let Some(java) = std::env::var("JAVA_HOME")
        .ok()
        .as_deref()
        .and_then(java_from_home)
    {
        candidates.insert(java);
    }
    let separator = if cfg!(windows) { ';' } else { ':' };
    for directory in std::env::var("PATH")
        .unwrap_or_default()
        .split(separator)
        .filter(|d| !d.is_empty())
    {
        let candidate = Path::new(directory).join(exe_name());
        if candidate.is_file() {
            candidates.insert(candidate);
        }
    }
    if cfg!(windows) {
        for (root, depth) in windows_java_roots() {
            add_java_homes(&root, depth, &mut candidates).await;
        }
    } else {
        for root in ["/usr/lib/jvm", "/Library/Java/JavaVirtualMachines"] {
            add_java_homes(Path::new(root), 4, &mut candidates).await;
        }
    }
    candidates.into_iter().filter(|c| c.is_file()).collect()
}

fn runtime_command(executable: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(executable);
    // Metadata probes are background work, not interactive Java applications.
    // A GUI launcher otherwise creates a console for every java.exe inspected.
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    command.kill_on_drop(true);
    command
}

async fn inspect_java(candidate: PathBuf) -> Option<JavaInfo> {
    let output = tokio::time::timeout(
        INSPECT_TIMEOUT,
        runtime_command(&candidate)
            .args(["-XshowSettings:properties", "-version"])
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    // TS parses `${stderr}\n${stdout}` regardless of exit status.
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    parse_java_runtime_details(&combined).map(|details| JavaInfo {
        path: candidate.to_string_lossy().into_owned(),
        version: details.version,
        major_version: details.major_version,
        architecture: details.architecture,
        vendor: details.vendor,
    })
}

fn compare_java_info(left: &JavaInfo, right: &JavaInfo) -> std::cmp::Ordering {
    // Newest major first, then numeric version desc, then path asc —
    // mirrors the TS sort comparator.
    right
        .major_version
        .cmp(&left.major_version)
        .then(compare_natural(&right.version, &left.version))
        .then(left.path.cmp(&right.path))
}

/// Mirrors `detectJava`, including case-insensitive path dedupe.
pub async fn detect_java() -> Vec<JavaInfo> {
    let candidates = discover_java_candidates().await;
    let mut handles = Vec::new();
    for candidate in candidates {
        handles.push(tokio::spawn(inspect_java(candidate)));
    }
    let mut unique: HashMap<String, JavaInfo> = HashMap::new();
    for handle in handles {
        let Some(java) = handle.await.ok().flatten() else {
            continue;
        };
        unique.entry(java.path.to_lowercase()).or_insert(java);
    }
    let mut list: Vec<JavaInfo> = unique.into_values().collect();
    list.sort_by(compare_java_info);
    list
}

/// Pure post-exec check shared by `test_java`, mirroring the TS callback.
pub fn check_java_output(
    java: &str,
    exit_ok: bool,
    stdout: &str,
    stderr: &str,
) -> Result<String, anyhow::Error> {
    let output = format!("{stderr}{stdout}");
    if !exit_ok {
        anyhow::bail!("Java test failed ({java}):\n{output}");
    }
    if parse_java_runtime_details(&output).is_none() {
        anyhow::bail!(
            "Java test failed ({java}): executable did not report a valid Java runtime.\n{output}"
        );
    }
    Ok(format!("OK - {java}\n{output}"))
}

/// Mirrors `testJava`.
pub async fn test_java(path_override: Option<&str>) -> Result<String, anyhow::Error> {
    let java = match path_override.map(str::trim).filter(|s| !s.is_empty()) {
        Some(explicit) => PathBuf::from(explicit),
        None => java_path().ok_or_else(|| {
            anyhow::anyhow!("no Java configured or found - set JAVA_HOME or pick a Java path")
        })?,
    };
    let label = java.to_string_lossy().into_owned();
    let output = tokio::time::timeout(
        INSPECT_TIMEOUT,
        runtime_command(&java).arg("-version").output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Java test failed ({label}): timed out"))?
    .map_err(|e| anyhow::anyhow!("Java test failed ({label}): {e}"))?;
    check_java_output(
        &label,
        output.status.success(),
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
    )
}

/// Mirrors `javaGuiExecutable`: on Windows prefer `javaw.exe` when present.
pub fn java_gui_executable(java: &Path) -> PathBuf {
    if !cfg!(windows) {
        return java.to_path_buf();
    }
    let text = java.to_string_lossy();
    if !patterns().gui_name.is_match(&text) {
        return java.to_path_buf();
    }
    let javaw = java.with_file_name("javaw.exe");
    if javaw.is_file() {
        javaw
    } else {
        java.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[tokio::test]
    async fn java_runtime_probe_has_no_windows_console_and_captures_output() {
        // Run the real process builder against a console-subsystem executable.
        // The helper uses GetConsoleWindow inside the child, requiring no GUI
        // automation or installed JVM. Both Java metadata call sites use this
        // same builder. With CREATE_NO_WINDOW removed this detects a console.
        let output = runtime_command(&std::env::current_exe().unwrap())
            .args([
                "--exact",
                "java::tests::windows_console_probe_child",
                "--nocapture",
            ])
            .env("INDUSTRIALIS_JAVA_CONSOLE_PROBE", "1")
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("java-probe-console=none"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("java-probe-stderr=captured"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_console_probe_child() {
        if std::env::var("INDUSTRIALIS_JAVA_CONSOLE_PROBE").as_deref() != Ok("1") {
            return;
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetConsoleWindow() -> *mut std::ffi::c_void;
        }
        assert!(
            unsafe { GetConsoleWindow() }.is_null(),
            "Java probe attached a Windows console"
        );
        println!("java-probe-console=none");
        eprintln!("java-probe-stderr=captured");
    }

    const ADOPTIUM_OUTPUT: &str = "
Property settings:
    java.vendor = Eclipse Adoptium
    java.version = 21.0.5
    os.arch = amd64
openjdk version \"21.0.5\" 2024-10-15 LTS
";

    #[test]
    fn reads_runtime_details_from_modern_properties() {
        assert_eq!(
            parse_java_runtime_details(
                "\nProperty settings:\n    java.vendor = Oracle Corporation\n    java.version = 21.0.5\n    os.arch = amd64\njava version \"21.0.5\" 2024-10-15 LTS\n"
            ),
            Some(JavaRuntimeDetails {
                version: "21.0.5".to_string(),
                major_version: 21,
                architecture: "amd64".to_string(),
                vendor: "Oracle Corporation".to_string(),
            })
        );
    }

    #[test]
    fn normalizes_legacy_java_8_to_major_8() {
        let details = parse_java_runtime_details(
            "\n    java.vendor = Oracle Corporation\n    java.version = 1.8.0_51\n    os.arch = x86\njava version \"1.8.0_51\"\n",
        )
        .unwrap();
        assert_eq!(details.version, "1.8.0_51");
        assert_eq!(details.major_version, 8);
        assert_eq!(details.architecture, "x86");
    }

    #[test]
    fn normalizes_architectures() {
        assert_eq!(normalize_architecture(Some("x86_64")), "amd64");
        assert_eq!(normalize_architecture(Some("aarch64")), "arm64");
        assert_eq!(normalize_architecture(Some("i686")), "x86");
        assert_eq!(normalize_architecture(None), "unknown");
    }

    #[test]
    fn accepts_runtime_output_and_rejects_non_java() {
        assert!(
            check_java_output("C:\\Java\\bin\\java.exe", true, "", ADOPTIUM_OUTPUT)
                .unwrap()
                .contains("OK - C:\\Java\\bin\\java.exe")
        );
        let err = check_java_output("/bin/true", true, "", "").unwrap_err();
        assert!(err
            .to_string()
            .contains("did not report a valid Java runtime"));
        let err = check_java_output("/bin/false", false, "", "boom").unwrap_err();
        assert!(err.to_string().starts_with("Java test failed (/bin/false)"));
    }

    #[test]
    fn resolves_java_from_home_layouts() {
        let root =
            std::env::temp_dir().join(format!("industrialis-java-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bin = root.join("jdk-21").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let exe = bin.join(exe_name());
        std::fs::write(&exe, "fake").unwrap();
        assert_eq!(
            java_from_home(root.join("jdk-21").to_str().unwrap()),
            Some(exe.clone())
        );
        // A `bin` dir passed directly also resolves.
        assert_eq!(java_from_home(bin.to_str().unwrap()), Some(exe));
        assert_eq!(java_from_home(root.join("missing").to_str().unwrap()), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sorts_newest_major_first_then_path() {
        let info = |path: &str, version: &str, major: u32| JavaInfo {
            path: path.to_string(),
            version: version.to_string(),
            major_version: major,
            architecture: "amd64".to_string(),
            vendor: "v".to_string(),
        };
        let mut list = vec![
            info("b", "17.0.1", 17),
            info("a", "21.0.5", 21),
            info("c", "21.0.10", 21),
        ];
        list.sort_by(compare_java_info);
        let paths: Vec<&str> = list.iter().map(|j| j.path.as_str()).collect();
        assert_eq!(paths, vec!["c", "a", "b"]);
    }
}
