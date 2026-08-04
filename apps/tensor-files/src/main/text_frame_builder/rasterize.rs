impl<'a> TextFrameBuilder<'a> {
    fn rasterize_label(
        &mut self,
        label: &str,
        label_width: u32,
        label_height: u32,
        alignment: LabelAlignment,
        wrap: LabelWrap,
    ) -> Vec<u8> {
        let mut pixels = vec![0; (label_width * label_height) as usize];
        let attrs = Attrs::new().family(Family::SansSerif);
        let metrics =
            text_metrics_for_label_height(label_height, self.max_font_size, self.max_line_height);
        self.text_buffer.set_metrics(metrics);
        self.text_buffer.set_wrap(wrap.cosmic_wrap());
        self.text_buffer
            .set_size(Some(label_width as f32), Some(label_height as f32));
        self.text_buffer.set_text(
            label,
            &attrs,
            shaping_for_label(label, wrap),
            Some(alignment.cosmic_align()),
        );
        self.text_buffer.draw(
            self.font_system,
            self.swash_cache,
            TextColor::rgba(255, 255, 255, 255),
            |x, y, w, h, glyph_color| {
                fill_text_alpha_pixels(
                    &mut pixels,
                    label_width,
                    label_height,
                    TextAlphaRect {
                        x,
                        y,
                        width: w,
                        height: h,
                    },
                    glyph_color,
                );
            },
        );
        pixels
    }
}
