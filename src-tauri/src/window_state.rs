//! Remembering where the window was.
//!
//! An application that opens in the middle of the screen at the same size
//! every time is one that has forgotten the user arranged their desk. This
//! saves the size, position and maximised state, and restores them on the
//! next start.
//!
//! The care goes into *not* restoring a position blindly. A window saved on a
//! second monitor that is no longer attached, or on a display whose resolution
//! changed, would come back off-screen and be unreachable. So a restored
//! position is only used when it still lands on a monitor that exists; failing
//! that, the size is kept and the window is centred.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize, Window};

/// What is written to disk. Logical units, so a window saved on a HiDPI screen
/// comes back the same apparent size on an ordinary one.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WindowState {
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    pub maximised: bool,
}

/// Anything smaller than this is a window the user cannot work in, and is more
/// likely a bad read than an intention.
const MIN_WIDTH: f64 = 480.0;
const MIN_HEIGHT: f64 = 360.0;

fn state_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let state = app.state::<crate::state::SharedState>();
    state
        .host
        .dirs
        .config_dir()
        .ok()
        .map(|dir| dir.join("window.json"))
}

/// Write the window's current geometry, ignoring failures.
///
/// A window that cannot be remembered is a small annoyance next time; it is
/// not worth interrupting a shutdown over, and the log records it.
pub fn save(window: &Window) {
    let Some(path) = state_path(&window.app_handle().clone()) else {
        return;
    };

    let Ok(scale) = window.scale_factor() else {
        return;
    };
    let maximised = window.is_maximized().unwrap_or(false);

    // A maximised window's own size is the screen's, which is not what should
    // come back when it is restored down. The previously saved size is kept
    // instead, so un-maximising lands where the user last dragged it.
    let previous = read(&path);
    let (size, position) = if maximised {
        match previous {
            Some(state) => (
                LogicalSize::new(state.width, state.height),
                LogicalPosition::new(state.x, state.y),
            ),
            None => return,
        }
    } else {
        let Ok(inner) = window.inner_size() else {
            return;
        };
        let Ok(outer) = window.outer_position() else {
            return;
        };
        (inner.to_logical(scale), outer.to_logical(scale))
    };

    let state = WindowState {
        width: size.width,
        height: size.height,
        x: position.x,
        y: position.y,
        maximised,
    };

    let written = serde_json::to_vec_pretty(&state)
        .map_err(|error| error.to_string())
        .and_then(|bytes| fs::write(&path, bytes).map_err(|error| error.to_string()));
    if let Err(error) = written {
        tracing::debug!(%error, "the window position could not be saved");
    }
}

fn read(path: &PathBuf) -> Option<WindowState> {
    let bytes = fs::read(path).ok()?;
    let state: WindowState = serde_json::from_slice(&bytes).ok()?;
    if !state.width.is_finite() || !state.height.is_finite() {
        return None;
    }
    if !state.x.is_finite() || !state.y.is_finite() {
        return None;
    }
    if state.width < MIN_WIDTH || state.height < MIN_HEIGHT {
        return None;
    }
    Some(state)
}

/// Put the window back where it was, if that is still a place on this desk.
pub fn restore(window: &Window) {
    let Some(path) = state_path(&window.app_handle().clone()) else {
        return;
    };
    let Some(state) = read(&path) else { return };
    let Ok(scale) = window.scale_factor() else {
        return;
    };

    if let Err(error) = window.set_size(LogicalSize::new(state.width, state.height)) {
        tracing::debug!(%error, "the window size could not be restored");
        return;
    }

    let position = LogicalPosition::new(state.x, state.y).to_physical::<i32>(scale);
    let size = LogicalSize::new(state.width, state.height).to_physical::<u32>(scale);

    if is_on_a_monitor(window, position, size) {
        if let Err(error) = window.set_position(position) {
            tracing::debug!(%error, "the window position could not be restored");
        }
    } else {
        // The monitor it was on is gone. Keep the size the user chose and put
        // the window somewhere they can actually reach it.
        tracing::info!("the saved window position is off-screen; centring instead");
        let _ = window.center();
    }

    if state.maximised {
        let _ = window.maximize();
    }
}

/// Whether enough of the window would land on an attached monitor to grab.
///
/// "Enough" is deliberately loose — a window hanging half off the right edge
/// is a window the user put there. What this rules out is one that is
/// entirely on a monitor that no longer exists, or pushed so far up that its
/// title bar is above every screen.
fn is_on_a_monitor(
    window: &Window,
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) -> bool {
    let Ok(monitors) = window.available_monitors() else {
        return false;
    };

    // A strip along the top of the window wide enough to drag by.
    let grab_height: i32 = 32;
    let left = position.x;
    let right = position.x + i32::try_from(size.width).unwrap_or(i32::MAX);
    let top = position.y;
    let bottom = position.y + grab_height;

    monitors.iter().any(|monitor| {
        let origin = monitor.position();
        let extent = monitor.size();
        let m_left = origin.x;
        let m_right = origin.x + i32::try_from(extent.width).unwrap_or(i32::MAX);
        let m_top = origin.y;
        let m_bottom = origin.y + i32::try_from(extent.height).unwrap_or(i32::MAX);

        let overlap_x = left.max(m_left) < right.min(m_right);
        let overlap_y = top.max(m_top) < bottom.min(m_bottom);
        overlap_x && overlap_y
    })
}
