use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use std::time::Instant;
use crate::error::CompressError;
use crate::stats::CompressionStats;
use crate::utilities::{
    capped_bitrate, copy_modified_time, is_ffmpeg_available, probe_audio_stream,
    replace_with_original_if_larger, same_extension, scaled_size,
};

/// 動画に載せる音声の目標ビットレート。元がこれ以下ならそのままコピーする。
const AUDIO_BITRATE: &str = "128k";

/// 対応する動画の拡張子。
///
/// `.ts`（MPEG-TS）はあえて含めていない。TypeScript のソースと拡張子が衝突し、
/// フォルダを再帰的に走査するこのツールでは誤検出が実害になるため。
/// MPEG-TS は `.m2ts` / `.mts` で拾える。
const VIDEO_EXTENSIONS: [&str; 14] = [
    "mov", "mp4", "m4v", "avi", "mkv", "webm", "wmv", "flv", "mpg", "mpeg", "m2ts", "mts", "3gp",
    "3g2",
];

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
    max_long_side: u32,
) -> Result<CompressionStats, CompressError> {
    let crf = crf.unwrap_or_else(|| codec.default_crf());
    compress_video(input_path, output_path, codec, crf, max_long_side)
}

pub fn is_match_extension(input_path: &str) -> bool {
    let path = Path::new(input_path);
    
    // 入力ファイルの存在チェック
    if !path.exists() {
        return false;
    }

    let extension = path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase());

    match extension {
        Some(ext) => VIDEO_EXTENSIONS.contains(&ext.as_str()),
        None => false,
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
/// * `max_long_side` - 長辺の上限（ピクセル）。超える場合は縮小する。0 なら縮小しない
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
///     1920,
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
    max_long_side: u32,
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

    // 縮小後のサイズ。解像度が取れない場合や上限以下の場合は縮小しない
    let mut scale_to = None;
    if dimensions.len() == 2 {
        if let (Ok(width), Ok(height)) = (dimensions[0].parse::<u32>(), dimensions[1].parse::<u32>()) {
            scale_to = scaled_size(width, height, max_long_side);
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
    if let Some((width, height)) = scale_to {
        command.args(&["-vf", &format!("scale={width}:{height}")]);
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

    /// 対応拡張子すべてが（大文字でも）動画として判定されること
    #[test]
    fn matches_all_video_extensions() {
        let dir = std::env::temp_dir().join("compressor_video_extensions");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        for ext in VIDEO_EXTENSIONS {
            let path = dir.join(format!("clip.{ext}"));
            fs::write(&path, b"").unwrap();
            assert!(is_match_extension(path.to_str().unwrap()), "{ext} が動画と判定されない");

            let upper = dir.join(format!("upper.{}", ext.to_uppercase()));
            fs::write(&upper, b"").unwrap();
            assert!(
                is_match_extension(upper.to_str().unwrap()),
                "{ext} が大文字だと判定されない"
            );
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// TypeScript のソースと衝突するため .ts は対象にしないこと
    #[test]
    fn typescript_source_is_not_matched() {
        let dir = std::env::temp_dir().join("compressor_video_ts");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let path = dir.join("index.ts");
        fs::write(&path, b"export const x = 1;").unwrap();
        assert!(!is_match_extension(path.to_str().unwrap()));

        let _ = fs::remove_dir_all(&dir);
    }

    /// 音声のみのコンテナを動画として拾わないこと
    #[test]
    fn audio_only_containers_are_not_matched() {
        let dir = std::env::temp_dir().join("compressor_video_audio_only");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        for name in ["song.m4a", "song.mka", "song.mp3"] {
            let path = dir.join(name);
            fs::write(&path, b"").unwrap();
            assert!(!is_match_extension(path.to_str().unwrap()), "{name} が動画と判定された");
        }

        let _ = fs::remove_dir_all(&dir);
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
            1920,
        );

        assert!(result.is_err(), "壊れた動画でErrにならなかった");
        let _ = fs::remove_dir_all(&dir);
    }
}
