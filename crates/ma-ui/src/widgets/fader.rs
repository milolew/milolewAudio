//! Vertical fader widget for volume control.

/// Response from a fader interaction.
pub struct FaderResponse {
    pub value: f32,
    pub changed: bool,
    pub inner: egui::Response,
}

/// A vertical fader widget (0.0 to 1.0 range).
pub struct Fader {
    value: f32,
    width: f32,
    height: f32,
    label: Option<String>,
}

impl Fader {
    pub fn new(value: f32) -> Self {
        Self {
            value,
            width: 30.0,
            height: 150.0,
            label: None,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> FaderResponse {
        let desired_size = egui::vec2(self.width, self.height);
        let (rect, response) =
            ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());

        let mut new_value = self.value;
        let mut changed = false;

        if response.dragged() {
            let delta_normalized = -response.drag_delta().y / rect.height();
            new_value = (self.value + delta_normalized).clamp(0.0, 1.0);
            changed = true;
        }

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);

            // Track background
            let track_rect = egui::Rect::from_center_size(
                rect.center(),
                egui::vec2(6.0, rect.height() - 20.0),
            );
            painter.rect_filled(
                track_rect,
                2.0,
                egui::Color32::from_rgb(40, 40, 40),
            );

            // Filled portion
            let fill_height = track_rect.height() * new_value;
            let fill_rect = egui::Rect::from_min_max(
                egui::pos2(track_rect.left(), track_rect.bottom() - fill_height),
                track_rect.max,
            );
            painter.rect_filled(
                fill_rect,
                2.0,
                egui::Color32::from_rgb(80, 160, 255),
            );

            // Thumb
            let thumb_y = track_rect.bottom() - track_rect.height() * new_value;
            let thumb_rect = egui::Rect::from_center_size(
                egui::pos2(rect.center().x, thumb_y),
                egui::vec2(self.width - 4.0, 10.0),
            );
            let thumb_color = if response.hovered() || response.dragged() {
                egui::Color32::from_rgb(200, 200, 200)
            } else {
                egui::Color32::from_rgb(160, 160, 160)
            };
            painter.rect_filled(thumb_rect, 3.0, thumb_color);

            // Value label
            let db = if new_value > 0.0 {
                20.0 * new_value.log10()
            } else {
                -60.0
            };
            let db_text = if db <= -60.0 {
                "-inf".to_string()
            } else {
                format!("{db:.1}")
            };
            painter.text(
                egui::pos2(rect.center().x, rect.bottom() - 8.0),
                egui::Align2::CENTER_BOTTOM,
                db_text,
                egui::FontId::proportional(9.0),
                egui::Color32::GRAY,
            );

            // Optional label at top
            if let Some(label) = &self.label {
                painter.text(
                    egui::pos2(rect.center().x, rect.top() + 2.0),
                    egui::Align2::CENTER_TOP,
                    label,
                    egui::FontId::proportional(10.0),
                    egui::Color32::LIGHT_GRAY,
                );
            }
        }

        FaderResponse {
            value: new_value,
            changed,
            inner: response,
        }
    }
}
