use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use std::time::Instant;
use crate::error::CompressError;
use crate::utilities::{get_aspect_ratio, is_ffmpeg_available};

/// 動画圧縮の結果統計情報
#[allow(dead_code)]
pub struct CompressionStats {
    /// 元のファイルサイズ（バイト）
    pub original_size: u64,
    /// 圧縮後のファイルサイズ（バイト）
    pub compressed_size: u64,
    /// サイズ削減率（%）
    pub size_reduction_percent: f64,
    /// 圧縮にかかった時間（秒）
    pub duration_seconds: f64,
}

/// 動画の出力コーデック
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    /// AV1 (libsvtav1)。最も圧縮率が高い。既定。
    Av1,
    /// HEVC/H.265 (libx265, hvc1 タグ)。iOS など旧来デバイスでの再生互換性が高い。
    Hevc,
}

impl VideoCodec {
    /// 未指定時に使う、コーデックごとの既定 CRF。
    /// CRF スケールはコーデック間で異なるため値を分ける。
    fn default_crf(self) -> u8 {
        match self {
            VideoCodec::Av1 => 40,
            VideoCodec::Hevc => 28,
        }
    }
}

pub fn path2compress(
    input_path: &str,
    output_path: &str,
    codec: VideoCodec,
    crf: Option<u8>,
) -> Result<CompressionStats, CompressError> {
    let crf = crf.unwrap_or_else(|| codec.default_crf());
    compress_video(input_path, output_path, codec, crf)
}

pub fn is_match_extension(input_path: &str) -> bool {
    let path = Path::new(input_path);
    
    // 入力ファイルの存在チェック
    if !path.exists() {
        return false;
    }

    let video_extensions = [".mov", ".mp4", ".avi", ".mkv", ".webm"];
    let extension = path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{}", ext.to_lowercase()));
    
    match extension {
        Some(ext) if video_extensions.contains(&ext.as_str()) => true,
        _ => false,
    }
}

