//! Right-pane image preview.
//!
//! This is the only module that touches `image` or `ratatui-image`; `src/git/`
//! stays clean of both and just hands over bytes. When the terminal speaks a
//! graphics protocol (sixel / kitty / iterm2) the picture renders natively;
//! otherwise `ratatui-image` falls back to unicode half-blocks, which work
//! anywhere, so there is always something to show.

use std::cell::RefCell;
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
    /// A decoded image ready for `StatefulImage`. `RefCell` because rendering
    /// resizes and encodes in place, and `ui::draw` only holds `&App`; boxed
    /// because a `StatefulProtocol` dwarfs the other variants.
    Image(Box<RefCell<StatefulProtocol>>),
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
        Ok(img) => {
            let mut proto = picker.new_resize_protocol(img);
            match proto.last_encoding_result() {
                Some(Err(e)) => Preview::Note(format!(
                    "[image] {}  ({} bytes)  encode failed: {e}",
                    path.display(),
                    bytes.len()
                )),
                _ => Preview::Image(Box::new(RefCell::new(proto))),
            }
        }
        Err(e) => Preview::Note(format!(
            "[image] {}  ({} bytes)  decode failed: {e}",
            path.display(),
            bytes.len()
        )),
    }
}
