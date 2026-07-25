use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::audio::{self, AudioCodec};
use crate::avif_image;
use crate::error::CompressError;
use crate::stats::CompressionStats;
use crate::file;
use crate::gif_image;
use crate::rgb_image;
use crate::rgba_image;
use crate::video::{self, VideoCodec};
use crate::webp_image;

/// 圧縮の設定。CLI引数のうち、ジョブの決定と実行に必要なものだけを持つ。
pub struct Settings {
    pub quality: f32,
    pub webp: bool,
    pub avif: bool,
    pub hevc: bool,
    pub crf: Option<u8>,
    pub opus: bool,
    pub audio_bitrate: String,
    /// 長辺の上限。`None` は「画像は縮小せず、動画だけ既定値に収める」を意味する
    pub max_long_side: Option<u32>,
}

/// `--max-long-side` 未指定時に動画へ適用する長辺の上限。
/// 従来から動画だけは暗黙に 1920 へ収めていたため、その挙動を保つ。
const DEFAULT_VIDEO_MAX_LONG_SIDE: u32 = 1920;

impl Settings {
    /// 動画の出力コーデック
    fn video_codec(&self) -> VideoCodec {
        if self.hevc {
            VideoCodec::Hevc
        } else {
            VideoCodec::Av1
        }
    }

    /// 動画に適用する長辺の上限。未指定でも動画だけは既定値に収める。
    fn video_max_long_side(&self) -> u32 {
        self.max_long_side.unwrap_or(DEFAULT_VIDEO_MAX_LONG_SIDE)
    }

    /// 音声の出力コーデック。可逆音源はFLAC、非可逆音源はAAC（`--opus` 指定時はOpus）。
    fn audio_codec(&self, source: &Path) -> AudioCodec {
        if audio::is_lossless_source(&source.to_string_lossy()) {
            AudioCodec::Flac
        } else if self.opus {
            AudioCodec::Opus
        } else {
            AudioCodec::Aac
        }
    }
}

/// 入力ファイルに対して実行する圧縮処理。
pub enum Action {
    /// jpg/jpeg を mozjpeg で再圧縮する
    RgbImage { quality: f32, max_long_side: Option<u32> },
    /// png を oxipng で最適化する
    RgbaImage { max_long_side: Option<u32> },
    /// 非可逆WebPに変換する
    WebpLossy { quality: f32, max_long_side: Option<u32> },
    /// 可逆WebPに変換する
    WebpLossless { max_long_side: Option<u32> },
    /// 非可逆AVIFに変換する（jpg/jpeg 向け）
    AvifLossy { quality: f32, max_long_side: Option<u32> },
    /// アルファを保った非可逆AVIFに変換する（png 向け）。AVIFは可逆にできない
    AvifLossyRgba { quality: f32, max_long_side: Option<u32> },
    /// GIF・TIFF・BMP をデコードし直してPNG化する
    DecodeToPng { max_long_side: Option<u32> },
    Video { codec: VideoCodec, crf: Option<u8>, max_long_side: u32 },
    Audio { codec: AudioCodec, bitrate: String },
}

impl Action {
    /// 出力ファイルの拡張子
    pub fn extension(&self) -> &'static str {
        match self {
            Action::RgbImage { .. } => "jpg",
            Action::RgbaImage { .. } | Action::DecodeToPng { .. } => "png",
            Action::WebpLossy { .. } | Action::WebpLossless { .. } => "webp",
            Action::AvifLossy { .. } | Action::AvifLossyRgba { .. } => "avif",
            Action::Video { .. } => "mp4",
            Action::Audio { codec, .. } => codec.extension(),
        }
    }

    /// ffmpeg を起動する処理かどうか。
    /// ffmpeg が無い環境で1件ずつ同じ理由で失敗させないため、実行前の判定に使う。
    pub fn needs_ffmpeg(&self) -> bool {
        matches!(self, Action::Video { .. } | Action::Audio { .. })
    }

    /// ログに出す処理名
    pub fn label(&self) -> String {
        match self {
            Action::RgbImage { .. } => "rgb image".to_string(),
            Action::RgbaImage { .. } => "rgba image".to_string(),
            Action::WebpLossy { .. } => "webp (lossy)".to_string(),
            Action::WebpLossless { .. } => "webp (lossless)".to_string(),
            Action::AvifLossy { .. } => "avif (lossy)".to_string(),
            Action::AvifLossyRgba { .. } => "avif (lossy, alpha)".to_string(),
            Action::DecodeToPng { .. } => "image -> png".to_string(),
            Action::Video { codec, .. } => format!("video ({})", codec.name()),
            Action::Audio { codec, .. } => format!("audio ({})", codec.extension()),
        }
    }
}

