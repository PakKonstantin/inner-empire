//! Structured logging.
//!
//! Logs go to a rotating file in the platform's log directory and never to the
//! user interface: an app that prints its diagnostics into the window the user
//! is writing in is not one they will keep. Problems the user needs to know
//! about travel as `CoreEvent::Notice` or as a typed error instead.

use std::path::Path;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_filter(self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

/// Keep this many rotated files. Enough to cover a few sessions, few enough
/// that a long-running install does not fill a disk.
pub const MAX_LOG_FILES: usize = 5;
/// Rotate once a file passes this size.
pub const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

/// Install the global subscriber. Safe to call more than once; later calls are
/// ignored rather than panicking, which matters because tests and the app both
/// call it.
pub fn init(log_dir: &Path, level: LogLevel) -> std::io::Result<()> {
    std::fs::create_dir_all(log_dir)?;
    rotate(log_dir)?;

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("inner-empire.log"))?;

    let filter =
        EnvFilter::try_from_env("IE_LOG").unwrap_or_else(|_| EnvFilter::new(level.as_filter()));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file)
                .with_ansi(false)
                .with_target(true)
                .with_thread_names(true),
        )
        .try_init();

    Ok(())
}

/// Roll `inner-empire.log` to `.1`, `.1` to `.2`, and so on, dropping the
/// oldest. Done at startup rather than continuously, which keeps the writer a
/// plain appending file handle with no locking.
fn rotate(log_dir: &Path) -> std::io::Result<()> {
    let current = log_dir.join("inner-empire.log");
    let too_big = std::fs::metadata(&current)
        .map(|m| m.len() > MAX_LOG_BYTES)
        .unwrap_or(false);
    if !too_big {
        return Ok(());
    }

    let oldest = log_dir.join(format!("inner-empire.log.{MAX_LOG_FILES}"));
    if oldest.exists() {
        std::fs::remove_file(&oldest)?;
    }
    for index in (1..MAX_LOG_FILES).rev() {
        let from = log_dir.join(format!("inner-empire.log.{index}"));
        if from.exists() {
            std::fs::rename(
                &from,
                log_dir.join(format!("inner-empire.log.{}", index + 1)),
            )?;
        }
    }
    std::fs::rename(&current, log_dir.join("inner-empire.log.1"))?;
    Ok(())
}

/// Read the tail of the current log, for the "copy diagnostics" button in
/// settings.
pub fn tail(log_dir: &Path, lines: usize) -> std::io::Result<String> {
    let text = std::fs::read_to_string(log_dir.join("inner-empire.log"))?;
    let collected: Vec<&str> = text.lines().rev().take(lines).collect();
    Ok(collected.into_iter().rev().collect::<Vec<_>>().join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_small_log_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inner-empire.log");
        std::fs::write(&path, b"short").unwrap();
        rotate(dir.path()).unwrap();
        assert!(path.exists());
        assert!(!dir.path().join("inner-empire.log.1").exists());
    }

    #[test]
    fn an_oversized_log_rolls_to_a_numbered_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inner-empire.log");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&vec![b'x'; (MAX_LOG_BYTES + 1) as usize])
            .unwrap();
        drop(file);

        rotate(dir.path()).unwrap();

        assert!(!path.exists(), "the current log was moved aside");
        assert!(dir.path().join("inner-empire.log.1").exists());
    }

    #[test]
    fn rotation_discards_the_oldest_file_rather_than_growing_forever() {
        let dir = tempfile::tempdir().unwrap();
        for index in 1..=MAX_LOG_FILES {
            std::fs::write(
                dir.path().join(format!("inner-empire.log.{index}")),
                format!("generation {index}"),
            )
            .unwrap();
        }
        std::fs::write(
            dir.path().join("inner-empire.log"),
            vec![b'x'; (MAX_LOG_BYTES + 1) as usize],
        )
        .unwrap();

        rotate(dir.path()).unwrap();

        let count = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(count, MAX_LOG_FILES, "one current plus the rotated ones");
        let oldest =
            std::fs::read_to_string(dir.path().join(format!("inner-empire.log.{MAX_LOG_FILES}")))
                .unwrap();
        assert_eq!(oldest, format!("generation {}", MAX_LOG_FILES - 1));
    }

    #[test]
    fn the_tail_returns_the_last_lines_in_order() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("inner-empire.log"),
            "one\ntwo\nthree\nfour\n",
        )
        .unwrap();
        assert_eq!(tail(dir.path(), 2).unwrap(), "three\nfour");
    }
}
