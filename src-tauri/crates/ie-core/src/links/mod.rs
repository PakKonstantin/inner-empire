//! Link resolution: turning what the user wrote into a file, and keeping every
//! reference correct when files move.

pub mod reference;
// pub mod rename; (next step)
// pub mod resolver; (next step)

pub use reference::{heading_matches, slugify, LinkTarget};
// re-exported once rename lands
// re-exported once resolver lands