/// 動画ファイルを圧縮する関数
///
/// CRF を尊重するソフトウェアエンコーダ（AV1: libsvtav1, HEVC: libx265）を用いる。
/// ハードウェアエンコーダ（videotoolbox/nvenc）は `-crf` を無視して圧縮率が落ちるため使わない。
///
/// # 引数
///
/// * `input_path` - 入力元の動画ファイルパス
/// * `output_path` - 圧縮後の出力先ファイルパス
/// * `codec` - 出力コーデック（AV1 もしくは HEVC）
/// * `crf` - Constant Rate Factor（低いほど高画質・大きいファイル）
///
/// # 戻り値
///
/// * `Result<CompressionStats, CompressError>` - 成功時は圧縮統計情報、失敗時はエラー
///
/// # 例
///
/// ```ignore
/// let result = compress_video(
///     "/path/to/input.mp4",
///     "/path/to/output.mp4",
///     VideoCodec::Av1,
///     40,
/// );
/// match result {
///     Ok(stats) => println!("圧縮完了: {}% 削減", stats.size_reduction_percent),
///     Err(e) => eprintln!("エラー: {}", e),
/// }
/// ```
pub fn compress_video(
    input_path: &str,
    output_path: &str,
    codec: VideoCodec,
    crf: u8,
) -> Result<CompressionStats, CompressError> {
    // 開始時間を記録
    let start = Instant::now();
    let output_file_path = PathBuf::from(output_path);

    // 出力ディレクトリの存在チェックと作成
    if let Some(parent) = output_file_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    // 元のファイルサイズを取得
    let metadata = fs::metadata(input_path)?;
    let original_size = metadata.len();

    // FFmpegの存在チェック
    if !is_ffmpeg_available() {
        return Err(CompressError::Ffmpeg(
            "FFmpegがインストールされていないか、PATHに含まれていません".to_string(),
        ));
    }

    // 動画の解像度とアスペクト比を取得
    let probe_output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height")
        .arg("-of")
        .arg("csv=p=0")
        .arg(input_path)
        .output()
        .map_err(|e| CompressError::Ffmpeg(format!("ffprobeの実行に失敗: {e}")))?;
    
    let dimensions = String::from_utf8_lossy(&probe_output.stdout);
    let dimensions: Vec<&str> = dimensions.trim().split(',').collect();
    
    let mut resize_filter = String::new();
    
    // 解像度情報が正しく取得できた場合
    if dimensions.len() == 2 {
        if let (Ok(width), Ok(height)) = (dimensions[0].parse::<u32>(), dimensions[1].parse::<u32>()) {
            // アスペクト比を計算
            let aspect_ratio = get_aspect_ratio(width, height);

            // 16:9のアスペクト比は約1.778
            let is_16_9 = aspect_ratio >= 1.775 && aspect_ratio <= 1.781;
            
            // 16:9かつフルHD（1920x1080）を超える場合
            if is_16_9 && (width > 1920 || height > 1080) {
                resize_filter = "-vf scale=1920:-2".to_string();
            }
        }
    }
    
    // FFmpegコマンドの実行
    let crf = crf.to_string();
    let mut command = Command::new("ffmpeg");
    command.args(&["-i", input_path]);
    match codec {
        // AV1: 圧縮率最優先。preset 5 は速度と効率のバランス（小さいほど高効率）。
        VideoCodec::Av1 => {
            command.args(&["-c:v", "libsvtav1", "-preset", "5", "-crf", &crf]);
        }
        // HEVC: hvc1 タグで iOS/QuickTime 再生互換。preset slow で圧縮効率を確保。
        VideoCodec::Hevc => {
            command.args(&["-c:v", "libx265", "-preset", "slow", "-crf", &crf, "-tag:v", "hvc1"]);
        }
    }

    // 幅広い再生互換のため 8bit 4:2:0 に固定
    command.args(&["-pix_fmt", "yuv420p"]);
    command.args(&["-c:a", "aac", "-b:a", "128k"]);

    // リサイズフィルターを追加（必要な場合）
    if !resize_filter.is_empty() {
        let filter_parts: Vec<&str> = resize_filter.split_whitespace().collect();
        command.args(filter_parts);
    }

    let status = command
        .args(&["-movflags", "+faststart"]) // ストリーミング向けに moov を先頭へ
        .arg("-y") // 確認なしで上書き
        .arg(output_file_path)
        .status()
        .map_err(|e| CompressError::Ffmpeg(format!("FFmpegの実行に失敗: {e}")))?;

    if !status.success() {
        return Err(CompressError::Ffmpeg(format!("FFmpegがエラーコードで終了: {status}")));
    }

    // 圧縮後のファイルサイズを取得
    let compressed_metadata = fs::metadata(output_path)?;
    let compressed_size = compressed_metadata.len();
    
    // 圧縮率の計算
    let size_reduction_percent = 100.0 * (1.0 - (compressed_size as f64 / original_size as f64));
    
    // 処理時間の計算
    let duration = start.elapsed();
    let duration_seconds = duration.as_secs_f64();
    
    Ok(CompressionStats {
        original_size,
        compressed_size,
        size_reduction_percent,
        duration_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CRFスケールはコーデック間で異なるため既定値を取り違えないこと
    #[test]
    fn default_crf_differs_per_codec() {
        assert_eq!(VideoCodec::Av1.default_crf(), 40);
        assert_eq!(VideoCodec::Hevc.default_crf(), 28);
    }

    /// 存在しないファイルは対象外として扱うこと
    #[test]
    fn missing_file_is_not_matched() {
        assert!(!is_match_extension("/nonexistent/clip.mp4"));
    }

    /// 圧縮失敗時にpanicせずErrを返すこと（バッチ処理を中断させないため）
    #[test]
    fn returns_error_instead_of_panicking() {
        let dir = std::env::temp_dir().join("compressor_video_test");
        fs::create_dir_all(&dir).unwrap();
        let broken = dir.join("broken.mp4");
        fs::write(&broken, b"not a real video").unwrap();
        let output = dir.join("out.mp4");

        let result = path2compress(
            broken.to_str().unwrap(),
            output.to_str().unwrap(),
            VideoCodec::Av1,
            None,
        );

        assert!(result.is_err(), "壊れた動画でErrにならなかった");
        let _ = fs::remove_dir_all(&dir);
    }
}
