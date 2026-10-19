//! Port of `apps/launcher/src/lib/launch-log.ts`.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogStream {
    Stdout,
    Stderr,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchLogLevel {
    Error,
    Warn,
    Info,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchLogLine {
    pub stream: LogStream,
    pub line: String,
}

struct Patterns {
    minecraft_level: Regex,
    error_crash_report: Regex,
    error_process_exit_prefix: Regex,
    error_process_exit_code: Regex,
    error_launch_failed: Regex,
    error_uncaught: Regex,
    error_exception_thread: Regex,
    error_caused_by: Regex,
    error_stack_frame: Regex,
    error_java_class: Regex,
    error_bracketed: Regex,
    error_trailing_bracket: Regex,
    error_exception_name: Regex,
    error_adding: Regex,
    error_failed_to: Regex,
    warn_warning_prefix: Regex,
    warn_bracket: Regex,
    launch_separator: Regex,
    without_exception: Regex,
}

fn patterns() -> &'static Patterns {
    static CELL: OnceLock<Patterns> = OnceLock::new();
    CELL.get_or_init(|| Patterns {
        minecraft_level: Regex::new(r"(?i)\[[^\]]*/(ERROR|FATAL|WARN)\]").unwrap(),
        error_crash_report: Regex::new(r"(?i)---- Minecraft Crash Report ----").unwrap(),
        error_process_exit_prefix: Regex::new(r"(?i)Process exited with code").unwrap(),
        error_process_exit_code: Regex::new(r"(?i)Process exited with code\s+(-?\d+)").unwrap(),
        error_launch_failed: Regex::new(r"(?i)launch failed").unwrap(),
        error_uncaught: Regex::new(r"(?i)An uncaught exception").unwrap(),
        error_exception_thread: Regex::new(r"(?i)Exception in thread").unwrap(),
        error_caused_by: Regex::new(r"(?i)\bCaused by:").unwrap(),
        error_stack_frame: Regex::new(r"^\s+at [\w.$/]").unwrap(),
        error_java_class: Regex::new(r"^java\.[\w.]+\b").unwrap(),
        error_bracketed: Regex::new(r"(?i)\[(ERROR|FATAL)\]").unwrap(),
        error_trailing_bracket: Regex::new(r"(?i)\b(ERROR|FATAL)\]").unwrap(),
        error_exception_name: Regex::new(r"\b\w+Exception\b").unwrap(),
        error_adding: Regex::new(r"(?i)\bError adding\b").unwrap(),
        error_failed_to: Regex::new(r"(?i)\bFailed to\b").unwrap(),
        warn_warning_prefix: Regex::new(r"(?i)^WARNING:").unwrap(),
        warn_bracket: Regex::new(r"(?i)\[WARN\]").unwrap(),
        launch_separator: Regex::new(r"─{3,}\s*Launch\s*─{3,}").unwrap(),
        without_exception: Regex::new(r"(?i)without exception").unwrap(),
    })
}

/// TS uses `/Process exited with code (?!0\b)/i`; the regex crate has no
/// lookahead, so the zero-exit exclusion is evaluated manually.
fn is_nonzero_exit(line: &str) -> bool {
    let p = patterns();
    if !p.error_process_exit_prefix.is_match(line) {
        return false;
    }
    match p.error_process_exit_code.captures(line).and_then(|c| c.get(1)) {
        // No parseable code (e.g. "code unknown") still counts as failure,
        // matching the TS lookahead which only excludes a literal `0`.
        None => true,
        Some(code) => code.as_str() != "0",
    }
}

fn minecraft_level(line: &str) -> Option<LaunchLogLevel> {
    let captures = patterns().minecraft_level.captures(line)?;
    match captures.get(1)?.as_str().to_uppercase().as_str() {
        "ERROR" | "FATAL" => Some(LaunchLogLevel::Error),
        "WARN" => Some(LaunchLogLevel::Warn),
        _ => None,
    }
}

