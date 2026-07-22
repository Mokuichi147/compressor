use image::{DynamicImage, ImageDecoder};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use webp::Encoder;
use crate::error::CompressError;

/// 画像を読み込み、Exif の Orientation をピクセルに反映して返す。
///
/// WebP には Exif をそのまま引き継げないため、向きの情報をピクセル側に焼き込む。
/// これをしないと、縦向きに撮影した写真が横倒しで出力される。
fn open_with_orientation(path: &Path) -> Result<DynamicImage, CompressError> {
    let mut decoder = image::ImageReader::open(path)?
        .with_guessed_format()?
        .into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);

    Ok(img)
}

/// jpg/jpeg 向け: 非可逆 WebP に圧縮する（quality は 0-100）。
pub fn path2compress_lossy(path: &Path, output_path: &Path, quality: f32) -> Result<(), CompressError> {
    let img = open_with_orientation(path)?;
    let rgb = img.to_rgb8();

    let encoder = Encoder::from_rgb(rgb.as_raw(), rgb.width(), rgb.height());
    let data = encoder.encode(quality);

    write_file(output_path, &data)
}

/// png 向け: 可逆 WebP に圧縮する（アルファ保持）。
pub fn path2compress_lossless(path: &Path, output_path: &Path) -> Result<(), CompressError> {
    let img = open_with_orientation(path)?;
    let rgba = img.to_rgba8();

    let encoder = Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
    let data = encoder.encode_lossless();

    write_file(output_path, &data)
}

fn write_file(output_path: &Path, data: &[u8]) -> Result<(), CompressError> {
    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    std::io::copy(&mut &data[..], &mut writer)?;

    Ok(())
}
