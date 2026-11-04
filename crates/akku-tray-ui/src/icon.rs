//! Procedurally drawn 32x32 icons for the tray and the window.
//!
//! Pure white-on-transparent glyphs: battery (with level), keyboard, mouse,
//! and a generic device. Used for per-device tray tooltips and the window.
//!
//! The wide/long tray pill (64x22 and 32x32 compact) is now rendered in
//! `akku_core::tray_pill::TrayPillRenderer`.

#![allow(dead_code, unreachable_patterns)]

pub const SIZE: u32 = 32;

/// White-on-transparent battery glyph. `None` draws an empty battery.
pub fn battery_rgba(percent: Option<u8>, charging: bool) -> Vec<u8> {
    const W: u32 = SIZE;
    const H: u32 = SIZE;
    let mut rgba = vec![0u8; (W * H * 4) as usize];

    let mut put = |x: u32, y: u32, intensity: u8| {
        if x >= W || y >= H {
            return;
        }
        let index = ((y * W + x) * 4) as usize;
        rgba[index] = intensity;
        rgba[index + 1] = intensity;
        rgba[index + 2] = intensity;
        rgba[index + 3] = 255;
    };

    // Body outline: rectangle from (3,10) to (24,25), 2px thick, rounded ends.
    for x in 3..25 {
        put(x, 10, 255);
        put(x, 11, 255);
        put(x, 24, 255);
        put(x, 23, 255);
    }
    for y in 10..24 {
        put(3, y, 255);
        put(4, y, 255);
        put(24, y, 255);
        put(23, y, 255);
    }
    // Terminal nub: (25,15)..(28,20), 2px thick.
    for y in 15..20 {
        put(25, y, 255);
        put(26, y, 255);
    }

    // Level fill from the inside-left corner, leaving a 2px frame.
    if let Some(percent) = percent {
        let level = percent.min(100) as f32 / 100.0;
        let inner_width = (19.0 * level) as u32;
        for x in 5..5 + inner_width {
            for y in 12..23 {
                put(x, y, 255);
            }
        }
    }

    // Charge marker: a small plus in the middle of the body.
    if charging {
        for x in 12..17 {
            put(x, 17, 255);
        }
        for y in 15..20 {
            put(14, y, 255);
        }
    }

    rgba
}

/// White-on-transparent keyboard glyph (minimal, 32x32).
pub fn keyboard_rgba() -> Vec<u8> {
    const W: u32 = SIZE;
    const H: u32 = SIZE;
    let mut rgba = vec![0u8; (W * H * 4) as usize];

    let mut put = |x: u32, y: u32, intensity: u8| {
        if x >= W || y >= H {
            return;
        }
        let index = ((y * W + x) * 4) as usize;
        rgba[index] = intensity;
        rgba[index + 1] = intensity;
        rgba[index + 2] = intensity;
        rgba[index + 3] = 255;
    };

    // Keyboard body: rounded rect from (4, 16) to (28, 26)
    for x in 4..29 {
        put(x, 16, 255);
        put(x, 17, 255);
        put(x, 25, 255);
        put(x, 26, 255);
    }
    for y in 16..27 {
        put(4, y, 255);
        put(5, y, 255);
        put(27, y, 255);
        put(28, y, 255);
    }
    // Key rows: three rows of small rectangles
    for row in 0..3 {
        let y = 19 + row * 2;
        for key in 0..8 {
            let x = 6 + key * 3;
            for dx in 0..2 {
                for dy in 0..2 {
                    put(x + dx, y + dy, 255);
                }
            }
        }
    }

    rgba
}

/// White-on-transparent mouse glyph (minimal, 32x32).
pub fn mouse_rgba() -> Vec<u8> {
    const W: u32 = SIZE;
    const H: u32 = SIZE;
    let mut rgba = vec![0u8; (W * H * 4) as usize];

    let mut put = |x: u32, y: u32, intensity: u8| {
        if x >= W || y >= H {
            return;
        }
        let index = ((y * W + x) * 4) as usize;
        rgba[index] = intensity;
        rgba[index + 1] = intensity;
        rgba[index + 2] = intensity;
        rgba[index + 3] = 255;
    };

    // Mouse body: rounded teardrop shape from (6, 8) to (26, 28)
    // Top curve
    for x in 8..24 {
        put(x, 8, 255);
        put(x, 9, 255);
    }
    // Sides
    for y in 8..26 {
        put(8, y, 255);
        put(9, y, 255);
        put(22, y, 255);
        put(23, y, 255);
    }
    // Bottom point
    for x in 13..19 {
        put(x, 25, 255);
        put(x, 26, 255);
    }
    // Scroll wheel indicator
    for x in 14..18 {
        put(x, 13, 255);
        put(x, 14, 255);
    }

    rgba
}

/// White-on-transparent generic device glyph (minimal, 32x32).
pub fn generic_rgba() -> Vec<u8> {
    const W: u32 = SIZE;
    const H: u32 = SIZE;
    let mut rgba = vec![0u8; (W * H * 4) as usize];

    let mut put = |x: u32, y: u32, intensity: u8| {
        if x >= W || y >= H {
            return;
        }
        let index = ((y * W + x) * 4) as usize;
        rgba[index] = intensity;
        rgba[index + 1] = intensity;
        rgba[index + 2] = intensity;
        rgba[index + 3] = 255;
    };

    // Rounded rectangle device outline: (6, 6) to (26, 26)
    for x in 6..27 {
        put(x, 6, 255);
        put(x, 7, 255);
        put(x, 25, 255);
        put(x, 26, 255);
    }
    for y in 6..27 {
        put(6, y, 255);
        put(7, y, 255);
        put(25, y, 255);
        put(26, y, 255);
    }
    // Battery indicator inside: small battery at center
    for x in 13..19 {
        put(x, 14, 255);
        put(x, 15, 255);
        put(x, 18, 255);
        put(x, 19, 255);
    }
    for y in 14..19 {
        put(13, y, 255);
        put(14, y, 255);
        put(18, y, 255);
        put(19, y, 255);
    }
    // Terminal nub
    put(19, 16, 255);
    put(19, 17, 255);

    rgba
}

/// Returns the appropriate device icon for a device kind.
pub fn device_rgba(kind: &akku_core::DeviceKind) -> Vec<u8> {
    match kind {
        akku_core::DeviceKind::Keyboard => keyboard_rgba(),
        akku_core::DeviceKind::Mouse => mouse_rgba(),
        _ => generic_rgba(),
    }
}
