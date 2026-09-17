//! The core's event vocabulary.
//!
//! One enum, emitted by the core and bridged unchanged to the webview and then
//! to plugins. Keeping a single definition means a plugin sees exactly what the
//! UI sees, and adding an event is one change rather than three.

use crate::error::Diagnostic;
use crate::index::IndexProgress;
use crate::vault::path::VaultPath;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CoreEvent {
    #[serde(rename_all = "camelCase")]
    VaultOpened { root: String, name: String },
    VaultClosed,

    #[serde(rename_all = "camelCase")]
    FileCreated { path: VaultPath },
    #[serde(rename_all = "camelCase")]
    FileModified { path: VaultPath },
    #[serde(rename_all = "camelCase")]
    FileDeleted { path: VaultPath },
    #[serde(rename_all = "camelCase")]
    FileRenamed { from: VaultPath, to: VaultPath },

    #[serde(rename_all = "camelCase")]
    IndexProgress { progress: IndexProgress },
    #[serde(rename_all = "camelCase")]
    IndexCompleted {
        files: usize,
        duration_ms: u64,
        diagnostics: Vec<Diagnostic>,
    },
    #[serde(rename_all = "camelCase")]
    IndexError { message: String },

    #[serde(rename_all = "camelCase")]
    ActiveFileChanged { path: Option<VaultPath> },
    WorkspaceChanged,

    /// Something the user should know about that is not an error, such as a
    /// rename having updated forty links.
    #[serde(rename_all = "camelCase")]
    Notice { level: NoticeLevel, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl CoreEvent {
    /// The event's name, used as the channel the UI subscribes to.
    pub fn name(&self) -> &'static str {
        match self {
            CoreEvent::VaultOpened { .. } => "vaultOpened",
            CoreEvent::VaultClosed => "vaultClosed",
            CoreEvent::FileCreated { .. } => "fileCreated",
            CoreEvent::FileModified { .. } => "fileModified",
            CoreEvent::FileDeleted { .. } => "fileDeleted",
            CoreEvent::FileRenamed { .. } => "fileRenamed",
            CoreEvent::IndexProgress { .. } => "indexProgress",
            CoreEvent::IndexCompleted { .. } => "indexCompleted",
            CoreEvent::IndexError { .. } => "indexError",
            CoreEvent::ActiveFileChanged { .. } => "activeFileChanged",
            CoreEvent::WorkspaceChanged => "workspaceChanged",
            CoreEvent::Notice { .. } => "notice",
        }
    }
}

/// Where the core sends events. The application layer supplies one.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: CoreEvent);
}

/// Discards events, for tests and for headless use.
pub struct NullEventSink;

impl EventSink for NullEventSink {
    fn emit(&self, _event: CoreEvent) {}
}

/// Collects events, for asserting on them in tests.
#[derive(Default)]
pub struct RecordingEventSink {
    events: std::sync::Mutex<Vec<CoreEvent>>,
}

impl RecordingEventSink {
    pub fn events(&self) -> Vec<CoreEvent> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.events().iter().map(CoreEvent::name).collect()
    }
}

impl EventSink for RecordingEventSink {
    fn emit(&self, event: CoreEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }
}

pub type SharedEventSink = std::sync::Arc<dyn EventSink>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_serialise_with_a_discriminating_tag() {
        let json = serde_json::to_string(&CoreEvent::FileRenamed {
            from: VaultPath::parse("a.md").unwrap(),
            to: VaultPath::parse("b.md").unwrap(),
        })
        .unwrap();
        assert_eq!(json, r#"{"type":"fileRenamed","from":"a.md","to":"b.md"}"#);
    }

    #[test]
    fn a_unit_event_still_carries_its_tag() {
        assert_eq!(
            serde_json::to_string(&CoreEvent::VaultClosed).unwrap(),
            r#"{"type":"vaultClosed"}"#
        );
    }

    #[test]
    fn the_recording_sink_keeps_events_in_order() {
        let sink = RecordingEventSink::default();
        sink.emit(CoreEvent::VaultClosed);
        sink.emit(CoreEvent::WorkspaceChanged);
        assert_eq!(sink.names(), vec!["vaultClosed", "workspaceChanged"]);
    }
}
