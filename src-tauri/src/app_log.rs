use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const ARCHIVE_COUNT: usize = 3;
const MAX_MESSAGE_BYTES: usize = 4096;
const MAX_ERROR_KEYS: usize = 64;
const ERROR_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Written,
    Suppressed,
}

struct ErrorEntry {
    message: String,
    written_at: Instant,
    suppressed: u64,
}

struct LogWriter {
    limit: u64,
    archives: usize,
    errors: VecDeque<ErrorEntry>,
}

impl Default for LogWriter {
    fn default() -> Self {
        Self {
            limit: MAX_FILE_BYTES,
            archives: ARCHIVE_COUNT,
            errors: VecDeque::new(),
        }
    }
}

impl LogWriter {
    fn write_at(
        &mut self,
        directory: &Path,
        level: Level,
        message: &str,
        timestamp: &str,
        now: Instant,
    ) -> std::io::Result<Outcome> {
        // Bound retained keys as well as output. Escaping controls prevents one
        // error from forging additional physical log lines.
        let timestamp = bounded_text(timestamp, 48);
        let prefix = format!("{timestamp} [Rust] [{}] ", level.label());
        let budget = (self.limit as usize)
            .saturating_sub(prefix.len() + 1)
            .min(MAX_MESSAGE_BYTES);
        let message = bounded_text(message, budget);
        let error_index = (level == Level::Error)
            .then(|| {
                self.errors
                    .iter()
                    .position(|entry| entry.message == message)
            })
            .flatten();
        let mut summary = String::new();
        if let Some(index) = error_index {
            if now.saturating_duration_since(self.errors[index].written_at) < ERROR_INTERVAL {
                let mut entry = self.errors.remove(index).unwrap();
                entry.suppressed = entry.suppressed.saturating_add(1);
                self.errors.push_back(entry);
                return Ok(Outcome::Suppressed);
            }
            let count = self.errors[index].suppressed;
            if count > 0 {
                summary = format!("[suppressed {count} repeats] ");
            }
        }
        // The summary and timestamp are included in the same bounded record.
        let body = bounded_text(&format!("{summary}{message}"), budget);
        let line = format!("{prefix}{body}\n");
        if line.len() as u64 > self.limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "log policy is smaller than its record prefix",
            ));
        }
        append_bounded(directory, line.as_bytes(), self.limit, self.archives)?;
        // Failed writes do not advance the limiter or lose a suppressed count.
        if level == Level::Error {
            if let Some(index) = error_index {
                self.errors.remove(index);
            }
            if self.errors.len() >= MAX_ERROR_KEYS {
                self.errors.pop_front();
            }
            self.errors.push_back(ErrorEntry {
                message,
                written_at: now,
                suppressed: 0,
            });
        }
        Ok(Outcome::Written)
    }
}

