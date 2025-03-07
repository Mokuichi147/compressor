use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use std::time::Instant;

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

pub fn path2compress(input_path: &str, output_path: &str) {
    let crf = "23";
    let preset = "";
    let _ = compress_video(input_path, output_path, crf, preset).unwrap();
}

pub fn is_match_extension(input_path: &str) -> bool {
    let path = Path::new(input_path);
    
    // 入力ファイルの存在チェック
    if !path.exists() {
        return false;
    }

    let video_extensions = [".mp4", ".avi", ".mov", ".mkv", ".wmv", ".flv", ".webm"];
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
/// # 引数
///
/// * `input_path` - 入力元の動画ファイルパス
/// * `output_path` - 圧縮後の出力先ファイルパス
/// * `crf` - Constant Rate Factor (0-51, 低いほど高画質)
/// * `preset` - エンコード速度プリセット (ultrafast, ..., veryslow)
///
/// # 戻り値
///
/// * `Result<CompressionStats, String>` - 成功時は圧縮統計情報、失敗時はエラーメッセージ
///
/// # 例
///
/// ```
/// let result = compress_video(
///     Path::new("/path/to/input.mp4"),
///     Path::new("/path/to/output.mp4"),
///     "23",
///     "medium"
/// );
/// match result {
///     Ok(stats) => println!("圧縮完了: {}% 削減", stats.size_reduction_percent),
///     Err(e) => eprintln!("エラー: {}", e),
/// }
/// ```
pub fn compress_video(
    input_path: &str,
    output_path: &str,
    crf: &str,
    preset: &str,
) -> Result<CompressionStats, String> {
    // 開始時間を記録
    let start = Instant::now();
    let output_file_path = PathBuf::from(output_path);

    // 出力ディレクトリの存在チェックと作成
    if let Some(parent) = output_file_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("出力ディレクトリの作成に失敗: {}", e))?;
        }
    }
    
    // 元のファイルサイズを取得
    let metadata = fs::metadata(input_path)
        .map_err(|e| format!("メタデータの取得に失敗: {}", e))?;
    let original_size = metadata.len();
    
    // FFmpegの存在チェック
    if !Command::new("ffmpeg").arg("-version").output().is_ok() {
        return Err("FFmpegがインストールされていないか、PATHに含まれていません".to_string());
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
        .map_err(|e| format!("ffprobeの実行に失敗: {}", e))?;
    
    let dimensions = String::from_utf8_lossy(&probe_output.stdout);
    let dimensions: Vec<&str> = dimensions.trim().split(',').collect();
    
    let mut resize_filter = String::new();
    
    // 解像度情報が正しく取得できた場合
    if dimensions.len() == 2 {
        if let (Ok(width), Ok(height)) = (dimensions[0].parse::<u32>(), dimensions[1].parse::<u32>()) {
            // アスペクト比を計算（小数点以下3桁まで）
            let aspect_ratio = (width as f64 / height as f64 * 1000.0).round() / 1000.0;
            
            // 16:9のアスペクト比は約1.778
            let is_16_9 = aspect_ratio >= 1.775 && aspect_ratio <= 1.781;
            
            // 16:9かつフルHD（1920x1080）を超える場合
            if is_16_9 && (width > 1920 || height > 1080) {
                resize_filter = "-vf scale=1920:-2".to_string();
            }
        }
    }
    
    // FFmpegコマンドの実行
    let mut command = Command::new("ffmpeg");
    command.arg("-i")
        .arg(input_path)
        .arg("-c:v")
        .arg("libx264")
        .arg("-crf")
        .arg(crf)
        .arg("-preset")
        .arg(preset)
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("128k");
    
    // リサイズフィルターを追加（必要な場合）
    if !resize_filter.is_empty() {
        let filter_parts: Vec<&str> = resize_filter.split_whitespace().collect();
        for part in filter_parts {
            command.arg(part);
        }
    }
    
    let status = command
        .arg("-y") // 確認なしで上書き
        .arg(output_file_path)
        .status()
        .map_err(|e| format!("FFmpegの実行に失敗: {}", e))?;
    
    if !status.success() {
        return Err(format!("FFmpegがエラーコードで終了: {}", status));
    }
    
    // 圧縮後のファイルサイズを取得
    let compressed_metadata = fs::metadata(output_path)
        .map_err(|e| format!("圧縮ファイルのメタデータ取得に失敗: {}", e))?;
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
