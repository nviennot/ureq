struct SimpleLogger;

impl ::log::Log for SimpleLogger {
    fn enabled(&self, _metadata: &::log::Metadata) -> bool {
        true
        // _metadata.target().starts_with("lolb")
    }

    fn log(&self, record: &::log::Record) {
        if self.enabled(record.metadata()) {
            println!("{} {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

use ::log::LevelFilter;

static LOGGER: SimpleLogger = SimpleLogger;

pub fn set_logger() {
    static INIT: ::std::sync::Once = ::std::sync::Once::new();
    INIT.call_once(|| {
        ::log::set_logger(&LOGGER)
            .map(|()| ::log::set_max_level(LevelFilter::Trace))
            .expect("Failed to set logger")
    });
}
