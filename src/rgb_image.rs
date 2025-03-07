use image::DynamicImage;
use mozjpeg::Compress;
use std::fs::File;
use std::io::BufWriter;


pub fn path2compress(path: &str, output_path: &str, quality: f32) {
    // 画像を読み込む
    let img = image::open(path).unwrap();

    // 軽量画像の作成
    compress(&img, output_path, quality);
}

#[allow(dead_code)]
pub fn data2compress(data: &Vec<u8>, output_path: &str, quality: f32) {
    // 画像を読み込む
    let img = image::load_from_memory(data).unwrap();

    // 軽量画像の作成
    compress(&img, output_path, quality);
}


fn compress(img: &DynamicImage, output_path: &str, quality: f32) {
    // 画像を読み込む
    let rgb_img = img.to_rgb8();

    // 画像の幅と高さを取得
    let width = rgb_img.width() as usize;
    let height = rgb_img.height() as usize;
    let pixels = rgb_img.into_raw();

    // mozjpegで圧縮する
    let mut comp = Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_quality(quality as f32);
    comp.set_size(width, height);

    let mut comp = comp.start_compress(Vec::new()).unwrap();
    comp.write_scanlines(&pixels).unwrap();
    let jpeg_data = comp.finish().unwrap();

    // 圧縮されたデータをファイルに保存
    let file = File::create(output_path).unwrap();
    let mut writer = BufWriter::new(file);
    std::io::copy(&mut &jpeg_data[..], &mut writer).unwrap();
}
