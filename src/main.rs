use std::path::PathBuf;
use clap::Parser;
mod file;
mod image;
mod video;

#[derive(Parser)]
struct AppArgs {
    #[clap(short, long, default_value = "compress")]
    output_dir: String,
    
    #[clap(short, long, num_args = 1..)]
    input_file: Option<Vec<PathBuf>>,

    #[clap(short, long, default_value="70.0")]
    quality: f32,
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
        //let basename = input_file.file_stem();

        if filepath.starts_with(&args.output_dir) {
            continue;
        }
        let filepath = file::get_absolute_path(input_file);

        let relative_path = file::get_relative_path(&root_dir, &input_file);
        let mut output_path = PathBuf::from(args.output_dir.clone());
        output_path.push(relative_path);
        let output_path = output_path.to_str().unwrap();

        match extension {
            Some(ext) => {
                let ext = ext.to_string_lossy().to_lowercase();
                if ext == "jpg" || ext == "jpeg" || ext == "png" {
                    println!("image: {:?}", filepath);
                    image::path2compress(&filepath.to_str().unwrap(), output_path, args.quality);
                }
                else if video::is_match_extension(filepath.to_str().unwrap()) {
                    println!("video: {:?}", filepath);
                    video::path2compress(&filepath.to_str().unwrap(), output_path);
                }
            },
            None => continue,
        }
    }
}