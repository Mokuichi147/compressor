pub fn get_aspect_ratio(width: u32, height: u32) -> f32 {
    if width == 0 || height == 0 {
        return 0.0;
    }

    (width as f32) / (height as f32)
}