//! Numeric-aware string ordering, mirroring
//! `String.localeCompare(other, undefined, { numeric: true })` for the
//! version keys, file names, and Java versions this launcher compares.
//!
//! Digit runs compare numerically, other runs compare by code point. Shared
//! by pack-version ordering and Java detection sorting.

use std::cmp::Ordering;

fn is_digit_char(c: char) -> bool {
    c.is_ascii_digit()
}

/// Positive when `left` sorts after `right` (ascending order).
pub fn compare_natural(left: &str, right: &str) -> Ordering {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn orders_digit_runs_numerically() {
        assert_eq!(compare_natural("2.8.9", "2.8.10"), Ordering::Less);
        assert_eq!(compare_natural("2.8.10", "2.8.9"), Ordering::Greater);
        assert_eq!(compare_natural("21.0.5", "21.0.5"), Ordering::Equal);
    }

    #[test]
    fn orders_non_digit_runs_by_code_point() {
        assert_eq!(compare_natural("2.9.0-alpha", "2.9.0-beta"), Ordering::Less);
    }
}
