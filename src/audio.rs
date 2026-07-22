use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use crate::error::CompressError;

/// 可逆音源の拡張子。既定でFLACに可逆圧縮する。
const LOSSLESS_EXTENSIONS: [&str; 4] = ["wav", "aiff", "aif", "flac"];
/// 非可逆音源の拡張子。既定で非可逆再エンコードする。
const LOSSY_EXTENSIONS: [&str; 5] = ["mp3", "m4a", "aac", "ogg", "wma"];

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

impl AudioCodec {
    /// コーデックに対応する出力拡張子
    pub fn extension(self) -> &'static str {
        match self {
            AudioCodec::Flac => "flac",
            AudioCodec::Aac => "m4a",
            AudioCodec::Opus => "opus",
        }
    }
}

/// 拡張子を小文字で取り出す
fn normalized_extension(input_path: &str) -> Option<String> {
    Path::new(input_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
}

/// 対応する音声拡張子かどうかを判定する
pub fn is_match_extension(input_path: &str) -> bool {
    // 入力ファイルの存在チェック
    if !Path::new(input_path).exists() {
        return false;
    }

    matches!(
        normalized_extension(input_path),
        Some(ext) if LOSSLESS_EXTENSIONS.contains(&ext.as_str())
            || LOSSY_EXTENSIONS.contains(&ext.as_str())
    )
}

/// 拡張子から、入力が可逆音源（WAV/AIFF/FLAC）かどうかを判定する。
/// 可逆音源は既定でFLACに圧縮し、非可逆音源（MP3/AAC等）は既定で非可逆再エンコードする。
pub fn is_lossless_source(input_path: &str) -> bool {
    matches!(
        normalized_extension(input_path),
        Some(ext) if LOSSLESS_EXTENSIONS.contains(&ext.as_str())
    )
}

/// FFmpegが使えるかを判定する。プロセス起動を伴うため一度だけ実行して結果を使い回す。
fn is_ffmpeg_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| Command::new("ffmpeg").arg("-version").output().is_ok())
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
    if !is_ffmpeg_available() {
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

    match codec {
        // FLAC/M4A はカバーアートを埋め込めるため、音声と併せて無変換で引き継ぐ。
        // 字幕・データストリームは対象コンテナに入らずmuxに失敗しうるので映像だけを拾う。
        AudioCodec::Flac | AudioCodec::Aac => {
            command.args(["-map", "0:a", "-map", "0:v?", "-c:v", "copy"]);
            command.args(["-disposition:v", "attached_pic"]);
        }
        // Opus（oggコンテナ）はカバーアートの埋め込みが素直に通らないため映像を落とす
        AudioCodec::Opus => {
            command.arg("-vn");
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 可逆音源のみが可逆判定になること
    #[test]
    fn detects_lossless_sources() {
        for ext in LOSSLESS_EXTENSIONS {
            assert!(is_lossless_source(&format!("song.{ext}")), "{ext} が可逆と判定されない");
        }
        for ext in LOSSY_EXTENSIONS {
            assert!(!is_lossless_source(&format!("song.{ext}")), "{ext} が可逆と判定された");
        }
    }

    /// 大文字の拡張子でも判定できること
    #[test]
    fn extension_check_is_case_insensitive() {
        assert!(is_lossless_source("song.WAV"));
        assert!(!is_lossless_source("song.MP3"));
    }

    /// 拡張子がない・対象外の場合は可逆扱いしないこと
    #[test]
    fn non_audio_is_not_lossless() {
        assert!(!is_lossless_source("song"));
        assert!(!is_lossless_source("clip.mp4"));
    }

    /// コーデックごとの出力拡張子
    #[test]
    fn codec_extensions() {
        assert_eq!(AudioCodec::Flac.extension(), "flac");
        assert_eq!(AudioCodec::Aac.extension(), "m4a");
        assert_eq!(AudioCodec::Opus.extension(), "opus");
    }
}
