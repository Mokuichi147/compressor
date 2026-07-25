use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;
use std::time::Instant;
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, DynamicImage, RgbaImage};
use webp::{AnimEncoder, AnimFrame, WebPConfig};
use crate::error::CompressError;
use crate::stats::CompressionStats;
use crate::utilities::{copy_modified_time, resize_to_fit};

/// 遅延が 0 のフレームに使う表示時間（ミリ秒）。
///
/// 遅延 0 の GIF は珍しくないが、そのまま渡すと全フレームの表示時刻が同じになり
/// アニメーションが壊れる。ブラウザが GIF の遅延 0 に充てる値に合わせる。
const FALLBACK_FRAME_DELAY_MS: i32 = 100;

/// アニメーションGIF（2フレーム以上）かどうかを判定する。
/// 先頭2フレームのみを遅延デコードして数えるため、巨大なGIFでも軽い。
pub fn is_animated(path: &Path) -> Result<bool, CompressError> {
    let file = File::open(path)?;
    let decoder = GifDecoder::new(BufReader::new(file))?;

    Ok(decoder.into_frames().take(2).count() > 1)
}

/// アニメーションGIFを可逆のアニメーションWebPとして出力する。
///
/// mp4 に変換すると透過とループが失われるため、それらを保ちたい場合の出力先。
/// 静止GIFを可逆WebPにするのと揃えて、こちらも可逆で符号化する。
pub fn path2compress_animated_webp(
    path: &Path,
    output_path: &Path,
    max_long_side: Option<u32>,
) -> Result<CompressionStats, CompressError> {
    let start = Instant::now();

    let loop_count = read_loop_count(path)?;

    // AnimFrame はピクセルデータを借用するため、先に全フレームを読み切って保持する
    let frames = decode_frames(path, max_long_side)?;
    let Some((first, _)) = frames.first() else {
        return Err(CompressError::Webp("GIFにフレームがありません".to_string()));
    };
    let (width, height) = (first.width(), first.height());

    let mut config =
        WebPConfig::new().map_err(|_| CompressError::Webp("WebPの設定を作れません".to_string()))?;
    config.lossless = 1;
    // 可逆モードでは quality は圧縮の手間の指標として使われる
    config.quality = 75.0;

    let mut encoder = AnimEncoder::new(width, height, &config);
    encoder.set_loop_count(loop_count);
    for (image, timestamp) in &frames {
        encoder.add_frame(AnimFrame::from_rgba(image.as_raw(), width, height, *timestamp));
    }

    let data = encoder
        .try_encode()
        .map_err(|e| CompressError::Webp(format!("{e:?}")))?;

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    std::io::copy(&mut &data[..], &mut writer)?;
    drop(writer);

    copy_modified_time(path, output_path)?;

    CompressionStats::measure(path, output_path, start)
}

/// 各フレームと、その表示を開始する時刻（先頭からの累計ミリ秒）を返す。
fn decode_frames(
    path: &Path,
    max_long_side: Option<u32>,
) -> Result<Vec<(RgbaImage, i32)>, CompressError> {
    let file = File::open(path)?;
    let decoder = GifDecoder::new(BufReader::new(file))?;

    let mut frames = Vec::new();
    let mut timestamp = 0i32;
    for frame in decoder.into_frames() {
        let frame = frame?;

        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let delay = if denominator == 0 {
            0
        } else {
            (numerator / denominator) as i32
        };

        // 全フレームが同じ大きさである必要があるため、縮小もフレームごとに同じ上限で行う
        let image = resize_to_fit(DynamicImage::ImageRgba8(frame.into_buffer()), max_long_side);

        frames.push((image.to_rgba8(), timestamp));
        timestamp += if delay > 0 { delay } else { FALLBACK_FRAME_DELAY_MS };
    }

    Ok(frames)
}

