use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor};
use std::path::PathBuf;
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, ImageFormat};
use oxipng::{optimize_from_memory, Options};

/// アニメーションGIF（2フレーム以上）かどうかを判定する。
/// 先頭2フレームのみを遅延デコードして数えるため、巨大なGIFでも軽い。
pub fn is_animated(path: &PathBuf) -> bool {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let decoder = match GifDecoder::new(BufReader::new(file)) {
        Ok(d) => d,
        Err(_) => return false,
    };
    decoder.into_frames().take(2).count() > 1
}

/// 静止GIFの先頭フレームを oxipng で最適化した PNG として出力する。
pub fn path2compress_png(path: &PathBuf, output_path: &PathBuf) {
    let img = image::open(path).unwrap();

    // oxipng は PNG バイト列を入力に取るため、一度 PNG にエンコードしてから最適化する。
    let mut png_buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_buf), ImageFormat::Png).unwrap();

    let mut options = Options::from_preset(2);
    options.force = true;
    let png_data = optimize_from_memory(&png_buf, &options).unwrap();

    let file = File::create(output_path).unwrap();
    let mut writer = BufWriter::new(file);
    std::io::copy(&mut &png_data[..], &mut writer).unwrap();
}
