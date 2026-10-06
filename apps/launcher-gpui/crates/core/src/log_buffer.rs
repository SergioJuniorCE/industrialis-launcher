//! Port of `apps/launcher/src/lib/log-buffer.ts`.

pub const MAX_RETAINED_LOG_LINES: usize = 5_000;

pub fn take_log_tail<T: Clone>(items: &[T], max_lines: usize) -> Vec<T> {
    if max_lines == 0 {
        return Vec::new();
    }
    if items.len() > max_lines {
        items[items.len() - max_lines..].to_vec()
    } else {
        items.to_vec()
    }
}

pub fn append_log_tail<T: Clone>(current: &[T], incoming: &[T], max_lines: usize) -> Vec<T> {
    if max_lines == 0 {
        return Vec::new();
    }
    if incoming.len() >= max_lines {
        return incoming[incoming.len() - max_lines..].to_vec();
    }
    let retained_current = max_lines - incoming.len();
    let retained = if retained_current == 0 {
        &[][..]
    } else if current.len() > retained_current {
        &current[current.len() - retained_current..]
    } else {
        current
    };
    let mut out = Vec::with_capacity(retained.len() + incoming.len());
    out.extend_from_slice(retained);
    out.extend_from_slice(incoming);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_newest_lines_when_loading_persisted_log() {
        assert_eq!(take_log_tail(&[1, 2, 3, 4], 2), vec![3, 4]);
    }

    #[test]
    fn keeps_newest_lines_when_appending_batch() {
        assert_eq!(append_log_tail(&[1, 2, 3], &[4, 5], 4), vec![2, 3, 4, 5]);
    }

    #[test]
    fn handles_batch_larger_than_window() {
        assert_eq!(append_log_tail(&[1, 2], &[3, 4, 5], 2), vec![4, 5]);
    }

    #[test]
    fn empty_for_non_positive_window() {
        assert!(take_log_tail(&[1, 2], 0).is_empty());
        assert!(append_log_tail(&[1, 2], &[3, 4], 0).is_empty());
    }

    #[test]
    fn does_not_mutate_sources() {
        let current = vec![1, 2, 3];
        let incoming = vec![4, 5];
        assert_eq!(append_log_tail(&current, &incoming, 4), vec![2, 3, 4, 5]);
        assert_eq!(current, vec![1, 2, 3]);
        assert_eq!(incoming, vec![4, 5]);
    }
}
