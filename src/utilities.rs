use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use crate::error::CompressError;

/// 圧縮結果と元データのうち、小さいほうを書き出す。
///
/// 既に圧縮済みのファイルを再エンコードすると、サイズが増えたうえに画質だけ落ちることがある。
/// 入力と出力が同じ形式のときにのみ使えることに注意（webp変換などでは元データを書けない）。
pub fn write_smaller(
    output_path: &Path,
    compressed: &[u8],
    original: &[u8],
) -> Result<(), CompressError> {
    let data = if compressed.len() < original.len() {
        compressed
    } else {
        original
    };

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(data)?;

    Ok(())
}

/// FFmpegが使えるかを判定する。プロセス起動を伴うため一度だけ実行して結果を使い回す。
pub fn is_ffmpeg_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| Command::new("ffmpeg").arg("-version").output().is_ok())
}

pub fn get_aspect_ratio(width: u32, height: u32) -> f32 {
    if width == 0 || height == 0 {
        return 0.0;
    }

    (width as f32) / (height as f32)
}
