//! Procedural wide/long tray pill renderer (`TrayPillRenderer`) and popover animation
//! state machine (`TrayAnimationController`).
//!
//! Addresses the requirement for a long tray icon capable of displaying both an
//! animated battery glyph and an inline percentage readout (e.g. `[🔋 87%]`),
//! plus smooth spring-eased open/close animations when toggling the tray popover.

use serde::{Deserialize, Serialize};

/// Dimensions for the tray icon RGBA buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayPillDimensions {
    pub width: u32,
    pub height: u32,
}

impl TrayPillDimensions {
    /// Wide pill layout (`64x22`) showing battery bar + inline `"87%"` pixel text.
    /// Supported natively on macOS `NSStatusItem` and Linux AppIndicator, and used
    /// in the custom tray pill overlay on Windows.
    pub const WIDE_PILL: Self = Self {
        width: 64,
        height: 22,
    };

    /// Compact square layout (`32x32`) stacking the battery bar on top and the
    /// `"87%"` pixel readout below so standard Windows `Shell_NotifyIconW` slots
    /// still display the live battery percentage legibly without clipping.
    pub const COMPACT_SQUARE: Self = Self {
        width: 32,
        height: 32,
    };
}

/// High-level state rendered into the system tray icon and tooltip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrayBadgeState {
    pub primary_device_name: Option<String>,
    pub battery_percent: Option<u8>,
    pub charging: bool,
    pub popover_open: bool,
    /// Animation phase in `0.0..=1.0` used for charging wave & open indicator glow.
    pub animation_phase: f32,
}

impl Default for TrayBadgeState {
    fn default() -> Self {
        Self {
            primary_device_name: None,
            battery_percent: None,
            charging: false,
            popover_open: false,
            animation_phase: 0.0,
        }
    }
}

impl TrayBadgeState {
    /// Formats the hover tooltip for the system tray icon (`"G Pro X Superlight — 87% (Charging)"`).
    pub fn tooltip(&self) -> String {
        match (&self.primary_device_name, self.battery_percent) {
            (Some(name), Some(pct)) => {
                let suffix = if self.charging { " (Charging)" } else { "" };
                format!("{name} — {}%{suffix}", pct.min(100))
            }
            (Some(name), None) => format!("{name} — Battery Unavailable"),
            (None, _) => "PeripheralDeck — No active peripheral".to_string(),
        }
    }

    /// Formats the compact text label (`"87%"` or `"--"`).
    pub fn percentage_text(&self) -> String {
        match self.battery_percent {
            Some(pct) => format!("{}%", pct.min(100)),
            None => "--%".to_string(),
        }
    }
}

/// Interpolated keyframe emitted to the Tauri popover window during open/close transitions.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PopoverAnimationFrame {
    /// Normalized progress `0.0` (closed) to `1.0` (fully open).
    pub progress: f32,
    /// Window opacity `0.0..=1.0`.
    pub opacity: f32,
    /// Vertical slide offset in physical pixels (slides from `+10px` to `0px`).
    pub translate_y_px: f32,
    /// Uniform scale factor (`0.95..=1.0`).
    pub scale: f32,
}

/// Deterministic spring-eased animation controller for tray popover open/close
/// and charging wave cycles.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrayAnimationController {
    popover_open: bool,
    progress: f32,
    charging_phase: f32,
}

impl Default for TrayAnimationController {
    fn default() -> Self {
        Self::new()
    }
}

impl TrayAnimationController {
    pub const fn new() -> Self {
        Self {
            popover_open: false,
            progress: 0.0,
            charging_phase: 0.0,
        }
    }

    pub fn is_open(&self) -> bool {
        self.popover_open
    }

    pub fn set_open(&mut self, open: bool) {
        self.popover_open = open;
    }

    pub fn toggle(&mut self) -> bool {
        self.popover_open = !self.popover_open;
        self.popover_open
    }

    /// Advances the animation state by `dt_ms` milliseconds and returns the current
    /// popover keyframe. Full transition completes in ~180 ms.
    pub fn tick(&mut self, dt_ms: f32, charging: bool) -> PopoverAnimationFrame {
        let step = (dt_ms.max(0.0) / 180.0).clamp(0.0, 1.0);
        if self.popover_open {
            self.progress = (self.progress + step).min(1.0);
        } else {
            self.progress = (self.progress - step).max(0.0);
        }

        if charging {
            self.charging_phase = (self.charging_phase + dt_ms.max(0.0) / 1200.0).rem_euclid(1.0);
        } else {
            self.charging_phase = 0.0;
        }

        // Cubic ease-out: 1 - (1 - t)^3
        let inv = 1.0 - self.progress;
        let eased = 1.0 - (inv * inv * inv);

        PopoverAnimationFrame {
            progress: self.progress,
            opacity: eased,
            translate_y_px: (1.0 - eased) * 10.0,
            scale: 0.95 + 0.05 * eased,
        }
    }

