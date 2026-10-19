//! Atomic, per-instance operation reservations shared by UI and CLI services.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::paths::LauncherPaths;

fn reservations() -> &'static Mutex<HashSet<PathBuf>> {
    static RESERVED: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    RESERVED.get_or_init(|| Mutex::new(HashSet::new()))
}

pub struct InstanceLease {
    keys: Vec<PathBuf>,
    _work: crate::lifecycle::WorkLease,
}

/// Reserve every source/destination together, before any asynchronous work.
/// The RAII lease releases on success, failure, or task cancellation.
pub fn reserve_instances(paths: &LauncherPaths, ids: &[&str]) -> anyhow::Result<InstanceLease> {
    let work = crate::lifecycle::admit(paths)?;
    let mut keys = Vec::new();
    for id in ids {
        let absolute = std::path::absolute(paths.instance_dir(id)?)?;
        #[cfg(windows)]
        let absolute = PathBuf::from(absolute.to_string_lossy().to_lowercase());
        if !keys.contains(&absolute) {
            keys.push(absolute);
        }
    }
    let mut reserved = reservations().lock().unwrap();
    if keys.iter().any(|key| reserved.contains(key)) {
        anyhow::bail!("An operation is already in progress for this instance.");
    }
    reserved.extend(keys.iter().cloned());
    Ok(InstanceLease { keys, _work: work })
}

impl Drop for InstanceLease {
    fn drop(&mut self) {
        let mut reserved = reservations().lock().unwrap();
        for key in &self.keys {
            reserved.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_and_destination_are_reserved_atomically_and_released() {
        let paths = LauncherPaths::new(std::env::temp_dir().join("industrialis-lease-atomic"));
        let source = reserve_instances(&paths, &["source"]).unwrap();
        assert!(reserve_instances(&paths, &["destination", "source"]).is_err());
        let destination = reserve_instances(&paths, &["destination"]).unwrap();
        drop((source, destination));
        let both = reserve_instances(&paths, &["source", "destination"]).unwrap();
        assert!(reserve_instances(&paths, &["source"]).is_err());
        assert!(reserve_instances(&paths, &["destination"]).is_err());
        drop(both);
        assert!(reserve_instances(&paths, &["source", "destination"]).is_ok());
    }
}
