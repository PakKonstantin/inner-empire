//! The vault: a plain folder the user owns, plus the small amount of app state
//! that travels with it.

pub mod attachments;
pub mod fileops;
pub mod path;
pub mod settings;
pub mod trash;

pub use attachments::{target_folder as attachment_folder, target_path as attachment_path};
pub use fileops::{Collision, FileOps};
pub use path::{sanitize_segment, validate_segment, VaultPath, APP_DIR};
pub use settings::{AttachmentLocation, DailyNoteSettings, LinkStyle, VaultSettings};
pub use trash::{Trash, TrashEntry, TrashManifest};