/// GIF のループ回数を読む。WebP では 0 が無限ループを表す。
///
/// `image` のデコーダは NETSCAPE 拡張のループ回数を公開していないため、
/// `gif` クレートで直接読む。
fn read_loop_count(path: &Path) -> Result<i32, CompressError> {
    let file = File::open(path)?;
    let decoder = gif::DecodeOptions::new()
        .read_info(BufReader::new(file))
        .map_err(|e| CompressError::Webp(format!("GIFのループ回数を読めません: {e}")))?;

    Ok(match decoder.repeat() {
        gif::Repeat::Infinite => 0,
        gif::Repeat::Finite(count) => i32::from(count),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Frame, ImageFormat};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 半透明を含む2フレームのアニメーションGIFを作る
    fn write_animated_gif(path: &Path, delay_ms: u32) {
        let mut frames = Vec::new();
        for alpha in [255u8, 0u8] {
            let mut image = RgbaImage::new(8, 8);
            for pixel in image.pixels_mut() {
                *pixel = image::Rgba([200, 50, 50, alpha]);
            }
            frames.push(Frame::from_parts(
                image,
                0,
                0,
                Delay::from_numer_denom_ms(delay_ms, 1),
            ));
        }

        let file = File::create(path).unwrap();
        let mut encoder = GifEncoder::new(BufWriter::new(file));
        encoder.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
        encoder.encode_frames(frames).unwrap();
    }

    /// アニメーションWebPとして出力できること（RIFF/WEBP かつ ANIM チャンクを持つ）
    #[test]
    fn writes_animated_webp() {
        let dir = temp_dir("compressor_anim_webp");
        let source = dir.join("a.gif");
        write_animated_gif(&source, 100);

        let target = dir.join("a.webp");
        path2compress_animated_webp(&source, &target, None).unwrap();

        let data = std::fs::read(&target).unwrap();
        assert_eq!(&data[0..4], b"RIFF");
        assert_eq!(&data[8..12], b"WEBP");
        assert!(
            data.windows(4).any(|w| w == b"ANIM"),
            "ANIMチャンクが無く、アニメーションになっていない"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 遅延が0でも全フレームが同じ時刻に潰れないこと
    #[test]
    fn zero_delay_does_not_collapse_frames() {
        let dir = temp_dir("compressor_anim_webp_zero_delay");
        let source = dir.join("a.gif");
        write_animated_gif(&source, 0);

        let frames = decode_frames(&source, None).unwrap();
        assert_eq!(frames.len(), 2);
        let timestamps: Vec<i32> = frames.iter().map(|(_, timestamp)| *timestamp).collect();
        assert!(
            timestamps[1] > timestamps[0],
            "2フレーム目の時刻が進んでいない: {timestamps:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 縮小指定があれば全フレームが同じ大きさに揃うこと（揃わないとエンコードに失敗する）
    #[test]
    fn resizes_every_frame_equally() {
        let dir = temp_dir("compressor_anim_webp_resize");
        let source = dir.join("a.gif");
        write_animated_gif(&source, 100);

        let frames = decode_frames(&source, Some(4)).unwrap();
        assert!(frames.iter().all(|(image, _)| image.dimensions() == (4, 4)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 無限ループのGIFが WebP の 0（無限）になること
    #[test]
    fn infinite_loop_maps_to_zero() {
        let dir = temp_dir("compressor_anim_webp_loop");
        let source = dir.join("a.gif");
        write_animated_gif(&source, 100);

        assert_eq!(read_loop_count(&source).unwrap(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// アニメーションGIFの判定が変わっていないこと
    #[test]
    fn detects_animation() {
        let dir = temp_dir("compressor_anim_webp_detect");
        let animated = dir.join("anim.gif");
        write_animated_gif(&animated, 100);
        assert!(is_animated(&animated).unwrap());

        let still = dir.join("still.gif");
        DynamicImage::ImageRgba8(RgbaImage::new(4, 4))
            .save_with_format(&still, ImageFormat::Gif)
            .unwrap();
        assert!(!is_animated(&still).unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