pub fn write(directory: &Path, level: Level, message: &str, timestamp: &str) {
    static WRITER: OnceLock<Mutex<LogWriter>> = OnceLock::new();
    let mut writer = WRITER
        .get_or_init(|| Mutex::new(LogWriter::default()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Err(error) = writer.write_at(directory, level, message, timestamp, Instant::now()) {
        // Logging is best-effort and must never interfere with brightness
        // recovery. Do not recursively try to log a logging failure.
        eprintln!("fishcreen log unavailable: {error}");
    }
}

fn bounded_text(value: &str, budget: usize) -> String {
    const TRUNCATED: &str = " [truncated]";
    let mut result = String::with_capacity(value.len().min(budget));
    for character in value.chars() {
        let mut encoded = [0; 4];
        let text = match character {
            '\n' => "\\n",
            '\r' => "\\r",
            '\0' => "\\0",
            other => other.encode_utf8(&mut encoded),
        };
        if result.len() + text.len() > budget {
            while result.len() + TRUNCATED.len() > budget && !result.is_empty() {
                result.pop();
            }
            result.push_str(&TRUNCATED[..TRUNCATED.len().min(budget.saturating_sub(result.len()))]);
            break;
        }
        result.push_str(text);
    }
    result
}

fn archive_path(directory: &Path, number: usize) -> PathBuf {
    directory.join(format!("app.log.{number}"))
}

fn bounded_tail(path: &Path, limit: u64) -> std::io::Result<Option<Vec<u8>>> {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let length = file.metadata()?.len();
    let start = length.saturating_sub(limit);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity(length.min(limit) as usize);
    file.take(limit).read_to_end(&mut bytes)?;
    // Do not start a retained UTF-8 tail in the middle of a code point.
    if start > 0 {
        let offset = bytes
            .iter()
            .position(|byte| byte & 0xc0 != 0x80)
            .unwrap_or(bytes.len());
        bytes.drain(..offset);
    }
    Ok(Some(bytes))
}

fn atomic_log_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    crate::config::atomic_write(path, bytes).map_err(std::io::Error::other)
}

fn rotate(directory: &Path, limit: u64, archives: usize) -> std::io::Result<()> {
    let current = directory.join("app.log");
    // Obtain the current evidence first, with bounded memory even for an old,
    // oversized app.log. The original remains untouched on archive failures.
    let Some(bytes) = bounded_tail(&current, limit)? else {
        return Ok(());
    };
    if archives == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "at least one log archive is required",
        ));
    }
    for number in (2..=archives).rev() {
        if let Some(previous) = bounded_tail(&archive_path(directory, number - 1), limit)? {
            atomic_log_write(&archive_path(directory, number), &previous)?;
        }
    }
    atomic_log_write(&archive_path(directory, 1), &bytes)?;
    // Pre-existing archives from an earlier policy may also be oversized.
    // Normalize sparse slots without allocating their full contents.
    for number in 2..=archives {
        let path = archive_path(directory, number);
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() > limit => {
                if let Some(tail) = bounded_tail(&path, limit)? {
                    atomic_log_write(&path, &tail)?;
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    // The current log is only cleared after its archive has been persisted.
    atomic_log_write(&current, &[])
}

fn append_bounded(
    directory: &Path,
    line: &[u8],
    limit: u64,
    archives: usize,
) -> std::io::Result<()> {
    if line.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "log record exceeds file limit",
        ));
    }
    fs::create_dir_all(directory)?;
    let current = directory.join("app.log");
    let size = match fs::metadata(&current) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error),
    };
    if size.saturating_add(line.len() as u64) > limit {
        rotate(directory, limit, archives)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(current)?;
    file.write_all(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "fishcreen-log-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn small_writer() -> LogWriter {
        LogWriter {
            limit: 128,
            archives: 3,
            ..Default::default()
        }
    }

    #[test]
    fn rotation_keeps_three_archives_newest_first_and_each_file_bounded() {
        let directory = TestDirectory::new();
        let current = directory.0.join("app.log");
        fs::write(&current, b"newest").unwrap();
        fs::write(archive_path(&directory.0, 1), b"previous").unwrap();
        fs::write(archive_path(&directory.0, 2), b"older").unwrap();
        fs::write(archive_path(&directory.0, 3), b"oldest discarded").unwrap();
        rotate(&directory.0, 128, 3).unwrap();
        assert_eq!(fs::read(&current).unwrap(), b"");
        assert_eq!(fs::read(archive_path(&directory.0, 1)).unwrap(), b"newest");
        assert_eq!(
            fs::read(archive_path(&directory.0, 2)).unwrap(),
            b"previous"
        );
        assert_eq!(fs::read(archive_path(&directory.0, 3)).unwrap(), b"older");
        let mut writer = small_writer();
        for index in 0..40 {
            writer
                .write_at(
                    &directory.0,
                    Level::Info,
                    &format!("event {index} with some content"),
                    "time",
                    Instant::now(),
                )
                .unwrap();
        }
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 4);
        for file in fs::read_dir(&directory.0).unwrap() {
            assert!(file.unwrap().metadata().unwrap().len() <= 128);
        }
    }

    #[test]
    fn exact_boundary_does_not_rotate_until_next_byte() {
        let directory = TestDirectory::new();
        append_bounded(&directory.0, b"12345678", 8, 3).unwrap();
        assert!(!archive_path(&directory.0, 1).exists());
        append_bounded(&directory.0, b"9", 8, 3).unwrap();
        assert_eq!(
            fs::read(archive_path(&directory.0, 1)).unwrap(),
            b"12345678"
        );
        assert_eq!(fs::read(directory.0.join("app.log")).unwrap(), b"9");
    }

    #[test]
    fn oversized_legacy_log_keeps_a_bounded_valid_utf8_tail() {
        let directory = TestDirectory::new();
        fs::write(directory.0.join("app.log"), "界".repeat(100)).unwrap();
        fs::write(archive_path(&directory.0, 3), "更旧".repeat(100)).unwrap();
        rotate(&directory.0, 128, 3).unwrap();
        let tail = fs::read(archive_path(&directory.0, 1)).unwrap();
        assert!(tail.len() <= 128);
        assert!(String::from_utf8(tail)
            .unwrap()
            .chars()
            .all(|character| character == '界'));
        assert!(fs::metadata(archive_path(&directory.0, 3)).unwrap().len() <= 128);
    }

    #[test]
    fn unicode_and_multiline_messages_cannot_exceed_the_record_limit() {
        let directory = TestDirectory::new();
        small_writer()
            .write_at(
                &directory.0,
                Level::Info,
                &"你好\n\r\0".repeat(4000),
                "time",
                Instant::now(),
            )
            .unwrap();
        let bytes = fs::read(directory.0.join("app.log")).unwrap();
        assert!(bytes.len() <= 128);
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(text.matches('\n').count(), 1);
        assert!(text.contains("[truncated]"));
        assert!(text.contains("\\n\\r\\0"));
    }

    #[test]
    fn archive_failure_never_truncates_current_or_appends_beyond_the_limit() {
        let directory = TestDirectory::new();
        let current = directory.0.join("app.log");
        let evidence = vec![b'x'; 128];
        fs::write(&current, &evidence).unwrap();
        fs::create_dir(archive_path(&directory.0, 1)).unwrap();
        assert!(small_writer()
            .write_at(
                &directory.0,
                Level::Error,
                "failure",
                "time",
                Instant::now()
            )
            .is_err());
        assert_eq!(fs::read(current).unwrap(), evidence);
    }

    #[test]
    fn repeated_errors_are_limited_but_info_and_recovery_events_are_not() {
        let directory = TestDirectory::new();
        let mut writer = LogWriter::default();
        let now = Instant::now();
        assert_eq!(
            writer
                .write_at(&directory.0, Level::Error, "DDC unavailable", "time", now)
                .unwrap(),
            Outcome::Written
        );
        for _ in 0..5 {
            assert_eq!(
                writer
                    .write_at(
                        &directory.0,
                        Level::Error,
                        "DDC unavailable",
                        "time",
                        now + Duration::from_secs(1)
                    )
                    .unwrap(),
                Outcome::Suppressed
            );
        }
        for _ in 0..2 {
            assert_eq!(
                writer
                    .write_at(
                        &directory.0,
                        Level::Info,
                        "brightness restored",
                        "time",
                        now + Duration::from_secs(2)
                    )
                    .unwrap(),
                Outcome::Written
            );
        }
        assert_eq!(
            writer
                .write_at(
                    &directory.0,
                    Level::Error,
                    "DDC unavailable",
                    "time",
                    now + ERROR_INTERVAL
                )
                .unwrap(),
            Outcome::Written
        );
        let text = fs::read_to_string(directory.0.join("app.log")).unwrap();
        assert_eq!(text.matches("[ERROR]").count(), 2);
        assert_eq!(text.matches("brightness restored").count(), 2);
        assert!(text.contains("[suppressed 5 repeats]"));
    }

    #[test]
    fn limiter_state_is_bounded_and_failed_writes_do_not_suppress_retries() {
        let directory = TestDirectory::new();
        let mut writer = LogWriter::default();
        let now = Instant::now();
        for index in 0..200 {
            writer
                .write_at(
                    &directory.0,
                    Level::Error,
                    &format!("error {index}"),
                    "time",
                    now,
                )
                .unwrap();
        }
        assert_eq!(writer.errors.len(), MAX_ERROR_KEYS);
        assert_eq!(
            writer
                .write_at(&directory.0, Level::Error, "error 0", "time", now)
                .unwrap(),
            Outcome::Written
        );
        let blocked = directory.0.join("not-a-directory");
        fs::write(&blocked, "keep").unwrap();
        assert!(writer
            .write_at(&blocked, Level::Error, "new failure", "time", now)
            .is_err());
        assert!(!writer
            .errors
            .iter()
            .any(|entry| entry.message == "new failure"));
        assert_eq!(
            writer
                .write_at(&directory.0, Level::Error, "new failure", "time", now)
                .unwrap(),
            Outcome::Written
        );
    }

    #[test]
    fn serialized_concurrent_writers_produce_complete_bounded_records() {
        let directory = TestDirectory::new();
        let writer = Arc::new(Mutex::new(LogWriter::default()));
        std::thread::scope(|scope| {
            for index in 0..16 {
                let writer = writer.clone();
                let path = &directory.0;
                scope.spawn(move || {
                    writer
                        .lock()
                        .unwrap()
                        .write_at(
                            path,
                            Level::Info,
                            &format!("event {index}"),
                            "time",
                            Instant::now(),
                        )
                        .unwrap();
                });
            }
        });
        let text = fs::read_to_string(directory.0.join("app.log")).unwrap();
        assert_eq!(text.lines().count(), 16);
        assert!(text
            .lines()
            .all(|line| line.starts_with("time [Rust] [INFO] event ")));
    }
}
