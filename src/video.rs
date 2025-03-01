use std::path::Path;
use std::process::Command;
use std::fs;
use std::io;
use std::time::Instant;

/// 動画圧縮の結果統計情報
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
    input_path: &Path,
    output_path: &Path,
    crf: &str,
    preset: &str,
) -> Result<CompressionStats, String> {
    // 開始時間を記録
    let start = Instant::now();
    
    // 入力ファイルの存在チェック
    if !input_path.exists() {
        return Err(format!("入力ファイルが存在しません: {}", input_path.display()));
    }
    
    // 入力ファイルが動画かどうかの簡易チェック
    let video_extensions = [".mp4", ".avi", ".mov", ".mkv", ".wmv", ".flv", ".webm"];
    let extension = input_path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{}", ext.to_lowercase()));
    
    match extension {
        Some(ext) if video_extensions.contains(&ext.as_str()) => {},
        _ => return Err(format!("サポートされていないファイル形式: {}", input_path.display())),
    }
    
    // 出力ディレクトリの存在チェックと作成
    if let Some(parent) = output_path.parent() {
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
            let is_16_9 = (aspect_ratio >= 1.775 && aspect_ratio <= 1.781);
            
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
        .arg(output_path)
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

/// 動画圧縮関数のオプション指定バージョン
///
/// # 引数
///
/// * `input_path` - 入力元の動画ファイルパス
/// * `output_path` - 圧縮後の出力先ファイルパス
/// * `options` - カスタムFFmpegオプション
///
/// # 戻り値
///
/// * `Result<CompressionStats, String>` - 成功時は圧縮統計情報、失敗時はエラーメッセージ
pub fn compress_video_with_options(
    input_path: &Path,
    output_path: &Path,
    options: &CompressionOptions,
) -> Result<CompressionStats, String> {
    // 開始時間を記録
    let start = Instant::now();
    
    // 入力ファイルの存在チェック
    if !input_path.exists() {
        return Err(format!("入力ファイルが存在しません: {}", input_path.display()));
    }
    
    // 出力ディレクトリの存在チェックと作成
    if let Some(parent) = output_path.parent() {
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
    
    // 解像度自動調整が有効で、カスタム解像度が指定されていない場合
    let mut auto_resize = false;
    if options.auto_resize_to_fullhd && options.resolution.is_none() {
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
        
        // 解像度情報が正しく取得できた場合
        if dimensions.len() == 2 {
            if let (Ok(width), Ok(height)) = (dimensions[0].parse::<u32>(), dimensions[1].parse::<u32>()) {
                // アスペクト比を計算（小数点以下3桁まで）
                let aspect_ratio = (width as f64 / height as f64 * 1000.0).round() / 1000.0;
                
                // 16:9のアスペクト比は約1.778
                let is_16_9 = (aspect_ratio >= 1.775 && aspect_ratio <= 1.781);
                
                // 16:9かつフルHD（1920x1080）を超える場合
                if is_16_9 && (width > 1920 || height > 1080) {
                    auto_resize = true;
                }
            }
        }
    }
    
    // FFmpegコマンドの構築
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-i").arg(input_path);
    
    // ビデオコーデックの設定
    cmd.arg("-c:v").arg(&options.video_codec);
    
    // CRFの設定
    if let Some(crf) = &options.crf {
        cmd.arg("-crf").arg(crf);
    }
    
    // プリセットの設定
    if let Some(preset) = &options.preset {
        cmd.arg("-preset").arg(preset);
    }
    
    // ビデオビットレートの設定
    if let Some(bitrate) = &options.video_bitrate {
        cmd.arg("-b:v").arg(bitrate);
    }
    
    // 解像度の設定（自動リサイズが有効なら、フィルターを使用）
    if auto_resize {
        cmd.arg("-vf").arg("scale=1920:-2");
    } else if let Some(resolution) = &options.resolution {
        cmd.arg("-s").arg(resolution);
    }
    
    // オーディオコーデックの設定
    if let Some(audio_codec) = &options.audio_codec {
        cmd.arg("-c:a").arg(audio_codec);
    }
    
    // オーディオビットレートの設定
    if let Some(audio_bitrate) = &options.audio_bitrate {
        cmd.arg("-b:a").arg(audio_bitrate);
    }
    
    // カスタムオプションの追加
    for (key, value) in &options.custom_options {
        cmd.arg(key);
        if !value.is_empty() {
            cmd.arg(value);
        }
    }
    
    // 確認なしで上書き
    cmd.arg("-y").arg(output_path);
    
    // コマンド実行
    let status = cmd.status()
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

/// 圧縮オプションを表す構造体
#[derive(Debug, Clone)]
pub struct CompressionOptions {
    /// ビデオコーデック (例: "libx264", "libx265")
    pub video_codec: String,
    /// Constant Rate Factor (0-51)
    pub crf: Option<String>,
    /// エンコード速度プリセット
    pub preset: Option<String>,
    /// ビデオビットレート (例: "1M")
    pub video_bitrate: Option<String>,
    /// 解像度 (例: "1280x720")
    pub resolution: Option<String>,
    /// オーディオコーデック (例: "aac")
    pub audio_codec: Option<String>,
    /// オーディオビットレート (例: "128k")
    pub audio_bitrate: Option<String>,
    /// カスタムFFmpegオプション
    pub custom_options: Vec<(String, String)>,
    /// 16:9の解像度がフルHDを超える場合に自動でフルHDにリサイズするかどうか
    pub auto_resize_to_fullhd: bool,
}

impl Default for CompressionOptions {
    fn default() -> Self {
        CompressionOptions {
            video_codec: "libx264".to_string(),
            crf: Some("23".to_string()),
            preset: Some("medium".to_string()),
            video_bitrate: None,
            resolution: None,
            audio_codec: Some("aac".to_string()),
            audio_bitrate: Some("128k".to_string()),
            custom_options: Vec::new(),
            auto_resize_to_fullhd: true, // デフォルトで有効
        }
    }
}

impl CompressionOptions {
    /// 新しい圧縮オプションを作成
    pub fn new() -> Self {
        Default::default()
    }
    
    /// CRFを設定
    pub fn crf(mut self, crf: &str) -> Self {
        self.crf = Some(crf.to_string());
        self
    }
    
    /// プリセットを設定
    pub fn preset(mut self, preset: &str) -> Self {
        self.preset = Some(preset.to_string());
        self
    }
    
    /// ビデオコーデックを設定
    pub fn video_codec(mut self, codec: &str) -> Self {
        self.video_codec = codec.to_string();
        self
    }
    
    /// ビデオビットレートを設定
    pub fn video_bitrate(mut self, bitrate: &str) -> Self {
        self.video_bitrate = Some(bitrate.to_string());
        self
    }
    
    /// 解像度を設定
    pub fn resolution(mut self, resolution: &str) -> Self {
        self.resolution = Some(resolution.to_string());
        self
    }
    
    /// オーディオコーデックを設定
    pub fn audio_codec(mut self, codec: &str) -> Self {
        self.audio_codec = Some(codec.to_string());
        self
    }
    
    /// オーディオビットレートを設定
    pub fn audio_bitrate(mut self, bitrate: &str) -> Self {
        self.audio_bitrate = Some(bitrate.to_string());
        self
    }
    
    /// 自動フルHDリサイズ機能の設定
    pub fn auto_resize_to_fullhd(mut self, enable: bool) -> Self {
        self.auto_resize_to_fullhd = enable;
        self
    }
    
    /// カスタムオプションを追加
    pub fn add_custom_option(mut self, key: &str, value: &str) -> Self {
        self.custom_options.push((key.to_string(), value.to_string()));
        self
    }
}