fn is_error_line(line: &str, trimmed_start: &str) -> bool {
    let p = patterns();
    if p.error_crash_report.is_match(line) {
        return true;
    }
    if is_nonzero_exit(line) {
        return true;
    }
    if p.error_launch_failed.is_match(line) {
        return true;
    }
    if p.error_uncaught.is_match(line) {
        return true;
    }
    if p.error_exception_thread.is_match(line) {
        return true;
    }
    if p.error_caused_by.is_match(line) {
        return true;
    }
    if p.error_stack_frame.is_match(line) {
        return true;
    }
    if p.error_java_class.is_match(trimmed_start) {
        return true;
    }
    if p.error_bracketed.is_match(line) {
        return true;
    }
    if p.error_trailing_bracket.is_match(line) {
        return true;
    }
    if p.error_exception_name.is_match(line) && !p.without_exception.is_match(line) {
        return true;
    }
    if p.error_adding.is_match(line) {
        return true;
    }
    if p.error_failed_to.is_match(line) {
        return true;
    }
    false
}

fn is_warn_line(line: &str, trimmed_start: &str) -> bool {
    let p = patterns();
    if p.warn_warning_prefix.is_match(trimmed_start) {
        return true;
    }
    // Both TS checks (`\bWARN\]` and `\[WARN\]`) reduce to the bracket form.
    if p.warn_bracket.is_match(line) {
        return true;
    }
    false
}

pub fn classify_launch_log_line(entry: &LaunchLogLine) -> LaunchLogLevel {
    if let Some(level) = minecraft_level(&entry.line) {
        return level;
    }
    let trimmed_start = entry.line.trim_start().to_string();
    if is_error_line(&entry.line, &trimmed_start) {
        return LaunchLogLevel::Error;
    }
    if is_warn_line(&entry.line, &trimmed_start) {
        return LaunchLogLevel::Warn;
    }
    if entry.stream == LogStream::System {
        return LaunchLogLevel::System;
    }
    LaunchLogLevel::Info
}

pub fn format_launch_log(log: &[LaunchLogLine]) -> String {
    log.iter().map(|entry| entry.line.as_str()).collect::<Vec<_>>().join("\n")
}

pub const CRASH_EXCERPT_MAX_LINES: usize = 400;
pub const CRASH_EXCERPT_HEADER_LINES: usize = 15;

pub fn is_launch_start_line(line: &str) -> bool {
    patterns().launch_separator.is_match(line)
}

fn find_latest_launch_start(log: &[LaunchLogLine]) -> Option<usize> {
    log.iter().rposition(|entry| is_launch_start_line(&entry.line))
}

/// Lines of the most recent launch; whole log when no marker exists.
pub fn slice_latest_launch(log: &[LaunchLogLine]) -> Vec<LaunchLogLine> {
    match find_latest_launch_start(log) {
        Some(start) => log[start..].to_vec(),
        None => log.to_vec(),
    }
}

