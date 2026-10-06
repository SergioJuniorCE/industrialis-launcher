//! Port of `apps/launcher/src/lib/pack-version-status.ts`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackVersionMeta {
    #[serde(rename = "title", default)]
    pub title: String,
    #[serde(rename = "releaseDate", default)]
    pub release_date: String,
    #[serde(rename = "maxJavaVersion", default)]
    pub max_java_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackUpdateStatus {
    Unknown,
    #[serde(rename = "up-to-date")]
    UpToDate,
    #[serde(rename = "update-available")]
    UpdateAvailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackVersionInfo {
    pub status: PackUpdateStatus,
    #[serde(rename = "currentVersion")]
    pub current_version: String,
    #[serde(rename = "latestVersion")]
    pub latest_version: Option<String>,
}

/// Mirrors `parseReleaseDate` + `Date.UTC(year, month - 1, day)`.
/// Returns 0 for malformed values, exactly like the TS version.
pub fn parse_release_date_ms(value: &str) -> i64 {
    let parts: Vec<&str> = value.trim().split(['/', '-']).collect();
    if parts.len() != 3 {
        return 0;
    }
    let mut nums = [0i64; 3];
    for (i, part) in parts.iter().enumerate() {
        match part.parse::<i64>() {
            Ok(n) => nums[i] = n,
            Err(_) => return 0,
        }
    }
    let (mut year, mut month, day) = (nums[0], nums[1], nums[2]);
    // Normalize month overflow the way Date.UTC does (month is 1-based here).
    year += month.div_euclid(12);
    month = month.rem_euclid(12);
    if month == 0 {
        month = 12;
        year -= 1;
    }
    days_from_civil(year, month as u32, day) * 86_400_000
}

/// Howard Hinnant's days-from-civil algorithm (days since 1970-01-01).
fn days_from_civil(year: i64, month: u32, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = ((month as i64) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn is_digit_char(c: char) -> bool {
    c.is_ascii_digit()
}

/// Approximation of `String.localeCompare(other, undefined, { numeric: true })`
/// for version keys (digits, dots, hyphens, ascii letters): digit runs compare
/// numerically, other runs compare by code point.
fn compare_natural(left: &str, right: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut l = left.chars().peekable();
    let mut r = right.chars().peekable();
    loop {
        match (l.peek(), r.peek()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(&lc), Some(&rc)) => {
                if is_digit_char(lc) && is_digit_char(rc) {
                    let mut lnum = String::new();
                    let mut rnum = String::new();
                    while let Some(&c) = l.peek() {
                        if !is_digit_char(c) {
                            break;
                        }
                        lnum.push(c);
                        l.next();
                    }
                    while let Some(&c) = r.peek() {
                        if !is_digit_char(c) {
                            break;
                        }
                        rnum.push(c);
                        r.next();
                    }
                    let ltrim = lnum.trim_start_matches('0');
                    let rtrim = rnum.trim_start_matches('0');
                    match ltrim.len().cmp(&rtrim.len()).then(ltrim.cmp(rtrim)) {
                        Ordering::Equal => continue,
                        ord => return ord,
                    }
                } else {
                    match lc.cmp(&rc) {
                        Ordering::Equal => {
                            l.next();
                            r.next();
                        }
                        ord => return ord,
                    }
                }
            }
        }
    }
}

/// Mirrors `compareVersionsByReleaseDate`: newest-first ordering value.
/// Positive means `right` is newer than `left`.
pub fn compare_versions_by_release_date(
    left_key: &str,
    right_key: &str,
    versions: Option<&HashMap<String, PackVersionMeta>>,
) -> i64 {
    let left_date = parse_release_date_ms(versions.and_then(|v| v.get(left_key)).map(|m| m.release_date.as_str()).unwrap_or(""));
    let right_date = parse_release_date_ms(versions.and_then(|v| v.get(right_key)).map(|m| m.release_date.as_str()).unwrap_or(""));
    if left_date != right_date {
        return right_date - left_date;
    }
    // TS: `rightKey.localeCompare(leftKey, undefined, { numeric: true })`.
    match compare_natural(right_key, left_key) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

fn release_date_of(key: &str, versions: &HashMap<String, PackVersionMeta>) -> i64 {
    parse_release_date_ms(versions.get(key).map(|m| m.release_date.as_str()).unwrap_or(""))
}

pub fn get_latest_pack_version(versions: Option<&HashMap<String, PackVersionMeta>>) -> Option<String> {
    let versions = versions?;
    let mut keys: Vec<&String> = versions.keys().collect();
    if keys.is_empty() {
        return None;
    }
    // Newest first: later release date wins, ties broken by numeric key
    // order — the same rule as the TS `keys.sort(compare)[0]`.
    keys.sort_by(|a, b| {
        release_date_of(b, versions)
            .cmp(&release_date_of(a, versions))
            .then(compare_natural(b, a))
    });
    keys.into_iter().next().cloned()
}

pub fn get_pack_version_info(current_version: &str, versions: Option<&HashMap<String, PackVersionMeta>>) -> PackVersionInfo {
    let Some(latest) = get_latest_pack_version(versions) else {
        return PackVersionInfo { status: PackUpdateStatus::Unknown, current_version: current_version.to_string(), latest_version: None };
    };
    if compare_versions_by_release_date(current_version, &latest, versions) > 0 {
        PackVersionInfo {
            status: PackUpdateStatus::UpdateAvailable,
            current_version: current_version.to_string(),
            latest_version: Some(latest),
        }
    } else {
        PackVersionInfo {
            status: PackUpdateStatus::UpToDate,
            current_version: current_version.to_string(),
            latest_version: Some(latest),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> HashMap<String, PackVersionMeta> {
        [
            ("2.8.0", "2025/09/27"),
            ("2.8.4", "2025/06/08"),
            ("2.9.0-beta-1", "2026/01/15"),
        ]
        .into_iter()
        .map(|(k, d)| {
            (
                k.to_string(),
                PackVersionMeta { title: "r".to_string(), release_date: d.to_string(), max_java_version: 25 },
            )
        })
        .collect()
    }

    #[test]
    fn picks_newest_pack_as_latest() {
        let versions = catalog();
        assert_eq!(get_latest_pack_version(Some(&versions)).as_deref(), Some("2.9.0-beta-1"));
    }

    #[test]
    fn marks_older_instances_as_update_available() {
        let versions = catalog();
        assert_eq!(
            get_pack_version_info("2.8.0", Some(&versions)),
            PackVersionInfo {
                status: PackUpdateStatus::UpdateAvailable,
                current_version: "2.8.0".to_string(),
                latest_version: Some("2.9.0-beta-1".to_string()),
            }
        );
    }

    #[test]
    fn marks_latest_as_up_to_date() {
        let versions = catalog();
        assert_eq!(
            get_pack_version_info("2.9.0-beta-1", Some(&versions)),
            PackVersionInfo {
                status: PackUpdateStatus::UpToDate,
                current_version: "2.9.0-beta-1".to_string(),
                latest_version: Some("2.9.0-beta-1".to_string()),
            }
        );
    }

    #[test]
    fn unknown_catalog_is_unknown() {
        let info = get_pack_version_info("2.8.0", None);
        assert_eq!(info.status, PackUpdateStatus::Unknown);
        assert_eq!(info.latest_version, None);
    }

    #[test]
    fn numeric_ordering_prefers_10_over_9() {
        let mut versions = HashMap::new();
        for key in ["2.8.9", "2.8.10"] {
            versions.insert(
                key.to_string(),
                PackVersionMeta { title: "r".to_string(), release_date: "2025/01/01".to_string(), max_java_version: 25 },
            );
        }
        assert_eq!(get_latest_pack_version(Some(&versions)).as_deref(), Some("2.8.10"));
    }
}