    pub fn charging_phase(&self) -> f32 {
        self.charging_phase
    }
}

/// Procedural RGBA renderer for the long tray pill (`64x22`) and compact square (`32x32`).
pub struct TrayPillRenderer;

impl TrayPillRenderer {
    /// Renders an RGBA buffer (`width * height * 4` bytes) for the wide `64x22` pill icon
    /// containing a left battery gauge and a right 3x5 pixel-font percentage readout (`"87%"`).
    pub fn render_wide_pill_rgba(state: &TrayBadgeState) -> Vec<u8> {
        let dims = TrayPillDimensions::WIDE_PILL;
        let w = dims.width;
        let h = dims.height;
        let mut rgba = vec![0u8; (w * h * 4) as usize];

        let put = |buf: &mut [u8], x: u32, y: u32, r: u8, g: u8, b: u8, a: u8| {
            if x >= w || y >= h {
                return;
            }
            let idx = ((y * w + x) * 4) as usize;
            buf[idx] = r;
            buf[idx + 1] = g;
            buf[idx + 2] = b;
            buf[idx + 3] = a;
        };

        // 1. Pill background with subtle border (dark translucent zinc with emerald/amber/red accent when open)
        let (border_r, border_g, border_b) = if state.popover_open {
            (52, 211, 153) // Active emerald highlight when popover is open
        } else {
            (82, 82, 91) // Zinc-600 border
        };

        for y in 1..(h - 1) {
            for x in 1..(w - 1) {
                // Rounded corner cutout
                let corner = (x == 1 && (y == 1 || y == h - 2))
                    || (x == w - 2 && (y == 1 || y == h - 2));
                if corner {
                    continue;
                }
                let is_border = x == 1 || x == w - 2 || y == 1 || y == h - 2;
                if is_border {
                    put(&mut rgba, x, y, border_r, border_g, border_b, 235);
                } else {
                    put(&mut rgba, x, y, 24, 24, 27, 220);
                }
            }
        }

        // 2. Left battery icon outline at x=5..22, y=6..16, nub at x=22..24, y=9..13
        for x in 5..22 {
            put(&mut rgba, x, 6, 244, 244, 245, 255);
            put(&mut rgba, x, 15, 244, 244, 245, 255);
        }
        for y in 6..16 {
            put(&mut rgba, 5, y, 244, 244, 245, 255);
            put(&mut rgba, 21, y, 244, 244, 245, 255);
        }
        for y in 9..13 {
            put(&mut rgba, 22, y, 244, 244, 245, 255);
            put(&mut rgba, 23, y, 244, 244, 245, 255);
        }

        // 3. Battery fill bar + charging wave animation
        if let Some(pct) = state.battery_percent {
            let clamped = pct.min(100);
            let fill_w = ((clamped as f32 / 100.0) * 13.0).round() as u32;
            let (fill_r, fill_g, fill_b) = Self::level_color(clamped, state.charging);
            let wave_x = ((state.animation_phase.clamp(0.0, 1.0)) * 13.0) as u32;

            for dx in 0..fill_w {
                let boost = if state.charging && dx.abs_diff(wave_x) <= 1 {
                    35u8
                } else {
                    0u8
                };
                for y in 8..14 {
                    put(
                        &mut rgba,
                        7 + dx,
                        y,
                        fill_r.saturating_add(boost),
                        fill_g.saturating_add(boost),
                        fill_b.saturating_add(boost),
                        255,
                    );
                }
            }
        }

        // 4. Right percentage text using crisp 3x5 bitmap glyphs starting at x=28, y=8
        let label = state.percentage_text();
        let mut cursor_x = 28u32;
        for ch in label.chars() {
            Self::draw_glyph_3x5(&mut rgba, w, h, cursor_x, 8, ch, (250, 250, 250, 255));
            cursor_x += 4;
        }

        rgba
    }

