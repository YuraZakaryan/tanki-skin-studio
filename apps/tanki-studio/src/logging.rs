// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's logger: `log::` records go to standard error and to a log file.
//!
//! A launch from Finder or the Start menu has no terminal, so without the file every warning (a
//! setting that could not be saved, the panic hook's report) vanished. The file is
//! `<data folder>/logs/tanki-studio.log`; each start moves the previous one to
//! `tanki-studio.1.log` (and that one to `.2`), so the log of a run that crashed survives the next
//! launch, and one file never grows past [`MAX_FILE_BYTES`].
//!
//! Levels: `info` for Tanki's own crates, `warn` for everything else (wgpu and naga are chatty).
//! `RUST_LOG` replaces that with env_logger-style directives: `debug`, `warn,tanki_apply=trace`;
//! a directive ending in `*` matches every target starting with it (`tanki*=debug`).
//!
//! Records logged before the data folder is known are kept (up to [`MAX_PENDING`]) and written
//! once the file is attached. Logging never panics: a file that can't be created or written leaves
//! standard error as the only sink.

use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock, PoisonError},
    time::SystemTime,
};

use log::LevelFilter;

/// The current log file's name inside the log folder.
pub const LOG_FILE: &str = "tanki-studio.log";
/// How many previous logs are kept (`tanki-studio.1.log` … `tanki-studio.<KEEP>.log`).
pub const KEEP: usize = 2;
/// The log file stops growing past this size (a runaway warning can't fill the disk).
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// Records kept in memory until the log file is attached.
pub const MAX_PENDING: usize = 512;
/// The built-in filter when `RUST_LOG` is unset or empty.
pub const DEFAULT_FILTER: &str = "warn,tanki*=info";

/// Per-target level filter parsed from env_logger-style directives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    default: LevelFilter,
    directives: Vec<(String, LevelFilter)>,
}

impl Filter {
    /// Parse `spec`; unknown levels and empty parts are skipped. Without a bare level, targets no
    /// directive names log errors only.
    pub fn parse(spec: &str) -> Filter {
        let mut filter = Filter {
            default: LevelFilter::Error,
            directives: Vec::new(),
        };
        for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            match part.split_once('=') {
                Some((name, level)) => {
                    let name = name.trim();
                    if let (false, Some(level)) = (name.is_empty(), parse_level(level)) {
                        filter.directives.push((name.to_owned(), level));
                    }
                }
                None => match parse_level(part) {
                    Some(level) => filter.default = level,
                    // A bare target name: everything from it (env_logger does the same).
                    None => filter
                        .directives
                        .push((part.to_owned(), LevelFilter::Trace)),
                },
            }
        }
        filter
    }

    /// The level that applies to `target` (the most specific matching directive wins).
    pub fn level_for(&self, target: &str) -> LevelFilter {
        self.directives
            .iter()
            .filter(|(name, _)| matches(name, target))
            .max_by_key(|(name, _)| name.len())
            .map_or(self.default, |(_, level)| *level)
    }

    /// The most verbose level any target can reach (for `log::set_max_level`).
    pub fn max(&self) -> LevelFilter {
        self.directives
            .iter()
            .map(|(_, level)| *level)
            .fold(self.default, Ord::max)
    }
}

fn parse_level(s: &str) -> Option<LevelFilter> {
    s.trim().parse().ok()
}

/// `name` matches `target` itself and its modules (`a` matches `a` and `a::b`, not `ab`);
/// `name*` matches every target starting with `name`.
fn matches(name: &str, target: &str) -> bool {
    match name.strip_suffix('*') {
        Some(prefix) => target.starts_with(prefix),
        None => target
            .strip_prefix(name)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with("::")),
    }
}

/// `2026-10-29T07:59:17.728Z` (UTC, milliseconds). A clock before 1970 or past chrono's range
/// reads as the epoch rather than panicking.
pub fn timestamp(time: SystemTime) -> String {
    let millis = time
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_millis()).ok())
        .and_then(chrono::DateTime::from_timestamp_millis)
        .unwrap_or_default();
    millis.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn numbered(dir: &Path, n: usize) -> PathBuf {
    if n == 0 {
        dir.join(LOG_FILE)
    } else {
        dir.join(format!("tanki-studio.{n}.log"))
    }
}

