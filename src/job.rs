use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::audio::{self, AudioCodec};
use crate::error::CompressError;
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
    pub hevc: bool,
    pub crf: Option<u8>,
    pub opus: bool,
    pub audio_bitrate: String,
}

impl Settings {
    /// 動画の出力コーデック
    fn video_codec(&self) -> VideoCodec {
        if self.hevc {
            VideoCodec::Hevc
        } else {
            VideoCodec::Av1
        }
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
    RgbImage { quality: f32 },
    /// png を oxipng で最適化する
    RgbaImage,
    /// 非可逆WebPに変換する
    WebpLossy { quality: f32 },
    /// 可逆WebPに変換する
    WebpLossless,
    /// 静止GIFをPNG化する
    GifToPng,
    Video { codec: VideoCodec, crf: Option<u8> },
    Audio { codec: AudioCodec, bitrate: String },
}

impl Action {
    /// 出力ファイルの拡張子
    pub fn extension(&self) -> &'static str {
        match self {
            Action::RgbImage { .. } => "jpg",
            Action::RgbaImage | Action::GifToPng => "png",
            Action::WebpLossy { .. } | Action::WebpLossless => "webp",
            Action::Video { .. } => "mp4",
            Action::Audio { codec, .. } => codec.extension(),
        }
    }

    /// ログに出す処理名
    pub fn label(&self) -> String {
        match self {
            Action::RgbImage { .. } => "rgb image".to_string(),
            Action::RgbaImage => "rgba image".to_string(),
            Action::WebpLossy { .. } => "webp (lossy)".to_string(),
            Action::WebpLossless => "webp (lossless)".to_string(),
            Action::GifToPng => "gif -> png".to_string(),
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
    pub fn run(&self) -> Result<(), CompressError> {
        match &self.action {
            Action::RgbImage { quality } => {
                rgb_image::path2compress(&self.source, &self.target, *quality)
            }
            Action::RgbaImage => rgba_image::path2compress(&self.source, &self.target),
            Action::WebpLossy { quality } => {
                webp_image::path2compress_lossy(&self.source, &self.target, *quality)
            }
            Action::WebpLossless => {
                webp_image::path2compress_lossless(&self.source, &self.target)
            }
            Action::GifToPng => gif_image::path2compress_png(&self.source, &self.target),
            Action::Video { codec, crf } => video::path2compress(
                &self.source.to_string_lossy(),
                &self.target.to_string_lossy(),
                *codec,
                *crf,
            )
            .map(|_| ()),
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
            if settings.webp {
                Action::WebpLossless
            } else {
                Action::RgbaImage
            }
        }
        "jpg" | "jpeg" => {
            if settings.webp {
                Action::WebpLossy {
                    quality: settings.quality,
                }
            } else {
                Action::RgbImage {
                    quality: settings.quality,
                }
            }
        }
        // GIFは内容で振り分ける。アニメーションGIFは動画として扱うため `--webp` の対象外。
        "gif" => {
            if gif_image::is_animated(source)? {
                Action::Video {
                    codec: settings.video_codec(),
                    crf: settings.crf,
                }
            } else if settings.webp {
                Action::WebpLossless
            } else {
                Action::GifToPng
            }
        }
        _ => {
            let path = source.to_string_lossy();
            if video::is_match_extension(&path) {
                Action::Video {
                    codec: settings.video_codec(),
                    crf: settings.crf,
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
            hevc: false,
            crf: None,
            opus: false,
            audio_bitrate: "128k".to_string(),
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
