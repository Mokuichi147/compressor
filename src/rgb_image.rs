use image::DynamicImage;
use mozjpeg::{Compress, Marker};
use std::path::Path;
use std::time::Instant;
use crate::error::CompressError;
use crate::stats::CompressionStats;
use crate::utilities::{copy_modified_time, get_aspect_ratio, resize_to_fit, write_smaller};

pub fn path2compress(
    path: &Path,
    output_path: &Path,
    quality: f32,
    max_long_side: Option<u32>,
) -> Result<CompressionStats, CompressError> {
    let start = Instant::now();

    // 元データはメタデータの引き継ぎとサイズ比較の両方で使う
    let original = std::fs::read(path)?;

    // 軽量画像の作成
    let (jpeg_data, resized) = compress(&original, quality, max_long_side)?;

    // 縮小した場合は元と別の画像になるため、「元より大きければ元を出す」保護は使えない
    if resized {
        std::fs::write(output_path, &jpeg_data)?;
    } else {
        write_smaller(output_path, &jpeg_data, &original)?;
    }
    copy_modified_time(path, output_path)?;

    CompressionStats::measure(path, output_path, start)
}

#[allow(dead_code)]
pub fn data2compress(data: &[u8], output_path: &Path, quality: f32) -> Result<(), CompressError> {
    // 軽量画像の作成
    let (jpeg_data, _) = compress(data, quality, None)?;

    write_smaller(output_path, &jpeg_data, data)
}

#[allow(dead_code)]
pub fn get_aspect_ratio_from_path(path: &Path) -> Result<f32, CompressError> {
    // 画像を読み込む
    let img = image::open(path)?;

    Ok(get_aspect_ratio(img.width(), img.height()))
}

#[allow(dead_code)]
pub fn get_aspect_ratio_from_data(data: &[u8]) -> Result<f32, CompressError> {
    // 画像を読み込む
    let img = image::load_from_memory(data)?;

    Ok(get_aspect_ratio(img.width(), img.height()))
}

/// JPEGから APP1(Exif) と APP2(ICCプロファイル) のセグメントを取り出す。
///
/// 再エンコードするとこれらは失われる。特に Exif の Orientation が消えると、
/// 縦向きに撮影した写真が横倒しで表示されてしまうため、元ファイルから引き継ぐ。
/// ピクセルの向きは変えないので、Orientation の値もそのままで正しく機能する。
fn extract_metadata_markers(jpeg: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut markers = Vec::new();

    // SOI で始まらないものはJPEGとして扱わない
    if jpeg.len() < 4 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return markers;
    }

    let mut i = 2;
    while i + 4 <= jpeg.len() {
        if jpeg[i] != 0xFF {
            break;
        }

        let marker = jpeg[i + 1];

        // SOS(0xDA)以降は圧縮データ本体なので探索を打ち切る
        if marker == 0xDA || marker == 0xD9 {
            break;
        }

        // 長さフィールドを持たないマーカー
        if marker == 0x01 || marker == 0xFF || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }

        // 長さは自身の2バイトを含む
        let length = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
        if length < 2 || i + 2 + length > jpeg.len() {
            break;
        }

        let payload = &jpeg[i + 4..i + 2 + length];
        if marker == 0xE1 && payload.starts_with(b"Exif\0\0") {
            markers.push((1, payload.to_vec()));
        } else if marker == 0xE2 && payload.starts_with(b"ICC_PROFILE\0") {
            markers.push((2, payload.to_vec()));
        }

        i += 2 + length;
    }

    markers
}

/// JPEGに再圧縮する。縮小したかどうかも返す（縮小時はサイズ比較による保護が使えないため）。
fn compress(
    original: &[u8],
    quality: f32,
    max_long_side: Option<u32>,
) -> Result<(Vec<u8>, bool), CompressError> {
    // 画像を読み込む
    let img: DynamicImage = image::load_from_memory(original)?;

    let (original_width, original_height) = (img.width(), img.height());
    let img = resize_to_fit(img, max_long_side);
    let resized = img.width() != original_width || img.height() != original_height;

    let rgb_img = img.to_rgb8();

    // 画像の幅と高さを取得
    let width = rgb_img.width() as usize;
    let height = rgb_img.height() as usize;
    let pixels = rgb_img.into_raw();

    // mozjpegで圧縮する
    let mut comp = Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_quality(quality);
    comp.set_size(width, height);

    let mut comp = comp.start_compress(Vec::new())?;

    // メタデータはスキャンラインより先に書く必要がある。
    // 縮小した場合、Exif が記録している元の画素数とはずれるが、
    // 撮影日時・GPS・Orientation は引き続き正しく機能する。
    for (app_number, data) in extract_metadata_markers(original) {
        comp.write_marker(Marker::APP(app_number), &data);
    }

    comp.write_scanlines(&pixels)?;

    Ok((comp.finish()?, resized))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JPEGでないデータを渡してもパニックせず空を返すこと
    #[test]
    fn non_jpeg_yields_no_markers() {
        assert!(extract_metadata_markers(b"").is_empty());
        assert!(extract_metadata_markers(b"not a jpeg at all").is_empty());
        assert!(extract_metadata_markers(&[0x89, 0x50, 0x4E, 0x47]).is_empty());
    }

    /// 長さフィールドが壊れていても無限ループや境界外アクセスにならないこと
    #[test]
    fn truncated_segment_is_handled() {
        // SOI + APP1 で長さだけ巨大な値を宣言し、実データが足りない
        let broken = [0xFF, 0xD8, 0xFF, 0xE1, 0xFF, 0xFF, 0x00];
        assert!(extract_metadata_markers(&broken).is_empty());
    }

    /// APP1(Exif) を取り出せること
    #[test]
    fn extracts_exif_marker() {
        let payload = b"Exif\0\0hello";
        let length = (payload.len() + 2) as u16;
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
        jpeg.extend_from_slice(&length.to_be_bytes());
        jpeg.extend_from_slice(payload);
        jpeg.extend_from_slice(&[0xFF, 0xDA]); // SOS

        let markers = extract_metadata_markers(&jpeg);
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].0, 1);
        assert_eq!(markers[0].1, payload);
    }

    /// Exif以外のAPP1（XMPなど）は対象にしないこと
    #[test]
    fn ignores_non_exif_app1() {
        let payload = b"http://ns.adobe.com/xap/1.0/\0xmp";
        let length = (payload.len() + 2) as u16;
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1];
        jpeg.extend_from_slice(&length.to_be_bytes());
        jpeg.extend_from_slice(payload);
        jpeg.extend_from_slice(&[0xFF, 0xDA]);

        assert!(extract_metadata_markers(&jpeg).is_empty());
    }
}
