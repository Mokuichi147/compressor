use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::Instant;
use webp::Encoder;
use crate::error::CompressError;
use crate::stats::CompressionStats;
use crate::utilities::{copy_modified_time, open_with_orientation, resize_to_fit};

/// jpg/jpeg 向け: 非可逆 WebP に圧縮する（quality は 0-100）。
pub fn path2compress_lossy(
    path: &Path,
    output_path: &Path,
    quality: f32,
    max_long_side: Option<u32>,
) -> Result<CompressionStats, CompressError> {
    let start = Instant::now();

    let img = resize_to_fit(open_with_orientation(path)?, max_long_side);
    let rgb = img.to_rgb8();

    let encoder = Encoder::from_rgb(rgb.as_raw(), rgb.width(), rgb.height());
    let data = encoder.encode(quality);

    write_file(output_path, &data)?;
    copy_modified_time(path, output_path)?;

    CompressionStats::measure(path, output_path, start)
}

/// png 向け: 可逆 WebP に圧縮する（アルファ保持）。
pub fn path2compress_lossless(
    path: &Path,
    output_path: &Path,
    max_long_side: Option<u32>,
) -> Result<CompressionStats, CompressError> {
    let start = Instant::now();

    let img = resize_to_fit(open_with_orientation(path)?, max_long_side);
    let rgba = img.to_rgba8();

    let encoder = Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
    let data = encoder.encode_lossless();

    write_file(output_path, &data)?;
    copy_modified_time(path, output_path)?;

    CompressionStats::measure(path, output_path, start)
}

fn write_file(output_path: &Path, data: &[u8]) -> Result<(), CompressError> {
    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    std::io::copy(&mut &data[..], &mut writer)?;

    Ok(())
}