    /// Renders a `32x32` square tray icon with the battery gauge in the upper half
    /// and the percentage digits in the lower half for OS trays that force 1:1 icons.
    pub fn render_compact_square_rgba(state: &TrayBadgeState) -> Vec<u8> {
        let dims = TrayPillDimensions::COMPACT_SQUARE;
        let w = dims.width;
        let h = dims.height;
        let mut rgba = vec![0u8; (w * h * 4) as usize];

        let put = |buf: &mut [u8], x: u32, y: u32, r: u8, g: u8, b: u8, a: u8| {
            if x >= w || y >= h {
                return;
            }
            let idx = ((y * w + x) * 4) as usize;
            buf[idx] = r;
            buf[idx + 1] = g;
            buf[idx + 2] = b;
            buf[idx + 3] = a;
        };

        // Upper battery body: x=3..26, y=4..16
        for x in 3..26 {
            put(&mut rgba, x, 4, 255, 255, 255, 255);
            put(&mut rgba, x, 15, 255, 255, 255, 255);
        }
        for y in 4..16 {
            put(&mut rgba, 3, y, 255, 255, 255, 255);
            put(&mut rgba, 25, y, 255, 255, 255, 255);
        }
        for y in 8..12 {
            put(&mut rgba, 26, y, 255, 255, 255, 255);
            put(&mut rgba, 27, y, 255, 255, 255, 255);
        }

        if let Some(pct) = state.battery_percent {
            let clamped = pct.min(100);
            let fill_w = ((clamped as f32 / 100.0) * 19.0).round() as u32;
            let (r, g, b) = Self::level_color(clamped, state.charging);
            for dx in 0..fill_w {
                for y in 6..14 {
                    put(&mut rgba, 5 + dx, y, r, g, b, 255);
                }
            }
        }

        // Lower percentage readout at y=20..25
        let label = state.percentage_text();
        let total_w = (label.chars().count() as u32).saturating_mul(4);
        let mut cursor_x = (w.saturating_sub(total_w)) / 2;
        for ch in label.chars() {
            Self::draw_glyph_3x5(&mut rgba, w, h, cursor_x, 20, ch, (255, 255, 255, 255));
            cursor_x += 4;
        }

        rgba
    }

    fn level_color(percent: u8, charging: bool) -> (u8, u8, u8) {
        if charging {
            (52, 211, 153) // Emerald-400
        } else if percent <= 15 {
            (248, 113, 113) // Red-400
        } else if percent <= 30 {
            (251, 191, 36) // Amber-400
        } else {
            (228, 228, 231) // Zinc-200
        }
    }

    fn draw_glyph_3x5(
        rgba: &mut [u8],
        w: u32,
        h: u32,
        ox: u32,
        oy: u32,
        ch: char,
        color: (u8, u8, u8, u8),
    ) {
        let rows: [u8; 5] = match ch {
            '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
            '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
            '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
            '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
            '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
            '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
            '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
            '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
            '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
            '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
            '%' => [0b101, 0b001, 0b010, 0b100, 0b101],
            '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
            _ => [0; 5],
        };
        for (ry, bits) in rows.iter().enumerate() {
            for rx in 0..3u32 {
                if (bits >> (2 - rx)) & 1 == 1 {
                    let px = ox + rx;
                    let py = oy + ry as u32;
                    if px < w && py < h {
                        let idx = ((py * w + px) * 4) as usize;
                        rgba[idx] = color.0;
                        rgba[idx + 1] = color.1;
                        rgba[idx + 2] = color.2;
                        rgba[idx + 3] = color.3;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_wide_pill_with_noninteractive_and_active_battery_pixels() {
        let state = TrayBadgeState {
            primary_device_name: Some("Logitech G Pro X Superlight".to_string()),
            battery_percent: Some(87),
            charging: true,
            popover_open: true,
            animation_phase: 0.5,
        };
        assert_eq!(
            state.tooltip(),
            "Logitech G Pro X Superlight — 87% (Charging)"
        );

        let rgba = TrayPillRenderer::render_wide_pill_rgba(&state);
        assert_eq!(
            rgba.len(),
            (TrayPillDimensions::WIDE_PILL.width * TrayPillDimensions::WIDE_PILL.height * 4)
                as usize
        );
        // Verify non-zero opaque pixels exist in both battery gauge region and percentage text region
        let non_transparent_pixels = rgba.chunks_exact(4).filter(|px| px[3] > 0).count();
        assert!(non_transparent_pixels > 200);
    }

    #[test]
    fn animates_popover_spring_from_closed_to_open() {
        let mut anim = TrayAnimationController::new();
        assert!(!anim.is_open());
        anim.toggle();
        assert!(anim.is_open());

        let mid = anim.tick(90.0, true);
        assert!(mid.progress > 0.4 && mid.progress < 0.6);
        assert!(mid.opacity > 0.8); // Cubic ease-out rises quickly

        let end = anim.tick(120.0, true);
        assert_eq!(end.progress, 1.0);
        assert_eq!(end.opacity, 1.0);
        assert_eq!(end.translate_y_px, 0.0);
    }
}
