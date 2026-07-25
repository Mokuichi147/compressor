use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use std::time::Instant;
use crate::error::CompressError;
use crate::stats::CompressionStats;
use crate::utilities::{
    capped_bitrate, copy_modified_time, get_aspect_ratio, is_ffmpeg_available,
    probe_audio_stream, replace_with_original_if_larger, same_extension,
};

/// 動画に載せる音声の目標ビットレート。元がこれ以下ならそのままコピーする。
const AUDIO_BITRATE: &str = "128k";

/// 動画の出力コーデック
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    /// AV1 (libsvtav1)。最も圧縮率が高い。既定。
    Av1,
    /// HEVC/H.265 (libx265, hvc1 タグ)。iOS など旧来デバイスでの再生互換性が高い。
    Hevc,
}

impl VideoCodec {
    /// ログ表示に使うコーデック名
    pub fn name(self) -> &'static str {
        match self {
            VideoCodec::Av1 => "av1",
            VideoCodec::Hevc => "hevc",
        }
    }

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
///     Ok(stats) => println!("圧縮完了: {}% 削減", stats.size_reduction_percent()),
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

    // 音声トラックの扱いを決める。
    // 元が既に AAC で目標ビットレート以下なら、再エンコードせずコピーして世代劣化を避ける。
    // それ以外は AAC に再エンコードするが、元より高いビットレートは指定しない。
    let audio_info = probe_audio_stream(Path::new(input_path));
    match &audio_info {
        Some(info)
            if info.codec_name == "aac"
                && info.bitrate_bps.is_some_and(|bps| bps <= 128_000) =>
        {
            command.args(&["-c:a", "copy"]);
        }
        Some(info) => {
            let bitrate = capped_bitrate(AUDIO_BITRATE, info.bitrate_bps);
            command.args(&["-c:a", "aac", "-b:a", &bitrate]);
        }
        None => {
            command.args(&["-c:a", "aac", "-b:a", AUDIO_BITRATE]);
        }
    }

    // リサイズフィルターを追加（必要な場合）
    if !resize_filter.is_empty() {
        let filter_parts: Vec<&str> = resize_filter.split_whitespace().collect();
        command.args(filter_parts);
    }

    // 撮影日時・GPS・カメラ情報を引き継ぐ。
    // use_metadata_tags を付けないと com.apple.quicktime.* が落ち、
    // iPhone で撮った動画から撮影日時と位置情報が失われる。
    // （コンテナのブランドは isom のまま。ffprobe に qt と出るのはコピーされたタグの側）
    command.args(&["-map_metadata", "0"]);

    let status = command
        // faststart はストリーミング向けに moov を先頭へ移す。
        // -movflags は後勝ちで上書きされるため、まとめて1回で指定する。
        .args(&["-movflags", "+faststart+use_metadata_tags"])
        .arg("-y") // 確認なしで上書き
        .arg(&output_file_path)
        .status()
        .map_err(|e| CompressError::Ffmpeg(format!("FFmpegの実行に失敗: {e}")))?;

    if !status.success() {
        return Err(CompressError::Ffmpeg(format!("FFmpegがエラーコードで終了: {status}")));
    }

    // 既に十分圧縮された動画を再エンコードすると、サイズが増えたうえに画質だけ落ちることがある。
    // 形式が変わらない場合（mp4→mp4）に限り、元のほうが小さければ元を出力する。
    if same_extension(Path::new(input_path), &output_file_path) {
        replace_with_original_if_larger(Path::new(input_path), &output_file_path)?;
    }

    // 元ファイルで置き換えた場合も更新日時を揃えたいので、コピーの後に行う
    copy_modified_time(Path::new(input_path), &output_file_path)?;

    CompressionStats::measure(Path::new(input_path), &output_file_path, start)
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
