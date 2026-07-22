use std::path::Path;
use std::process::Command;
use crate::error::CompressError;

/// 音声の出力コーデック
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    /// FLAC。可逆圧縮。
    Flac,
    /// AAC。非可逆圧縮の既定。
    Aac,
    /// Opus (libopus)。非可逆圧縮。AACより低ビットレートで高音質になりやすい。
    Opus,
}

/// 対応する音声拡張子かどうかを判定する
pub fn is_match_extension(input_path: &str) -> bool {
    let path = Path::new(input_path);

    // 入力ファイルの存在チェック
    if !path.exists() {
        return false;
    }

    let audio_extensions = [".wav", ".aiff", ".aif", ".flac", ".mp3", ".m4a", ".aac", ".ogg", ".wma"];
    let extension = path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{}", ext.to_lowercase()));

    match extension {
        Some(ext) if audio_extensions.contains(&ext.as_str()) => true,
        _ => false,
    }
}

/// 拡張子から、入力が可逆音源（WAV/AIFF/FLAC）かどうかを判定する。
/// 可逆音源は既定でFLACに圧縮し、非可逆音源（MP3/AAC等）は既定で非可逆再エンコードする。
pub fn is_lossless_source(input_path: &str) -> bool {
    let path = Path::new(input_path);
    let lossless_extensions = [".wav", ".aiff", ".aif", ".flac"];
    let extension = path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{}", ext.to_lowercase()));

    matches!(extension, Some(ext) if lossless_extensions.contains(&ext.as_str()))
}

/// 音声ファイルを圧縮する関数
///
/// # 引数
///
/// * `input_path` - 入力元の音声ファイルパス
/// * `output_path` - 圧縮後の出力先ファイルパス
/// * `codec` - 出力コーデック（FLAC/AAC/Opus）
/// * `bitrate` - 非可逆圧縮時のビットレート（例: "128k"）。FLACでは無視される
pub fn path2compress(
    input_path: &Path,
    output_path: &Path,
    codec: AudioCodec,
    bitrate: &str,
) -> Result<(), CompressError> {
    // FFmpegの存在チェック
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        return Err(CompressError::Ffmpeg(
            "FFmpegがインストールされていないか、PATHに含まれていません".to_string(),
        ));
    }

    // 出力ディレクトリの存在チェックと作成
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let mut command = Command::new("ffmpeg");
    command.arg("-i").arg(input_path);

    // カバーアート等の映像ストリームを除去し、音声のみを対象にする
    command.arg("-vn");

    match codec {
        AudioCodec::Flac => {
            command.args(["-c:a", "flac", "-compression_level", "8"]);
        }
        AudioCodec::Aac => {
            command.args(["-c:a", "aac", "-b:a", bitrate]);
        }
        AudioCodec::Opus => {
            command.args(["-c:a", "libopus", "-b:a", bitrate]);
        }
    }

    let status = command
        .arg("-y") // 確認なしで上書き
        .arg(output_path)
        .status()
        .map_err(|e| CompressError::Ffmpeg(format!("FFmpegの実行に失敗: {e}")))?;

    if !status.success() {
        return Err(CompressError::Ffmpeg(format!("FFmpegがエラーコードで終了: {status}")));
    }

    Ok(())
}
