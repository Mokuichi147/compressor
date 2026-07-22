use image::DynamicImage;
use oxipng::{optimize_from_memory, Options};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use crate::error::CompressError;
use crate::utilities::{get_aspect_ratio, write_smaller};

pub fn path2compress(path: &Path, output_path: &Path) -> Result<(), CompressError> {
    // 元データはサイズ比較に使う
    let original = std::fs::read(path)?;

    let mut options = Options::from_preset(2);
    // 改善がなくても結果を受け取り、元と比較して小さいほうを書く
    options.force = true;

    let optimized = optimize_from_memory(&original, &options)?;

    write_smaller(output_path, &optimized, &original)
}

#[allow(dead_code)]
pub fn data2compress(data: &[u8], output_path: &Path) -> Result<(), CompressError> {
    let img = image::load_from_memory(data)?;

    let mut png_data = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png_data), image::ImageFormat::Png)?;

    compress(&img, output_path)
}

#[allow(dead_code)]
pub fn get_aspect_ratio_from_path(path: &Path) -> Result<f32, CompressError> {
    // 画像を読み込む
    let img = image::open(path)?;

    Ok(get_aspect_ratio(img.width(), img.height()))
}

#[allow(dead_code)]
pub fn get_aspect_ratio_from_data(data: &[u8]) -> Result<f32, CompressError> {
    // 画像を読み込む
    let img = image::load_from_memory(data)?;

    Ok(get_aspect_ratio(img.width(), img.height()))
}

#[allow(dead_code)]
pub fn compress(img: &DynamicImage, output_path: &Path) -> Result<(), CompressError> {
    let rgba_img = img.to_rgba8().into_raw();

    let mut options = Options::from_preset(2);
    options.force = true;

    let png_data = optimize_from_memory(&rgba_img, &options)?;

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    std::io::copy(&mut &png_data[..], &mut writer)?;

    Ok(())
}
