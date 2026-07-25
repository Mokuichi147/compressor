use std::path::Path;
use std::process::Command;
use std::time::Instant;
use crate::error::CompressError;
use crate::stats::CompressionStats;
use crate::utilities::{
    capped_bitrate, copy_modified_time, is_ffmpeg_available, probe_audio_stream,
    replace_with_original_if_larger, same_extension,
};

/// 可逆音源の拡張子。既定でFLACに可逆圧縮する。
const LOSSLESS_EXTENSIONS: [&str; 4] = ["wav", "aiff", "aif", "flac"];
/// 非可逆音源の拡張子。既定で非可逆再エンコードする。
/// m4b はオーディオブック、mka は Matroska の音声のみのコンテナ。
const LOSSY_EXTENSIONS: [&str; 8] = ["mp3", "m4a", "m4b", "aac", "ogg", "opus", "wma", "mka"];

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

/// 音声ファイルを圧縮する関数
///
/// # 引数
///
/// * `input_path` - 入力元の音声ファイルパス
/// * `output_path` - 圧縮後の出力先ファイルパス
/// * `codec` - 出力コーデック（FLAC/AAC/Opus）
/// * `bitrate` - 非可逆圧縮時のビットレート（例: "128k"）。FLACでは無視される
///
/// 非可逆圧縮では、指定ビットレートを元の音声のビットレートで頭打ちにする。
/// 64kbps の音源を 128k で再エンコードしても音質は戻らず、サイズだけが増えるため。
pub fn path2compress(
    input_path: &Path,
    output_path: &Path,
    codec: AudioCodec,
    bitrate: &str,
) -> Result<CompressionStats, CompressError> {
    let start = Instant::now();

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

    // 非可逆圧縮では元のビットレートを上限にする。可逆圧縮（FLAC）では使わないため取得もしない。
    let bitrate = match codec {
        AudioCodec::Flac => bitrate.to_string(),
        AudioCodec::Aac | AudioCodec::Opus => {
            let source_bps = probe_audio_stream(input_path).and_then(|info| info.bitrate_bps);
            capped_bitrate(bitrate, source_bps)
        }
    };

    let mut command = Command::new("ffmpeg");
    command.arg("-i").arg(input_path);

    // タグ（アーティスト・アルバムなど）を明示的に引き継ぐ
    command.args(["-map_metadata", "0"]);

    match codec {
        AudioCodec::Flac => {
            command.args(["-c:a", "flac", "-compression_level", "8"]);
        }
        AudioCodec::Aac => {
            command.args(["-c:a", "aac", "-b:a", &bitrate]);
        }
        AudioCodec::Opus => {
            command.args(["-c:a", "libopus", "-b:a", &bitrate]);
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

    // 既に十分圧縮された音源を再エンコードすると、サイズが増えたうえに音質だけ落ちることがある。
    // 形式が変わらない場合（flac→flac など）に限り、元のほうが小さければ元を出力する。
    if same_extension(input_path, output_path) {
        replace_with_original_if_larger(input_path, output_path)?;
    }

    // 元ファイルで置き換えた場合も更新日時を揃えたいので、コピーの後に行う
    copy_modified_time(input_path, output_path)?;

    CompressionStats::measure(input_path, output_path, start)
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

    /// 対応拡張子すべてが音声として判定されること
    #[test]
    fn matches_all_audio_extensions() {
        let dir = std::env::temp_dir().join("compressor_audio_extensions");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        for ext in LOSSLESS_EXTENSIONS.iter().chain(LOSSY_EXTENSIONS.iter()) {
            let path = dir.join(format!("song.{ext}"));
            std::fs::write(&path, b"").unwrap();
            assert!(is_match_extension(path.to_str().unwrap()), "{ext} が音声と判定されない");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 出力形式である opus を入力としても受け付けること（対応が非対称にならないように）
    #[test]
    fn opus_is_accepted_as_input() {
        assert!(LOSSY_EXTENSIONS.contains(&"opus"));
        assert!(!is_lossless_source("song.opus"));
    }

    /// 可逆と非可逆の拡張子が重複していないこと
    #[test]
    fn extension_lists_do_not_overlap() {
        for ext in LOSSY_EXTENSIONS {
            assert!(!LOSSLESS_EXTENSIONS.contains(&ext), "{ext} が両方のリストにある");
        }
    }

    /// コーデックごとの出力拡張子
    #[test]
    fn codec_extensions() {
        assert_eq!(AudioCodec::Flac.extension(), "flac");
        assert_eq!(AudioCodec::Aac.extension(), "m4a");
        assert_eq!(AudioCodec::Opus.extension(), "opus");
    }
}