/// Shift the previous logs up one (`.log` → `.1.log` → … → `.<KEEP>.log`, the oldest dropped)
/// and create a fresh, empty log file in `dir` (created if missing).
pub fn rotate(dir: &Path) -> std::io::Result<(PathBuf, File)> {
    std::fs::create_dir_all(dir)?;
    for n in (1..=KEEP).rev() {
        match std::fs::rename(numbered(dir, n - 1), numbered(dir, n)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
    }
    let path = numbered(dir, 0);
    let file = File::create(&path)?;
    Ok((path, file))
}

/// One formatted log line: `<timestamp> <LEVEL> [<thread>] <target>: <message>\n`.
pub fn format_line(
    time: &str,
    level: log::Level,
    thread: &str,
    target: &str,
    message: &str,
) -> String {
    let message = message.strip_suffix('\n').unwrap_or(message);
    format!("{time} {level:<5} [{thread}] {target}: {message}\n")
}

/// Where formatted lines go besides standard error: kept in memory until a file is attached,
/// then appended to it until it reaches its size cap.
pub struct Sink {
    file: Option<File>,
    pending: Vec<String>,
    dropped: usize,
    written: u64,
    max_bytes: u64,
    capped: bool,
}

impl Sink {
    pub fn new(max_bytes: u64) -> Sink {
        Sink {
            file: None,
            pending: Vec::new(),
            dropped: 0,
            written: 0,
            max_bytes,
            capped: false,
        }
    }

    /// Append `line` to the file, or keep it until [`Sink::attach`].
    pub fn write(&mut self, line: &str) {
        if self.file.is_none() {
            if self.pending.len() < MAX_PENDING {
                self.pending.push(line.to_owned());
            } else {
                self.dropped = self.dropped.saturating_add(1);
            }
            return;
        }
        self.append(line);
    }

    /// Start writing to `file`, first the lines kept so far.
    pub fn attach(&mut self, file: File) {
        self.file = Some(file);
        for line in std::mem::take(&mut self.pending) {
            self.append(&line);
        }
        if self.dropped > 0 {
            let note = format!(
                "{} earlier log lines were dropped before the log file was opened\n",
                self.dropped
            );
            self.dropped = 0;
            self.append(&note);
        }
    }

    fn append(&mut self, line: &str) {
        if self.capped {
            return;
        }
        let Some(file) = self.file.as_mut() else {
            return;
        };
        let len = u64::try_from(line.len()).unwrap_or(u64::MAX);
        let text = if self.written.saturating_add(len) > self.max_bytes {
            self.capped = true;
            format!(
                "log file reached {} bytes; later records go to standard error only\n",
                self.max_bytes
            )
        } else {
            self.written = self.written.saturating_add(len);
            line.to_owned()
        };
        // A full disk or a vanished file must not take the app down: drop the file sink.
        if file.write_all(text.as_bytes()).is_err() {
            self.file = None;
            self.capped = true;
        }
    }
}

/// The installed logger: a [`Filter`], standard error and a [`Sink`].
pub struct AppLogger {
    filter: Filter,
    stderr: bool,
    sink: Mutex<Sink>,
}

impl AppLogger {
    pub fn new(filter: Filter, stderr: bool) -> AppLogger {
        AppLogger {
            filter,
            stderr,
            sink: Mutex::new(Sink::new(MAX_FILE_BYTES)),
        }
    }

    /// Rotate the logs in `dir` and send records to the fresh file; returns its path.
    pub fn attach_dir(&self, dir: &Path) -> Result<PathBuf, String> {
        let (path, file) = rotate(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        self.sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .attach(file);
        Ok(path)
    }
}

impl log::Log for AppLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= self.filter.level_for(metadata.target())
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let thread = std::thread::current();
        let line = format_line(
            &timestamp(SystemTime::now()),
            record.level(),
            thread.name().unwrap_or("?"),
            record.target(),
            &record.args().to_string(),
        );
        if self.stderr {
            // Unlike `eprintln!`, never panics on a closed standard error.
            let _ = std::io::stderr().write_all(line.as_bytes());
        }
        self.sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .write(&line);
    }

    fn flush(&self) {}
}

/// Install the logger (filter from `RUST_LOG`, else [`DEFAULT_FILTER`]); `None` when another
/// logger was installed first. Call [`AppLogger::attach_dir`] once the data folder is known.
pub fn install() -> Option<&'static AppLogger> {
    static LOGGER: OnceLock<AppLogger> = OnceLock::new();
    let spec = std::env::var("RUST_LOG")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let filter = Filter::parse(spec.as_deref().unwrap_or(DEFAULT_FILTER));
    let max = filter.max();
    let logger = LOGGER.get_or_init(|| AppLogger::new(filter, true));
    log::set_logger(logger).ok()?;
    log::set_max_level(max);
    Some(logger)
}

