use std::process::Command;
use std::sync::OnceLock;

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
