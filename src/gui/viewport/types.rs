//! Viewport state and input event types for CEF frame buffer rendering.
//!
//! Contains the `ViewportState` struct that tracks the current texture and
//! frame version for efficient delta updates, and the `ViewportInput` enum
//! representing mouse/keyboard events forwarded to CEF.

use egui::{ColorImage, TextureHandle, TextureOptions};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Global frame version counter, incremented by CEF's on_paint callback.
/// The viewport compares against its own last-seen version to skip redundant
/// texture uploads.
static FRAME_VERSION: AtomicU64 = AtomicU64::new(0);

/// Call this from the on_paint callback after writing new frame data.
pub fn bump_frame_version() {
    FRAME_VERSION.fetch_add(1, Ordering::Release);
}

/// Returns the current global frame version (acquire ordering).
pub(crate) fn current_frame_version() -> u64 {
    FRAME_VERSION.load(Ordering::Acquire)
}

/// Holds the current frame texture from CEF.
pub struct ViewportState {
    pub texture: Option<TextureHandle>,
    pub(crate) last_mouse_pos: Option<(i32, i32)>,
    /// Last frame version we uploaded as a texture.
    last_frame_version: u64,
    /// ID of the tab whose frame buffer we last rendered. When the active tab
    /// changes we must force a texture re-upload even if FRAME_VERSION hasn't
    /// changed, because we're now pointing at a different buffer.
    last_tab_id: Option<uuid::Uuid>,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewportState {
    pub fn new() -> Self {
        Self {
            texture: None,
            last_mouse_pos: None,
            last_frame_version: 0,
            last_tab_id: None,
        }
    }

    /// Update the texture from CEF's BGRA frame buffer.
    /// Only re-converts if the frame buffer has actually changed (version check)
    /// or if the active tab has changed (different buffer entirely).
    /// Releases the read lock ASAP by cloning the buffer first.
    pub fn update_from_frame_buffer(
        &mut self,
        ctx: &egui::Context,
        frame_buffer: &Arc<RwLock<Vec<u8>>>,
        frame_size: &Arc<RwLock<(u32, u32)>>,
        tab_id: uuid::Uuid,
    ) {
        let tab_changed = self.last_tab_id != Some(tab_id);
        let current_version = current_frame_version();
        if !tab_changed && current_version == self.last_frame_version && self.texture.is_some() {
            // Same tab, same frame version — skip.
            return;
        }
        if tab_changed {
            self.last_tab_id = Some(tab_id);
        }

        // Convert BGRA → RGBA directly while holding the read lock, avoiding
        // a full fb.clone() (~8 MB at 1920x1080). Pure byte-shuffling is O(n)
        // and faster than clone + separate convert because it is a single pass.
        // The CEF on_paint callback only writes when it holds the write lock,
        // so holding the read lock here does not block painting for long.
        let (rgba, w, h) = {
            let fb = frame_buffer.read();
            let (w, h) = *frame_size.read();

            if fb.is_empty() || w == 0 || h == 0 {
                return;
            }

            let expected = (w as usize) * (h as usize) * 4;
            let len = fb.len().min(expected);
            // Single memcpy + in-place B/R swap (faster than per-byte push loop)
            let mut rgba = fb[..len].to_vec();
            for chunk in rgba.chunks_exact_mut(4) {
                chunk.swap(0, 2); // Swap B↔R: BGRA → RGBA
            }
            (rgba, w, h)
        };
        // Read lock released here — single allocation, no intermediate clone.

        let image = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);

        if let Some(tex) = &mut self.texture {
            // Reuse existing GPU texture — avoids allocation + deallocation per frame
            tex.set(image, TextureOptions::LINEAR);
        } else {
            // First frame — allocate new texture
            self.texture = Some(ctx.load_texture("cef_page", image, TextureOptions::LINEAR));
        }
        self.last_frame_version = current_version;
    }
}

/// Input event to forward to CEF.
pub enum ViewportInput {
    MouseMove { x: i32, y: i32 },
    MouseClick { x: i32, y: i32, button: i32 },
    MouseWheel { x: i32, y: i32, delta_x: i32, delta_y: i32 },
    KeyDown { key_code: i32, character: u16 },
    KeyUp { key_code: i32, character: u16 },
    CharInput { character: u16 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_state_new_defaults() {
        let state = ViewportState::new();
        assert!(state.texture.is_none());
        assert_eq!(state.last_frame_version, 0);
        assert!(state.last_tab_id.is_none());
    }

    #[test]
    fn test_bump_frame_version_increments() {
        let before = FRAME_VERSION.load(Ordering::Acquire);
        bump_frame_version();
        let after = FRAME_VERSION.load(Ordering::Acquire);
        assert_eq!(after, before + 1);
    }
}
