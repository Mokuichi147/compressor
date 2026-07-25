use image::DynamicImage;
use oxipng::{optimize_from_memory, Options};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::Instant;
use crate::error::CompressError;
use crate::stats::CompressionStats;
use crate::utilities::{copy_modified_time, get_aspect_ratio, write_smaller};

pub fn path2compress(path: &Path, output_path: &Path) -> Result<CompressionStats, CompressError> {
    let start = Instant::now();

    // 元データはサイズ比較に使う
    let original = std::fs::read(path)?;

    let mut options = Options::from_preset(2);
    // 改善がなくても結果を受け取り、元と比較して小さいほうを書く
    options.force = true;

    let optimized = optimize_from_memory(&original, &options)?;

    write_smaller(output_path, &optimized, &original)?;
    copy_modified_time(path, output_path)?;

    CompressionStats::measure(path, output_path, start)
}

/// PNG以外の可逆画像（GIF / TIFF / BMP）をPNGに変換し、oxipng で最適化して出力する。
///
/// これらは元のバイト列を直接 oxipng に渡せないため、一度デコードして PNG にし直す。
/// 出力形式が変わるので、[`write_smaller`] による「元より大きければ元を出す」保護は使えない。
pub fn decode2compress_png(
    path: &Path,
    output_path: &Path,
) -> Result<CompressionStats, CompressError> {
    let start = Instant::now();

    let img = image::open(path)?;

    // oxipng は PNG バイト列を入力に取るため、一度 PNG にエンコードしてから最適化する。
    let mut png_buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png_buf), image::ImageFormat::Png)?;

    let mut options = Options::from_preset(2);
    options.force = true;
    let png_data = optimize_from_memory(&png_buf, &options)?;

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    std::io::copy(&mut &png_data[..], &mut writer)?;
    drop(writer);

    copy_modified_time(path, output_path)?;

    CompressionStats::measure(path, output_path, start)
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
