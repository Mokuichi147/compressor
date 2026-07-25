use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use image::codecs::gif::GifDecoder;
use image::AnimationDecoder;
use crate::error::CompressError;

/// アニメーションGIF（2フレーム以上）かどうかを判定する。
/// 先頭2フレームのみを遅延デコードして数えるため、巨大なGIFでも軽い。
pub fn is_animated(path: &Path) -> Result<bool, CompressError> {
    let file = File::open(path)?;
    let decoder = GifDecoder::new(BufReader::new(file))?;

    Ok(decoder.into_frames().take(2).count() > 1)
}
