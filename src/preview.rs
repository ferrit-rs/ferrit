//! Right-pane image preview.
//!
//! This is the only module that touches `image` or `ratatui-image`; `src/git/`
//! stays clean of both and just hands over bytes. When the terminal speaks a
//! graphics protocol (sixel / kitty / iterm2) the picture renders natively;
//! otherwise `ratatui-image` falls back to unicode half-blocks, which work
//! anywhere, so there is always something to show.

use std::path::Path;

use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

/// What the right pane should show for the current selection.
#[derive(Default)]
pub enum Preview {
    /// Not an image selection: the right pane keeps its normal content.
    #[default]
    None,
    /// An image path that could not be turned into a picture.
    Note(String),
    /// A decoded image, held exactly as the ratatui-image examples do: a live
    /// `StatefulProtocol` that `StatefulImage` resizes and re-encodes in place
    /// at render time, so `ui::draw` takes `&mut App`. Boxed only because a
    /// `StatefulProtocol` dwarfs the other variants.
    Image(Box<StatefulProtocol>),
}

const IMAGE_EXTS: [&str; 7] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico"];

/// Does this path look like an image we would try to preview?
pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|e| IMAGE_EXTS.contains(&e.as_str()))
}

/// Decode `bytes` into a `Preview`. Never panics; anything that fails to decode
/// comes back as `Preview::Note`.
pub fn from_bytes(picker: &Picker, path: &Path, bytes: &[u8]) -> Preview {
    if bytes.is_empty() {
        return Preview::Note(format!("[image] {}  (no bytes)", path.display()));
    }
    match image::load_from_memory(bytes) {
        // `new_resize_protocol` just stores the source; the first render does
        // the resize + encode, same as `examples/thread.rs`. Nothing to check
        // here yet, so hand back the live protocol.
        Ok(img) => Preview::Image(Box::new(picker.new_resize_protocol(img))),
        Err(e) => Preview::Note(format!(
            "[image] {}  ({} bytes)  decode failed: {e}",
            path.display(),
            bytes.len()
        )),
    }
}
