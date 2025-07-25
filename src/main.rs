use std::{fs, path::PathBuf};
use clap::Parser;
mod file;
mod rgb_image;
mod rgba_image;
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
    force: bool
}

fn main() {
    let args = AppArgs::parse();

    let mut input_files = args.input_file.unwrap_or_default();
    if input_files.len() == 0 {
        input_files = file::get_files(".");
    }

    std::fs::create_dir_all(&args.output_dir).unwrap();
    let root_dir = PathBuf::from(".");

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

        match extension {
            Some(ext) => {
                let ext = ext.to_string_lossy().to_lowercase();
                if ext == "png" {
                    println!("rgba image: {:?}", filepath);
                    output_path.set_extension("png");
                    if fs::metadata(&output_path).is_ok() && !args.force {
                        continue;
                    }
                    rgba_image::path2compress(&PathBuf::from(&filepath), &PathBuf::from(output_path));
                } else if ext == "jpg" || ext == "jpeg" {
                    println!("rgb image: {:?}", filepath);
                    output_path.set_extension("jpg");
                    if fs::metadata(&output_path).is_ok() && !args.force {
                        continue;
                    }
                    rgb_image::path2compress(&PathBuf::from(&filepath), &PathBuf::from(output_path), args.quality);
                } else if video::is_match_extension(filepath.to_str().unwrap()) {
                    println!("video: {:?}", filepath);
                    output_path.set_extension("mp4");
                    if fs::metadata(&output_path).is_ok() && !args.force {
                        continue;
                    }
                    video::path2compress(&filepath.to_str().unwrap(), output_path.to_str().unwrap());
                }
            },
            None => continue,
        }
    }
}