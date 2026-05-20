use std::{collections::HashSet, fs, path::PathBuf};
use clap::Parser;
mod file;
mod utilities;
mod rgb_image;
mod rgba_image;
mod webp_image;
mod video;

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
}

fn main() {
    let args = AppArgs::parse();

    let mut input_files = args.input_file.unwrap_or_default();
    if input_files.len() == 0 {
        input_files = file::get_files(".");
    }

    std::fs::create_dir_all(&args.output_dir).unwrap();
    let root_dir = PathBuf::from(".");

    // --webp で生成済みの出力先を記録し、同名衝突を回避する
    let mut webp_outputs: HashSet<PathBuf> = HashSet::new();

    for input_file in input_files.iter() {
        let filepath = input_file.to_str().unwrap();
        let extension = input_file.extension();

        // 圧縮済みのファイルはスキップする
        if filepath.contains(format!("/{}/", &args.output_dir).as_str()) {
            continue;
        }

        let filepath = file::get_absolute_path(input_file);

        let relative_path = file::get_relative_path(&root_dir, &input_file);
        let mut output_path = PathBuf::from(args.output_dir.clone());
        output_path.push(relative_path);

        // 入力がサブディレクトリ配下の場合、出力先の親ディレクトリを作成する
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }

        match extension {
            Some(ext) => {
                let ext = ext.to_string_lossy().to_lowercase();
                if ext == "png" {
                    if args.webp {
                        let target = file::webp_target(&output_path, &mut webp_outputs);
                        println!("png -> webp (lossless): {:?} -> {:?}", filepath, target);
                        if fs::metadata(&target).is_ok() && !args.force {
                            continue;
                        }
                        webp_image::path2compress_lossless(&PathBuf::from(&filepath), &target);
                    } else {
                        println!("rgba image: {:?}", filepath);
                        output_path.set_extension("png");
                        if fs::metadata(&output_path).is_ok() && !args.force {
                            continue;
                        }
                        rgba_image::path2compress(&PathBuf::from(&filepath), &output_path);
                    }
                } else if ext == "jpg" || ext == "jpeg" {
                    if args.webp {
                        let target = file::webp_target(&output_path, &mut webp_outputs);
                        println!("jpg -> webp (lossy): {:?} -> {:?}", filepath, target);
                        if fs::metadata(&target).is_ok() && !args.force {
                            continue;
                        }
                        webp_image::path2compress_lossy(&PathBuf::from(&filepath), &target, args.quality);
                    } else {
                        println!("rgb image: {:?}", filepath);
                        output_path.set_extension("jpg");
                        if fs::metadata(&output_path).is_ok() && !args.force {
                            continue;
                        }
                        rgb_image::path2compress(&PathBuf::from(&filepath), &output_path, args.quality);
                    }
                } else if video::is_match_extension(filepath.to_str().unwrap()) {
                    let codec = if args.hevc {
                        video::VideoCodec::Hevc
                    } else {
                        video::VideoCodec::Av1
                    };
                    println!("video ({}): {:?}", if args.hevc { "hevc" } else { "av1" }, filepath);
                    output_path.set_extension("mp4");
                    if fs::metadata(&output_path).is_ok() && !args.force {
                        continue;
                    }
                    video::path2compress(&filepath.to_str().unwrap(), output_path.to_str().unwrap(), codec, args.crf);
                }
            },
            None => continue,
        }
    }
}