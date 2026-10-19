//! Keep platform/renderer failures visible in packaged stderr captures.
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::io::Write;

struct Diagnostics;

impl Log for Diagnostics {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        // Renderer selection helps diagnose driver failures without enabling
        // verbose network, account or per-frame debug logging.
        metadata.level() <= Level::Info
            && (metadata.target().starts_with("gpui") || metadata.target().starts_with("wgpu"))
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            let _ = writeln!(
                std::io::stderr(),
                "[{} {}] {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

pub fn init() {
    static LOGGER: Diagnostics = Diagnostics;
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
}
