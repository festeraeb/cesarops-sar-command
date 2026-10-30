fn main() {
    let icon_dir = std::path::Path::new("icons");
    let icon_path = icon_dir.join("icon.png");
    if !icon_path.exists() {
        std::fs::create_dir_all(icon_dir).expect("create icon directory");
        let file = std::fs::File::create(&icon_path).expect("create generated icon");
        let mut encoder = png::Encoder::new(file, 32, 32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("write icon header");
        let mut pixels = Vec::with_capacity(32 * 32 * 4);
        for y in 0..32 {
            for x in 0..32 {
                let on_cross = (13..=18).contains(&x) || (13..=18).contains(&y);
                pixels.extend_from_slice(if on_cross {
                    &[246, 173, 47, 255]
                } else {
                    &[17, 26, 29, 255]
                });
            }
        }
        writer.write_image_data(&pixels).expect("write icon pixels");
    }
    tauri_build::build()
}
