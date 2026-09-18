/// Time, injected rather than read from the process, so tests over daily
/// notes, trash retention and template variables are deterministic.
pub trait Clock: Send + Sync {
    /// Milliseconds since the Unix epoch, UTC.
    fn now_ms(&self) -> i64;
    /// The local UTC offset in whole seconds, east of Greenwich.
    fn local_offset_seconds(&self) -> i32;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_millis()).ok())
            .unwrap_or(0)
    }

    fn local_offset_seconds(&self) -> i32 {
        // `time` refuses to read the local offset from a multi-threaded process
        // on Unix because the C library call is not thread-safe. Falling back to
        // UTC is correct-but-shifted rather than wrong, and the setting is
        // overridable by the user.
        time::UtcOffset::current_local_offset()
            .map(|o| o.whole_seconds())
            .unwrap_or(0)
    }
}

/// A real clock whose UTC offset is supplied by the host rather than read from
/// the C library.
///
/// `SystemClock` cannot read the local offset on Unix from a multi-threaded
/// process — `time` refuses, because `localtime_r` is not thread-safe against a
/// concurrent `setenv` — and silently falls back to UTC. A host that already
/// knows the offset (iOS reads `TimeZone.current`, and is told when it changes)
/// should pass it in instead of accepting that fallback.
#[derive(Debug)]
pub struct HostOffsetClock {
    offset_seconds: std::sync::atomic::AtomicI32,
}

impl HostOffsetClock {
    pub fn new(offset_seconds: i32) -> Self {
        Self {
            offset_seconds: std::sync::atomic::AtomicI32::new(offset_seconds),
        }
    }

    /// Called when the host observes a time-zone change; the daily note's idea
    /// of "today" must follow the device, not the device at launch.
    pub fn set_offset_seconds(&self, offset_seconds: i32) {
        self.offset_seconds
            .store(offset_seconds, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Clock for HostOffsetClock {
    fn now_ms(&self) -> i64 {
        SystemClock.now_ms()
    }

    fn local_offset_seconds(&self) -> i32 {
        self.offset_seconds
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// A clock frozen at a fixed instant, for tests.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    pub now_ms: i64,
    pub offset_seconds: i32,
}

impl FixedClock {
    pub fn new(now_ms: i64) -> Self {
        Self {
            now_ms,
            offset_seconds: 0,
        }
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.now_ms
    }
    fn local_offset_seconds(&self) -> i32 {
        self.offset_seconds
    }
}

pub type SharedClock = std::sync::Arc<dyn Clock>;