/// 1ファイル分の圧縮ジョブ。「どこから」「どこへ」「どう圧縮するか」が決まった状態。
pub struct Job {
    pub source: PathBuf,
    pub target: PathBuf,
    pub action: Action,
}

impl Job {
    pub fn run(&self) -> Result<CompressionStats, CompressError> {
        match &self.action {
            Action::RgbImage { quality, max_long_side } => {
                rgb_image::path2compress(&self.source, &self.target, *quality, *max_long_side)
            }
            Action::RgbaImage { max_long_side } => {
                rgba_image::path2compress(&self.source, &self.target, *max_long_side)
            }
            Action::WebpLossy { quality, max_long_side } => webp_image::path2compress_lossy(
                &self.source,
                &self.target,
                *quality,
                *max_long_side,
            ),
            Action::WebpLossless { max_long_side } => {
                webp_image::path2compress_lossless(&self.source, &self.target, *max_long_side)
            }
            Action::AvifLossy { quality, max_long_side } => avif_image::path2compress_lossy(
                &self.source,
                &self.target,
                *quality,
                *max_long_side,
            ),
            Action::AvifLossyRgba { quality, max_long_side } => {
                avif_image::path2compress_lossy_rgba(
                    &self.source,
                    &self.target,
                    *quality,
                    *max_long_side,
                )
            }
            Action::DecodeToPng { max_long_side } => {
                rgba_image::decode2compress_png(&self.source, &self.target, *max_long_side)
            }
            Action::Video { codec, crf, max_long_side } => video::path2compress(
                &self.source.to_string_lossy(),
                &self.target.to_string_lossy(),
                *codec,
                *crf,
                *max_long_side,
            ),
            Action::Audio { codec, bitrate } => {
                audio::path2compress(&self.source, &self.target, *codec, bitrate)
            }
        }
    }
}

/// 入力ファイルに対する圧縮ジョブを決める。対象外の形式なら `None` を返す。
///
/// 出力先の決定（拡張子の置き換えと衝突回避）もここで行うため、
/// 実行するかどうか（既に出力が存在するか）の判定は呼び出し側でジョブを見て決められる。
pub fn plan(
    source: &Path,
    output_base: &Path,
    settings: &Settings,
    used: &mut HashSet<PathBuf>,
) -> Result<Option<Job>, CompressError> {
    let Some(action) = decide_action(source, settings)? else {
        return Ok(None);
    };

    let target = file::unique_target(&output_base.to_path_buf(), action.extension(), used);

    Ok(Some(Job {
        source: source.to_path_buf(),
        target,
        action,
    }))
}

