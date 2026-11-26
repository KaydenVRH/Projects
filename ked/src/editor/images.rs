//! Kitty graphics: markdown images and pasted clipboard images.

use super::*;

impl Editor {
    /// Install the terminal's graphics capability info (probed in
    /// `main.rs` at startup).
    pub fn set_graphics(&mut self, g: crate::graphics::KittyGraphics) {
        self.graphics = g;
    }

    /// Diff the desired image placements against what's currently on
    /// screen and queue transmit/place/delete escapes for `main.rs`
    /// to flush after the frame is drawn.
    pub(super) fn queue_md_images(
        &mut self,
        images: &[crate::graphics::PlacedImage],
        area: Rect,
        gutter: usize,
    ) {
        let mut desired: Vec<(u32, (u16, u16, u16, u16))> = Vec::new();
        for im in images {
            let row = im.line.saturating_sub(self.top) as u16;
            if row >= area.height {
                continue;
            }
            let cols = im.cols.min(area.width.saturating_sub(gutter as u16));
            let rows = im.rows.min(area.height.saturating_sub(row));
            if cols < 4 || rows < 1 {
                continue;
            }
            desired.push((im.key, (area.x + gutter as u16, area.y + row, cols, rows)));
        }

        let mut out: Vec<u8> = Vec::new();
        for (key, rect) in &desired {
            let need_place = match self.placed_images.get(key) {
                Some(old) if old == rect => false,
                _ => true,
            };
            if !need_place {
                continue;
            }
            if let Some(old) = self.placed_images.remove(key) {
                let _ = old;
                crate::graphics::delete_cmd(*key, &mut out);
            }
            if !self.transmitted_images.contains(key) {
                if let Some(im) = images.iter().find(|im| im.key == *key) {
                    if let Ok(bytes) = fs::read(&im.path) {
                        crate::graphics::transmit_cmd(*key, &bytes, &mut out);
                        self.transmitted_images.insert(*key);
                    }
                }
            }
            if self.transmitted_images.contains(key) {
                crate::graphics::place_cmd(
                    *key,
                    rect.0,
                    rect.1,
                    rect.2,
                    rect.3,
                    &mut out,
                );
            }
        }
        // Delete images that scrolled out of view.
        for key in self.placed_images.keys() {
            if !desired.iter().any(|(k, _)| k == key) {
                crate::graphics::delete_cmd(*key, &mut out);
            }
        }
        self.placed_images = desired.into_iter().collect();

        // Restore the editor cursor afterwards (placement moves it).
        if !out.is_empty() {
            if let Some((cx, cy)) = self.cursor_screen {
                out.extend_from_slice(format!("\x1b[{};{}H", cy + 1, cx + 1).as_bytes());
            }
        }
        self.graphics_out = out;
    }

    /// Take the queued kitty-graphics escapes (which already end with a cursor
    /// move) so the caller can append them to the frame and write the whole
    /// thing to the terminal at once. Called after each frame is drawn.
    pub fn take_graphics(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.graphics_out)
    }

    /// Remove every inline image from the terminal (called on exit).
    pub fn clear_graphics(&mut self) {
        if !self.graphics.supported {
            return;
        }
        let mut out = Vec::new();
        crate::graphics::delete_all_cmd(&mut out);
        let _ = io::stdout().write_all(&out);
        let _ = io::stdout().flush();
        self.placed_images.clear();
        self.transmitted_images.clear();
    }

    /// Paste the clipboard image into the buffer as a markdown
    /// reference (`![](img-<stamp>.png)`), saving the PNG next to the
    /// current file.
    pub fn paste_clipboard_image(&mut self) -> Result<(), String> {
        let bytes =
            crate::graphics::clipboard_image_png().ok_or("clipboard holds no image")?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let name = format!("img-{stamp}.png");
        let dir = self
            .filename
            .as_deref()
            .and_then(|f| Path::new(f).parent())
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            });
        let path = dir.join(&name);
        fs::write(&path, &bytes).map_err(|e| format!("can't write {}: {e}", path.display()))?;

        let alt = name.trim_end_matches(".png");
        let md = format!("![{alt}]({name})");
        self.save_undo();
        let line = &mut self.lines[self.cy];
        let pos = line.floor_char_boundary(self.cx);
        line.insert_str(pos, &md);
        self.cx = pos + md.len();
        self.modified = true;
        self.clamp_scroll();
        self.flash = Some(format!("Pasted image as {name}"));
        Ok(())
    }

    // ── memories / reminders / journal ─────────────────────────
}
