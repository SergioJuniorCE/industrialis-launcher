//! Watch-mode shutdown enters the same save guards/drain as a normal Quit.
//! This module is absent from release builds. Only an explicitly opted-in
//! debug process reads the private pipe supplied by the development runner.
use std::io::BufRead;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Default)]
pub struct DevControl(Arc<AtomicBool>);

impl DevControl {
    pub fn install() -> Self {
        let control = Self::default();
        if std::env::var("INDUSTRIALIS_DEV_WATCH").as_deref() == Ok("1") {
            let requested = control.0.clone();
            std::thread::spawn(move || read_requests(std::io::stdin().lock(), &requested));
        }
        control
    }

    pub fn requested(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }
}

fn read_requests(input: impl BufRead, requested: &AtomicBool) {
    for line in input.lines() {
        match line {
            Ok(line) if line == "industrialis-dev-restart" => {
                requested.store(true, Ordering::Release)
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    // If the runner exits unexpectedly, still ask to close safely.
    requested.store(true, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_and_pipe_disconnect_request_quit() {
        let control = DevControl::default();
        read_requests(&b"industrialis-dev-restart\n"[..], &control.0);
        assert!(control.requested());
        assert!(!control.requested());
        read_requests(&b""[..], &control.0);
        assert!(control.requested());
    }
}