fn decide_action(source: &Path, settings: &Settings) -> Result<Option<Action>, CompressError> {
    let Some(ext) = source.extension() else {
        return Ok(None);
    };
    let ext = ext.to_string_lossy().to_lowercase();

    let action = match ext.as_str() {
        "png" => {
            if settings.avif {
                Action::AvifLossyRgba {
                    quality: settings.quality,
                    max_long_side: settings.max_long_side,
                }
            } else if settings.webp {
                Action::WebpLossless { max_long_side: settings.max_long_side }
            } else {
                Action::RgbaImage { max_long_side: settings.max_long_side }
            }
        }
        "jpg" | "jpeg" => {
            if settings.avif {
                Action::AvifLossy {
                    quality: settings.quality,
                    max_long_side: settings.max_long_side,
                }
            } else if settings.webp {
                Action::WebpLossy {
                    quality: settings.quality,
                    max_long_side: settings.max_long_side,
                }
            } else {
                Action::RgbImage {
                    quality: settings.quality,
                    max_long_side: settings.max_long_side,
                }
            }
        }
        // GIFは内容で振り分ける。アニメーションGIFは動画として扱うため `--webp` の対象外。
        "gif" => {
            if gif_image::is_animated(source)? {
                Action::Video {
                    codec: settings.video_codec(),
                    crf: settings.crf,
                    max_long_side: settings.video_max_long_side(),
                }
            } else if settings.avif {
                Action::AvifLossyRgba {
                    quality: settings.quality,
                    max_long_side: settings.max_long_side,
                }
            } else if settings.webp {
                Action::WebpLossless { max_long_side: settings.max_long_side }
            } else {
                Action::DecodeToPng { max_long_side: settings.max_long_side }
            }
        }
        // 可逆だが圧縮が弱い（もしくは無圧縮の）画像。GIFと同様にPNG化する
        "tiff" | "tif" | "bmp" => {
            if settings.avif {
                Action::AvifLossyRgba {
                    quality: settings.quality,
                    max_long_side: settings.max_long_side,
                }
            } else if settings.webp {
                Action::WebpLossless { max_long_side: settings.max_long_side }
            } else {
                Action::DecodeToPng { max_long_side: settings.max_long_side }
            }
        }
        _ => {
            let path = source.to_string_lossy();
            if video::is_match_extension(&path) {
                Action::Video {
                    codec: settings.video_codec(),
                    crf: settings.crf,
                    max_long_side: settings.video_max_long_side(),
                }
            } else if audio::is_match_extension(&path) {
                Action::Audio {
                    codec: settings.audio_codec(source),
                    bitrate: settings.audio_bitrate.clone(),
                }
            } else {
                return Ok(None);
            }
        }
    };

    Ok(Some(action))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn settings() -> Settings {
        Settings {
            quality: 70.0,
            webp: false,
            avif: false,
            hevc: false,
            crf: None,
            opus: false,
            audio_bitrate: "128k".to_string(),
            max_long_side: None,
        }
    }

    /// 動画・音声の判定は実ファイルの存在を見るため、テスト用に空ファイルを作る
    fn touch(dir: &Path, name: &str) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, b"").unwrap();
        path
    }

    fn plan_for(source: &Path, settings: &Settings) -> Option<Job> {
        let mut used = HashSet::new();
        let base = PathBuf::from("compress").join(source.file_name().unwrap());
        plan(source, &base, settings, &mut used).unwrap()
    }

    /// 拡張子ごとに想定どおりの出力拡張子になること
    #[test]
    fn maps_extension_to_output() {
        let dir = std::env::temp_dir().join("compressor_job_ext");
        let _ = fs::remove_dir_all(&dir);

        let cases = [
            ("a.jpg", "jpg"),
            ("a.jpeg", "jpg"),
            ("a.png", "png"),
            ("a.mov", "mp4"),
            ("a.mkv", "mp4"),
            ("a.wav", "flac"),
            ("a.mp3", "m4a"),
        ];
        for (name, expected) in cases {
            let source = touch(&dir, name);
            let job = plan_for(&source, &settings()).expect(name);
            assert_eq!(job.action.extension(), expected, "{name} の出力拡張子");
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// --webp 指定時は画像だけがWebPになり、動画・音声は影響を受けないこと
    #[test]
    fn webp_only_affects_images() {
        let dir = std::env::temp_dir().join("compressor_job_webp");
        let _ = fs::remove_dir_all(&dir);
        let mut settings = settings();
        settings.webp = true;

        for name in ["a.jpg", "a.png"] {
            let source = touch(&dir, name);
            let job = plan_for(&source, &settings).expect(name);
            assert_eq!(job.action.extension(), "webp", "{name} がWebPにならない");
        }
        for (name, expected) in [("a.mov", "mp4"), ("a.mp3", "m4a")] {
            let source = touch(&dir, name);
            let job = plan_for(&source, &settings).expect(name);
            assert_eq!(job.action.extension(), expected, "{name} がWebPの影響を受けた");
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// ffmpegを要する処理だけが needs_ffmpeg になること。
    /// 取り違えると、ffmpegが無い環境で画像まで飛ばしてしまう
    #[test]
    fn only_ffmpeg_actions_need_ffmpeg() {
        let dir = std::env::temp_dir().join("compressor_job_needs_ffmpeg");
        let _ = fs::remove_dir_all(&dir);

        for (name, expected) in [
            ("a.mp4", true),
            ("a.mp3", true),
            ("a.wav", true),
            ("a.jpg", false),
            ("a.png", false),
        ] {
            let source = touch(&dir, name);
            let job = plan_for(&source, &settings()).expect(name);
            assert_eq!(job.action.needs_ffmpeg(), expected, "{name} の判定");
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// 対象外の拡張子・拡張子なしはジョブを作らないこと
    #[test]
    fn unsupported_yields_no_job() {
        let dir = std::env::temp_dir().join("compressor_job_unsupported");
        let _ = fs::remove_dir_all(&dir);

        for name in ["a.txt", "a.pdf", "README"] {
            let source = touch(&dir, name);
            assert!(plan_for(&source, &settings()).is_none(), "{name} がジョブになった");
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// tiff / bmp は GIF と同じくPNG化し、--webp なら可逆WebPになること
    #[test]
    fn tiff_and_bmp_become_png() {
        let dir = std::env::temp_dir().join("compressor_job_tiff_bmp");
        let _ = fs::remove_dir_all(&dir);

        let mut webp = settings();
        webp.webp = true;

        for name in ["a.tiff", "a.tif", "a.bmp"] {
            let source = touch(&dir, name);
            assert_eq!(
                plan_for(&source, &settings()).expect(name).action.extension(),
                "png",
                "{name} がPNGにならない"
            );
            assert_eq!(
                plan_for(&source, &webp).expect(name).action.extension(),
                "webp",
                "{name} が可逆WebPにならない"
            );
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// 静止GIFは画像として扱うこと（アニメーションGIFは動画）
    #[test]
    fn static_gif_is_treated_as_image() {
        let dir = std::env::temp_dir().join("compressor_job_gif");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let source = dir.join("a.gif");
        let img = image::RgbaImage::new(2, 2);
        image::DynamicImage::ImageRgba8(img)
            .save_with_format(&source, image::ImageFormat::Gif)
            .unwrap();

        assert_eq!(plan_for(&source, &settings()).unwrap().action.extension(), "png");

        let mut webp = settings();
        webp.webp = true;
        assert_eq!(plan_for(&source, &webp).unwrap().action.extension(), "webp");

        let _ = fs::remove_dir_all(&dir);
    }

    /// 出力先が衝突する入力でも、別々の出力先が割り当てられること
    #[test]
    fn colliding_sources_get_distinct_targets() {
        let dir = std::env::temp_dir().join("compressor_job_collision");
        let _ = fs::remove_dir_all(&dir);

        let mut used = HashSet::new();
        let mut targets = Vec::new();
        for name in ["song.mp3", "song.m4a"] {
            let source = touch(&dir, name);
            let base = PathBuf::from("compress").join(name);
            let job = plan(&source, &base, &settings(), &mut used).unwrap().unwrap();
            targets.push(job.target);
        }

        assert_eq!(targets[0], PathBuf::from("compress/song.m4a"));
        assert_eq!(targets[1], PathBuf::from("compress/song.m4a.m4a"));

        let _ = fs::remove_dir_all(&dir);
    }
}
