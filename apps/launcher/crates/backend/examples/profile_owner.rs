//! Headless process fixture for the cross-runtime ownership tests. Explicit
//! --data-dir equivalent is mandatory: this never selects a user's live profile.
use industrialis_backend::{
    paths::LauncherPaths,
    profile_owner::{acquire, Acquisition, OwnerKind},
};
use std::{
    io::{self, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
fn main() -> anyhow::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("isolated data directory required"))?;
    let kind = if std::env::args().nth(2).as_deref() == Some("operation") {
        OwnerKind::Operation
    } else {
        OwnerKind::Launcher
    };
    match acquire(&LauncherPaths::new(path.into()), kind)? {
        Acquisition::Existing(kind) => println!("EXISTING {kind:?}"),
        Acquisition::Primary(owner) => {
            println!("PRIMARY");
            io::stdout().flush()?;
            let stop = Arc::new(AtomicBool::new(false));
            let done = stop.clone();
            std::thread::spawn(move || {
                let mut input = String::new();
                let _ = io::stdin().read_line(&mut input);
                done.store(true, Ordering::Release);
            });
            let signal = owner.activation_signal();
            while !stop.load(Ordering::Acquire) {
                if signal.swap(false, Ordering::AcqRel) {
                    println!("FOCUS");
                    io::stdout().flush()?;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    Ok(())
}
