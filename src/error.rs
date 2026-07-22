use std::fmt;

/// 公開APIの圧縮処理で発生しうるエラー。
#[derive(Debug)]
pub enum CompressError {
    /// 入力画像のデコードに失敗した（対応していないフォーマット・壊れたファイルなど、恒久的な失敗）。
    Image(image::ImageError),
    /// ファイルの読み書きに失敗した（権限不足・ディスク不足など、リトライ可能な失敗を含む）。
    Io(std::io::Error),
    /// oxipngによるPNG最適化に失敗した。
    Png(oxipng::PngError),
    /// ffmpegの実行に失敗した（未インストール、エンコードエラーなど）。
    Ffmpeg(String),
}

impl fmt::Display for CompressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompressError::Image(e) => write!(f, "image decode error: {e}"),
            CompressError::Io(e) => write!(f, "io error: {e}"),
            CompressError::Png(e) => write!(f, "png optimize error: {e}"),
            CompressError::Ffmpeg(e) => write!(f, "ffmpeg error: {e}"),
        }
    }
}

impl std::error::Error for CompressError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CompressError::Image(e) => Some(e),
            CompressError::Io(e) => Some(e),
            CompressError::Png(e) => Some(e),
            CompressError::Ffmpeg(_) => None,
        }
    }
}

impl From<image::ImageError> for CompressError {
    fn from(e: image::ImageError) -> Self {
        CompressError::Image(e)
    }
}

impl From<std::io::Error> for CompressError {
    fn from(e: std::io::Error) -> Self {
        CompressError::Io(e)
    }
}

impl From<oxipng::PngError> for CompressError {
    fn from(e: oxipng::PngError) -> Self {
        CompressError::Png(e)
    }
}
