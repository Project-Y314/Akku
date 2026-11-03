//! Procedurally drawn 32x32 battery icons for the tray and the window.
//!
//! Pure black / white pixels only; the level fill follows the battery percent.
//! A `+` glyph marks charging.

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