/// Copy-paste friendly excerpt of the latest launch for AI debugging.
pub fn extract_latest_crash_lines(log: &[LaunchLogLine], max_lines: usize) -> Vec<LaunchLogLine> {
    if log.is_empty() {
        return Vec::new();
    }
    let latest = slice_latest_launch(log);
    if latest.is_empty() {
        return Vec::new();
    }
    let limit = if max_lines > 1 { max_lines } else { CRASH_EXCERPT_MAX_LINES };
    if latest.len() <= limit {
        return latest;
    }
    let header_count = CRASH_EXCERPT_HEADER_LINES.min((limit / 4).max(2)).min(limit - 2);
    let header = latest[..header_count.max(0)].to_vec();
    let tail_count = limit - header.len() - 1;
    let tail = latest[latest.len() - tail_count..].to_vec();
    let truncated = latest.len() - header.len() - tail.len();
    let has_boundary = find_latest_launch_start(log).is_some();
    let marker = if has_boundary {
        format!("... [truncated {truncated} lines from latest launch — showing launch header + last {} lines for AI debugging] ...", tail.len())
    } else {
        format!("... [truncated {truncated} lines — showing first {} + last {} lines for AI debugging] ...", header.len(), tail.len())
    };
    let mut excerpt = header;
    excerpt.push(LaunchLogLine { stream: LogStream::System, line: marker });
    excerpt.extend(tail);
    excerpt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, stream: LogStream) -> LaunchLogLine {
        LaunchLogLine { stream, line: text.to_string() }
    }

    fn stdout(text: &str) -> LaunchLogLine {
        line(text, LogStream::Stdout)
    }

    #[test]
    fn classifies_fml_error_on_stdout_as_error() {
        assert_eq!(
            classify_launch_log_line(&stdout("[23:14:55] [Client thread/ERROR] [IC2]: signature mismatch")),
            LaunchLogLevel::Error
        );
    }

    #[test]
    fn classifies_fml_warn_on_stdout_as_warn() {
        assert_eq!(
            classify_launch_log_line(&stdout("[23:14:54] [Client thread/WARN] [mixin]: Error loading class")),
            LaunchLogLevel::Warn
        );
    }

    #[test]
    fn classifies_fml_info_on_stdout_as_info() {
        assert_eq!(
            classify_launch_log_line(&stdout("[23:14:55] [Client thread/INFO] [FML]: Forge Mod Loader")),
            LaunchLogLevel::Info
        );
    }

    #[test]
    fn classifies_java_exceptions_as_error() {
        assert_eq!(
            classify_launch_log_line(&stdout("java.lang.NullPointerException: Cannot invoke")),
            LaunchLogLevel::Error
        );
    }

    #[test]
    fn classifies_jdk_warning_on_stderr_as_warn() {
        assert_eq!(
            classify_launch_log_line(&line("WARNING: package sun.lwawt.macosx not in java.desktop", LogStream::Stderr)),
            LaunchLogLevel::Warn
        );
    }

    #[test]
    fn classifies_netty_info_on_stderr_as_info() {
        assert_eq!(
            classify_launch_log_line(&line("INFO: Your platform does not provide complete low-level API", LogStream::Stderr)),
            LaunchLogLevel::Info
        );
    }

    #[test]
    fn classifies_launcher_system_lines_as_system() {
        assert_eq!(
            classify_launch_log_line(&line("──────── Launch ────────", LogStream::System)),
            LaunchLogLevel::System
        );
    }

    #[test]
    fn classifies_nonzero_exit_as_error_but_not_zero_exit() {
        assert_eq!(
            classify_launch_log_line(&line("Process exited with code -1", LogStream::System)),
            LaunchLogLevel::Error
        );
        assert_eq!(
            classify_launch_log_line(&line("Process exited with code 0", LogStream::System)),
            LaunchLogLevel::System
        );
    }

    #[test]
    fn slices_latest_launch() {
        let log = vec![
            line("──────── Launch ────────", LogStream::System),
            stdout("old crash: java.lang.NullPointerException"),
            line("Process exited with code 1", LogStream::System),
            line("──────── Launch ────────", LogStream::System),
            line("Java: C:/java.exe", LogStream::System),
            stdout("new crash: java.lang.OutOfMemoryError"),
        ];
        let latest = slice_latest_launch(&log);
        let lines: Vec<&str> = latest.iter().map(|e| e.line.as_str()).collect();
        assert_eq!(lines, vec!["──────── Launch ────────", "Java: C:/java.exe", "new crash: java.lang.OutOfMemoryError"]);
    }

    #[test]
    fn returns_full_log_without_marker() {
        let log = vec![stdout("one"), stdout("two")];
        assert_eq!(slice_latest_launch(&log), log);
    }

    #[test]
    fn detects_backend_launch_separator() {
        assert!(is_launch_start_line("──────── Launch ────────"));
        assert!(!is_launch_start_line("Launch args saved to C:/instance/launch.arg"));
    }

    #[test]
    fn extracts_crash_excerpt() {
        assert!(extract_latest_crash_lines(&[], 400).is_empty());

        let log = vec![
            line("──────── Launch ────────", LogStream::System),
            stdout("---- Minecraft Crash Report ----"),
            stdout("java.lang.RuntimeException: crash"),
            line("Process exited with code 1", LogStream::System),
        ];
        assert_eq!(extract_latest_crash_lines(&log, 400), log);

        let mut latest_launch = vec![line("──────── Launch ────────", LogStream::System)];
        for index in 0..20 {
            latest_launch.push(stdout(&format!("filler {index}")));
        }
        latest_launch.push(stdout("java.lang.IllegalStateException: latest crash"));
        latest_launch.push(line("Process exited with code 1", LogStream::System));
        let mut full = vec![
            line("──────── Launch ────────", LogStream::System),
            stdout("old launch output"),
        ];
        full.extend(latest_launch);

        let excerpt = extract_latest_crash_lines(&full, 10);
        assert_eq!(excerpt.len(), 10);
        assert_eq!(excerpt[0].line, "──────── Launch ────────");
        assert!(excerpt.iter().any(|e| e.line.contains("truncated")));
        assert!(excerpt.iter().any(|e| e.line.contains("latest crash")));
        assert!(!excerpt.iter().any(|e| e.line.contains("old launch output")));
    }
}
