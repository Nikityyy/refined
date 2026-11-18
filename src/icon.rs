//! Procedurally generated Adobe Spectrum 2 style icon for Refined.
//! Generates crisp RGBA pixels for window, tray, and installer.

/// Returns RGBA pixel buffer of size x size.
pub fn rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let radius = s * 0.22;
    let mut out = Vec::with_capacity((size * size * 4) as usize);

    for y in 0..size {
        for x in 0..size {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            // Distance to squircle box
            let dx = (px - s / 2.0).abs() - (s / 2.0 - radius);
            let dy = (py - s / 2.0).abs() - (s / 2.0 - radius);
            let dist = if dx > 0.0 && dy > 0.0 {
                (dx * dx + dy * dy).sqrt() - radius
            } else {
                dx.max(dy) - radius
            };

            let squircle_alpha = (-dist + 0.5).clamp(0.0, 1.0);

            // Adobe Spectrum 2 vibrant gradient: Indigo to Electric Blue
            let t = (px + py) / (s * 2.0);
            let r = lerp(0x3B, 0x8B, t);
            let g = lerp(0x82, 0x5C, t);
            let b = lerp(0xF6, 0xF6, t);

            // Center Refined mark: 4-point spark / diamond
            let cx = s * 0.5;
            let cy = s * 0.5;
            let mark_dist = ((px - cx).abs() + (py - cy).abs()) / (s * 0.32);
            let mark_alpha = (1.0 - mark_dist).clamp(0.0, 1.0).powf(1.8);

            // Blend white spark into vibrant gradient
            let final_r = lerp(r, 0xFF, mark_alpha);
            let final_g = lerp(g, 0xFF, mark_alpha);
            let final_b = lerp(b, 0xFF, mark_alpha);

            out.push(final_r);
            out.push(final_g);
            out.push(final_b);
            out.push((squircle_alpha * 255.0) as u8);
        }
    }
    out
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 + (b as f32 - a as f32) * t) as u8
}
