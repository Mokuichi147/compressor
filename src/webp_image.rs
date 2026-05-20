use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use webp::Encoder;

/// jpg/jpeg 向け: 非可逆 WebP に圧縮する（quality は 0-100）。
pub fn path2compress_lossy(path: &PathBuf, output_path: &PathBuf, quality: f32) {
    let img = image::open(path).unwrap();
    let rgb = img.to_rgb8();

    let encoder = Encoder::from_rgb(rgb.as_raw(), rgb.width(), rgb.height());
    let data = encoder.encode(quality);

    write_file(output_path, &data);
}

/// png 向け: 可逆 WebP に圧縮する（アルファ保持）。
pub fn path2compress_lossless(path: &PathBuf, output_path: &PathBuf) {
    let img = image::open(path).unwrap();
    let rgba = img.to_rgba8();

    let encoder = Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
    let data = encoder.encode_lossless();

    write_file(output_path, &data);
}

fn write_file(output_path: &PathBuf, data: &[u8]) {
    let file = File::create(output_path).unwrap();
    let mut writer = BufWriter::new(file);
    std::io::copy(&mut &data[..], &mut writer).unwrap();
}
