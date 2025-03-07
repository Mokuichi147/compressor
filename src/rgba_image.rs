use image::DynamicImage;
use oxipng::{optimize, optimize_from_memory, InFile, Options, OutFile};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;


pub fn path2compress(path: &str, output_path: &str) {
    let mut options = Options::from_preset(2);
    options.force = true;

    let _ = optimize(&InFile::from(PathBuf::from(path)), &OutFile::from_path(PathBuf::from(output_path)), &options);
}

#[allow(dead_code)]
pub fn data2compress(data: &Vec<u8>, output_path: &str) {
    let img = image::load_from_memory(data).unwrap();

    let mut png_data = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png_data), image::ImageFormat::Png).unwrap();

    compress(&img, output_path);
}

#[allow(dead_code)]
pub fn compress(img: &DynamicImage, output_path: &str) {
    let rgba_img = img.to_rgba8().into_raw();

    let mut options = Options::from_preset(2);
    options.force = true;

    let png_data = optimize_from_memory(&rgba_img, &options).unwrap();

    let file = File::create(output_path).unwrap();
    let mut writer = BufWriter::new(file);
    std::io::copy(&mut &png_data[..], &mut writer).unwrap();
}
