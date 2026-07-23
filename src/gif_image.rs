use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor};
use std::path::Path;
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, ImageFormat};
use oxipng::{optimize_from_memory, Options};
use crate::error::CompressError;

/// アニメーションGIF（2フレーム以上）かどうかを判定する。
/// 先頭2フレームのみを遅延デコードして数えるため、巨大なGIFでも軽い。
pub fn is_animated(path: &Path) -> Result<bool, CompressError> {
    let file = File::open(path)?;
    let decoder = GifDecoder::new(BufReader::new(file))?;

    Ok(decoder.into_frames().take(2).count() > 1)
}

/// 静止GIFの先頭フレームを oxipng で最適化した PNG として出力する。
pub fn path2compress_png(path: &Path, output_path: &Path) -> Result<(), CompressError> {
    let img = image::open(path)?;

    // oxipng は PNG バイト列を入力に取るため、一度 PNG にエンコードしてから最適化する。
    let mut png_buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_buf), ImageFormat::Png)?;

    let mut options = Options::from_preset(2);
    options.force = true;
    let png_data = optimize_from_memory(&png_buf, &options)?;

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    std::io::copy(&mut &png_data[..], &mut writer)?;

    Ok(())
}