/// Log every panic, caught or not, with a backtrace when `RUST_BACKTRACE` is set: the log file
/// of a crashed run is what a bug report can attach.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let trace = std::backtrace::Backtrace::capture();
        let report = if trace.status() == std::backtrace::BacktraceStatus::Captured {
            format!("internal error: {info}\n{trace}")
        } else {
            format!("internal error: {info}")
        };
        if log::log_enabled!(target: "tanki_studio", log::Level::Error) {
            log::error!(target: "tanki_studio", "{report}");
        } else {
            // `eprintln!` panics on a broken pipe, and a panic inside the panic hook aborts.
            let _ = writeln!(std::io::stderr(), "tanki-studio: {report}");
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::Log;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, UNIX_EPOCH},
    };

    fn scratch(label: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "tanki-logging-{label}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    fn record(logger: &AppLogger, level: log::Level, target: &str, message: &str) {
        logger.log(
            &log::Record::builder()
                .level(level)
                .target(target)
                .args(format_args!("{message}"))
                .build(),
        );
    }

    #[test]
    fn the_default_filter_shows_tanki_info_and_other_crates_warnings() {
        let filter = Filter::parse(DEFAULT_FILTER);
        assert_eq!(filter.level_for("tanki_studio"), LevelFilter::Info);
        assert_eq!(
            filter.level_for("tanki_apply::recognize"),
            LevelFilter::Info
        );
        assert_eq!(filter.level_for("wgpu_core::device"), LevelFilter::Warn);
        assert_eq!(filter.level_for("naga"), LevelFilter::Warn);
        assert_eq!(filter.max(), LevelFilter::Info);
    }

    #[test]
    fn directives_follow_env_logger_and_the_most_specific_one_wins() {
        let filter =
            Filter::parse("info,wgpu_core=error,tanki_apply=trace,tanki_apply::geometry=off");
        assert_eq!(filter.level_for("eframe"), LevelFilter::Info);
        assert_eq!(filter.level_for("wgpu_core::instance"), LevelFilter::Error);
        assert_eq!(filter.level_for("tanki_apply::apply"), LevelFilter::Trace);
        assert_eq!(filter.level_for("tanki_apply::geometry"), LevelFilter::Off);
        assert_eq!(filter.max(), LevelFilter::Trace);
        // A module name is matched at `::` boundaries, not as a bare prefix.
        assert_eq!(filter.level_for("tanki_apply_extra"), LevelFilter::Info);
        // A bare target name logs everything from it, as env_logger does.
        assert_eq!(
            Filter::parse("naga").level_for("naga::front"),
            LevelFilter::Trace
        );
        assert_eq!(
            Filter::parse("naga").level_for("eframe"),
            LevelFilter::Error
        );
        assert_eq!(
            Filter::parse("tanki*=debug").level_for("tanki_pack::archive"),
            LevelFilter::Debug
        );
    }

    #[test]
    fn hostile_specs_never_panic_and_fall_back_to_errors() {
        let long = "x".repeat(100_000);
        let odd = [
            "",
            ",,,",
            "=",
            "=debug",
            "tanki=",
            "nonsense=loud",
            "🦀=info",
            "*",
            "a=b=c",
        ];
        for spec in odd.into_iter().chain([long.as_str()]) {
            let filter = Filter::parse(spec);
            let _ = (
                filter.level_for("tanki_studio"),
                filter.level_for(""),
                filter.max(),
            );
        }
        assert_eq!(
            Filter::parse("nonsense=loud").level_for("nonsense"),
            LevelFilter::Error
        );
        assert_eq!(Filter::parse("=debug").max(), LevelFilter::Error);
        assert_eq!(
            Filter::parse("warn,,tanki_studio=DEBUG").level_for("tanki_studio"),
            LevelFilter::Debug
        );
    }

    #[test]
    fn timestamps_are_utc_with_milliseconds_and_a_wrong_clock_never_panics() {
        assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            timestamp(UNIX_EPOCH + Duration::from_millis(1_791_446_357_728)),
            "2026-10-08T07:59:17.728Z"
        );
        assert_eq!(
            timestamp(UNIX_EPOCH + Duration::from_secs(1_709_164_800)),
            "2024-02-29T00:00:00.000Z"
        );
        assert_eq!(
            timestamp(UNIX_EPOCH - Duration::from_secs(5)),
            "1970-01-01T00:00:00.000Z"
        );
        if let Some(far) = UNIX_EPOCH.checked_add(Duration::from_secs(u64::MAX / 2)) {
            assert_eq!(timestamp(far), "1970-01-01T00:00:00.000Z");
        }
    }

    #[test]
    fn rotation_keeps_two_previous_logs_and_starts_an_empty_file() {
        let dir = scratch("rotate");
        for run in 1..=4 {
            let (path, _file) = rotate(&dir).unwrap();
            assert_eq!(path, dir.join(LOG_FILE));
            assert_eq!(read(&path), "");
            std::fs::write(&path, format!("run {run}")).unwrap();
        }
        assert_eq!(read(&dir.join(LOG_FILE)), "run 4");
        assert_eq!(read(&dir.join("tanki-studio.1.log")), "run 3");
        assert_eq!(read(&dir.join("tanki-studio.2.log")), "run 2");
        assert!(
            !dir.join("tanki-studio.3.log").exists(),
            "only {KEEP} old logs are kept"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_line_carries_time_level_thread_target_and_message() {
        assert_eq!(
            format_line(
                "2026-10-29T07:59:17.728Z",
                log::Level::Warn,
                "main",
                "tanki_studio::actions",
                "could not save the theme: disk full"
            ),
            "2026-10-29T07:59:17.728Z WARN  [main] tanki_studio::actions: could not save the theme: disk full\n"
        );
        // A multi-line message (a panic with a backtrace) keeps its lines and ends in one newline.
        assert!(format_line("t", log::Level::Error, "w", "x", "a\nb\n").ends_with("x: a\nb\n"));
    }

    #[test]
    fn lines_logged_before_the_file_exists_are_written_when_it_is_attached() {
        let dir = scratch("pending");
        let (path, file) = rotate(&dir).unwrap();
        let mut sink = Sink::new(MAX_FILE_BYTES);
        sink.write("early 1\n");
        sink.write("early 2\n");
        sink.attach(file);
        sink.write("late\n");
        assert_eq!(read(&path), "early 1\nearly 2\nlate\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_pending_buffer_is_bounded_and_says_how_much_it_dropped() {
        let dir = scratch("bounded");
        let (path, file) = rotate(&dir).unwrap();
        let mut sink = Sink::new(MAX_FILE_BYTES);
        for i in 0..MAX_PENDING + 5 {
            sink.write(&format!("line {i}\n"));
        }
        sink.attach(file);
        let text = read(&path);
        assert_eq!(
            text.lines().filter(|l| l.starts_with("line ")).count(),
            MAX_PENDING
        );
        assert_eq!(
            text.matches("5 earlier log lines were dropped").count(),
            1,
            "{text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_file_stops_at_its_size_cap_with_one_note() {
        let dir = scratch("cap");
        let (path, file) = rotate(&dir).unwrap();
        let mut sink = Sink::new(100);
        sink.attach(file);
        for i in 0..50 {
            sink.write(&format!("line {i:02} with some padding\n"));
        }
        let text = read(&path);
        assert!(
            text.lines().filter(|l| l.starts_with("line ")).count() < 5,
            "{text}"
        );
        assert_eq!(text.matches("log file reached").count(), 1, "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_logger_filters_records_and_writes_them_to_the_attached_file() {
        let dir = scratch("logger");
        let logger = AppLogger::new(Filter::parse(DEFAULT_FILTER), false);
        record(&logger, log::Level::Info, "tanki_studio", "before the file");
        record(&logger, log::Level::Info, "wgpu_core::device", "too chatty");
        let path = logger.attach_dir(&dir).unwrap();
        record(
            &logger,
            log::Level::Warn,
            "wgpu_hal::metal",
            "a real warning",
        );
        record(&logger, log::Level::Debug, "tanki_studio", "below info");
        let text = read(&path);
        assert!(text.contains("tanki_studio: before the file\n"), "{text}");
        assert!(text.contains("wgpu_hal::metal: a real warning\n"), "{text}");
        assert!(
            !text.contains("too chatty") && !text.contains("below info"),
            "{text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unusable_log_folder_is_an_error_and_logging_goes_on() {
        let dir = scratch("unusable");
        std::fs::write(&dir, "a file where the folder should be").unwrap();
        let logger = AppLogger::new(Filter::parse("info"), false);
        assert!(logger.attach_dir(&dir).is_err());
        record(&logger, log::Level::Error, "tanki_studio", "still fine");
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn a_poisoned_lock_does_not_stop_logging() {
        let dir = scratch("poison");
        let logger = AppLogger::new(Filter::parse("info"), false);
        let path = logger.attach_dir(&dir).unwrap();
        std::thread::scope(|s| {
            let _ = s
                .spawn(|| {
                    let _guard = logger.sink.lock();
                    panic!("poison the sink");
                })
                .join();
        });
        assert!(logger.sink.is_poisoned());
        record(&logger, log::Level::Warn, "tanki_studio", "after the panic");
        assert!(read(&path).contains("tanki_studio: after the panic\n"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
