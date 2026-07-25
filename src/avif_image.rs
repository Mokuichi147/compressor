use ravif::{Encoder, Img, RGB8, RGBA8};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use crate::error::CompressError;
use crate::stats::CompressionStats;
use crate::utilities::{copy_modified_time, open_with_orientation, resize_to_fit};
use std::time::Instant;

/// ravif が受け付ける品質の範囲。範囲外を渡すと panic するため、必ず通してから使う。
fn clamp_quality(quality: f32) -> f32 {
    quality.clamp(1.0, 100.0)
}

/// jpg/jpeg 向け: 非可逆 AVIF に圧縮する（quality は 1-100）。
pub fn path2compress_lossy(
    path: &Path,
    output_path: &Path,
    quality: f32,
    max_long_side: Option<u32>,
) -> Result<CompressionStats, CompressError> {
    let start = Instant::now();

    let img = resize_to_fit(open_with_orientation(path)?, max_long_side);
    let rgb = img.to_rgb8();
    let (width, height) = (rgb.width() as usize, rgb.height() as usize);

    let pixels: Vec<RGB8> = rgb
        .pixels()
        .map(|p| RGB8::new(p.0[0], p.0[1], p.0[2]))
        .collect();

    let encoded = Encoder::new()
        .with_quality(clamp_quality(quality))
        .encode_rgb(Img::new(pixels.as_slice(), width, height))
        .map_err(|e| CompressError::Avif(e.to_string()))?;

    write_file(output_path, &encoded.avif_file)?;
    copy_modified_time(path, output_path)?;

    CompressionStats::measure(path, output_path, start)
}

/// png 向け: アルファを保ったまま非可逆 AVIF に圧縮する（quality は 1-100）。
///
/// `--webp` の png と違って**可逆にはならない**。ravif が使う rav1e は
/// 量子化を 0 にしても完全な可逆にはならず、実測でも往復でピクセルが一致しない。
/// アルファは欠けが目立ちやすいため、色より高い品質で符号化する。
pub fn path2compress_lossy_rgba(
    path: &Path,
    output_path: &Path,
    quality: f32,
    max_long_side: Option<u32>,
) -> Result<CompressionStats, CompressError> {
    let start = Instant::now();

    let img = resize_to_fit(open_with_orientation(path)?, max_long_side);
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width() as usize, rgba.height() as usize);

    let pixels: Vec<RGBA8> = rgba
        .pixels()
        .map(|p| RGBA8::new(p.0[0], p.0[1], p.0[2], p.0[3]))
        .collect();

    let encoded = Encoder::new()
        .with_quality(clamp_quality(quality))
        .with_alpha_quality(100.0)
        .encode_rgba(Img::new(pixels.as_slice(), width, height))
        .map_err(|e| CompressError::Avif(e.to_string()))?;

    write_file(output_path, &encoded.avif_file)?;
    copy_modified_time(path, output_path)?;

    CompressionStats::measure(path, output_path, start)
}

fn write_file(output_path: &Path, data: &[u8]) -> Result<(), CompressError> {
    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    std::io::copy(&mut &data[..], &mut writer)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ravif は範囲外の品質で panic するため、呼ぶ前に丸めること
    #[test]
    fn clamps_quality_into_supported_range() {
        assert_eq!(clamp_quality(0.0), 1.0);
        assert_eq!(clamp_quality(-10.0), 1.0);
        assert_eq!(clamp_quality(70.0), 70.0);
        assert_eq!(clamp_quality(150.0), 100.0);
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// AVIF として読める（ftyp が avif の）ファイルを出力すること
    #[test]
    fn writes_avif_container() {
        let dir = temp_dir("compressor_avif_container");
        let source = dir.join("a.png");
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(8, 8))
            .save(&source)
            .unwrap();

        let target = dir.join("a.avif");
        path2compress_lossy_rgba(&source, &target, 70.0, None).unwrap();

        let data = std::fs::read(&target).unwrap();
        assert_eq!(&data[4..12], b"ftypavif", "AVIFのftypになっていない");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 品質 0 を渡しても panic せずに出力できること
    #[test]
    fn does_not_panic_on_zero_quality() {
        let dir = temp_dir("compressor_avif_quality");
        let source = dir.join("a.png");
        image::DynamicImage::ImageRgb8(image::RgbImage::new(8, 8))
            .save(&source)
            .unwrap();

        let target = dir.join("a.avif");
        assert!(path2compress_lossy(&source, &target, 0.0, None).is_ok());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
