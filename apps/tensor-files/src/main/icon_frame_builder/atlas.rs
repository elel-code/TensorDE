impl<'a> IconFrameBuilder<'a> {

    fn allocate(&mut self, icon_width: u32, icon_height: u32) -> AtlasRect {
        if self.cursor_x + icon_width + ICON_PADDING > self.width {
            self.cursor_x = ICON_PADDING;
            self.cursor_y += self.row_height.max(1);
            self.row_height = 0;
        }

        let x = self.cursor_x;
        let y = self.cursor_y;
        self.cursor_x += icon_width + ICON_PADDING;
        self.row_height = self.row_height.max(icon_height + ICON_PADDING);
        self.ensure_height(y + icon_height + ICON_PADDING);

        AtlasRect {
            x: x as f32,
            y: y as f32,
            width: icon_width as f32,
            height: icon_height as f32,
        }
    }

    fn ensure_height(&mut self, needed_height: u32) {
        if needed_height <= self.height {
            return;
        }
        self.height = needed_height.next_power_of_two();
    }
}
