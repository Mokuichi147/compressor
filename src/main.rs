use std::{collections::HashSet, fs, path::PathBuf};
use clap::Parser;
mod file;
mod utilities;
mod error;
mod rgb_image;
mod rgba_image;
mod webp_image;
mod video;
mod audio;

#[derive(Parser)]
struct AppArgs {
    /// 圧縮済みファイルの保存先
    #[clap(short, long, default_value = "compress")]
    output_dir: String,
    
    /// 圧縮したいファイル（入力のない場合は全て）
    #[clap(short, long, num_args = 1..)]
    input_file: Option<Vec<PathBuf>>,

    /// RGB画像の圧縮率
    #[clap(short, long, default_value="70.0")]
    quality: f32,

    /// 圧縮済みファイルを上書きして再圧縮するか
    #[clap(short, long)]
    force: bool,

    /// 画像をWebPで出力する（jpg/jpeg→非可逆, png→可逆）
    #[clap(short, long)]
    webp: bool,

    /// 動画をHEVC(H.265)で出力する（既定はAV1。HEVCは旧来デバイスでの再生互換性が高い）
    #[clap(long)]
    hevc: bool,

    /// 動画の品質 (CRF)。低いほど高品質・大きいファイル。未指定時はコーデックごとの既定値
    #[clap(long)]
    crf: Option<u8>,

    /// 音声をOpusで出力する（既定はAAC）。非可逆圧縮時のみ有効
    #[clap(long)]
    opus: bool,

    /// 音声の非可逆圧縮時のビットレート
    #[clap(long, default_value = "128k")]
    audio_bitrate: String,
}

fn main() {
    let args = AppArgs::parse();

    let mut input_files = args.input_file.unwrap_or_default();
    if input_files.len() == 0 {
        input_files = file::get_files(".");
    }

    std::fs::create_dir_all(&args.output_dir).unwrap();
    let root_dir = PathBuf::from(".");

    // 生成済みの出力先を記録し、同名衝突を回避する。
    // 出力の拡張子は入力より種類が少ない（jpeg→jpg, mov/mkv→mp4, mp3/ogg→m4a など）ため、
    // 拡張子だけ違う同名ファイルは出力先が衝突しうる。
    let mut used_outputs: HashSet<PathBuf> = HashSet::new();

    for input_file in input_files.iter() {
        let filepath = input_file.to_str().unwrap();
        let extension = input_file.extension();

        // 圧縮済みのファイルはスキップする
        if filepath.contains(format!("/{}/", &args.output_dir).as_str()) {
            continue;
        }

        let filepath = match file::get_absolute_path(input_file) {
            Ok(path) => path,
            Err(e) => {
                eprintln!("圧縮に失敗しました: {:?}: {e}", input_file);
                continue;
            }
        };

        let relative_path = file::get_relative_path(&root_dir, &input_file);
        let output_path = PathBuf::from(args.output_dir.clone()).join(relative_path);

        // 入力がサブディレクトリ配下の場合、出力先の親ディレクトリを作成する
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }

        match extension {
            Some(ext) => {
                let ext = ext.to_string_lossy().to_lowercase();
                if ext == "png" {
                    if args.webp {
                        let target = file::webp_target(&output_path, &mut used_outputs);
                        println!("png -> webp (lossless): {:?} -> {:?}", filepath, target);
                        if fs::metadata(&target).is_ok() && !args.force {
                            continue;
                        }
                        if let Err(e) = webp_image::path2compress_lossless(&filepath, &target) {
                            eprintln!("圧縮に失敗しました: {:?}: {e}", filepath);
                        }
                    } else {
                        let target = file::unique_target(&output_path, "png", &mut used_outputs);
                        println!("rgba image: {:?} -> {:?}", filepath, target);
                        if fs::metadata(&target).is_ok() && !args.force {
                            continue;
                        }
                        if let Err(e) = rgba_image::path2compress(&filepath, &target) {
                            eprintln!("圧縮に失敗しました: {:?}: {e}", filepath);
                        }
                    }
                } else if ext == "jpg" || ext == "jpeg" {
                    if args.webp {
                        let target = file::webp_target(&output_path, &mut used_outputs);
                        println!("jpg -> webp (lossy): {:?} -> {:?}", filepath, target);
                        if fs::metadata(&target).is_ok() && !args.force {
                            continue;
                        }
                        if let Err(e) = webp_image::path2compress_lossy(&filepath, &target, args.quality) {
                            eprintln!("圧縮に失敗しました: {:?}: {e}", filepath);
                        }
                    } else {
                        let target = file::unique_target(&output_path, "jpg", &mut used_outputs);
                        println!("rgb image: {:?} -> {:?}", filepath, target);
                        if fs::metadata(&target).is_ok() && !args.force {
                            continue;
                        }
                        if let Err(e) = rgb_image::path2compress(&filepath, &target, args.quality) {
                            eprintln!("圧縮に失敗しました: {:?}: {e}", filepath);
                        }
                    }
                } else if video::is_match_extension(filepath.to_str().unwrap()) {
                    let codec = if args.hevc {
                        video::VideoCodec::Hevc
                    } else {
                        video::VideoCodec::Av1
                    };
                    let target = file::unique_target(&output_path, "mp4", &mut used_outputs);
                    println!("video ({}): {:?} -> {:?}", if args.hevc { "hevc" } else { "av1" }, filepath, target);
                    if fs::metadata(&target).is_ok() && !args.force {
                        continue;
                    }
                    if let Err(e) = video::path2compress(filepath.to_str().unwrap(), target.to_str().unwrap(), codec, args.crf) {
                        eprintln!("圧縮に失敗しました: {:?}: {e}", filepath);
                    }
                } else if audio::is_match_extension(filepath.to_str().unwrap()) {
                    let source_lossless = audio::is_lossless_source(filepath.to_str().unwrap());

                    let codec = if source_lossless {
                        audio::AudioCodec::Flac
                    } else if args.opus {
                        audio::AudioCodec::Opus
                    } else {
                        audio::AudioCodec::Aac
                    };

                    let target = file::unique_target(&output_path, codec.extension(), &mut used_outputs);
                    println!("audio ({}): {:?} -> {:?}", codec.extension(), filepath, target);
                    if fs::metadata(&target).is_ok() && !args.force {
                        continue;
                    }
                    if let Err(e) = audio::path2compress(&filepath, &target, codec, &args.audio_bitrate) {
                        eprintln!("圧縮に失敗しました: {:?}: {e}", filepath);
                    }
                }
            },
            None => continue,
        }
    }
}