//! 既存の圧縮実装 (mozjpeg / oxipng) と WebP を実測比較するベンチマーク。
//!
//! 使い方:
//!   cargo run --release --example webp_bench -- <画像ディレクトリ> [品質]
//!
//! 同一ソース画像から以下を生成し、ファイルサイズ・PSNR・SSIM・エンコード時間を比較する。
//!   - 非可逆: mozjpeg(quality)  vs  WebP lossy(quality)
//!   - 可逆:   oxipng(preset2)   vs  WebP lossless

use std::path::{Path, PathBuf};
use std::time::Instant;

use image::RgbImage;
use mozjpeg::Compress;
use oxipng::{optimize_from_memory, Options};
use webp::Encoder;

const TIMING_RUNS: u32 = 5;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| "/tmp/bench/orig".to_string());
    let quality: f32 = args
        .next()
        .and_then(|q| q.parse().ok())
        .unwrap_or(70.0);

    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("ディレクトリを読めません {dir}: {e}"))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            matches!(
                p.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase()),
                Some(ref e) if e == "png" || e == "jpg" || e == "jpeg"
            )
        })
        .collect();
    paths.sort();

    if paths.is_empty() {
        eprintln!("対象画像がありません: {dir}");
        std::process::exit(1);
    }

    println!("品質パラメータ: {quality}\n");

    // ---- 非可逆比較: mozjpeg vs WebP lossy ----
    println!("== 非可逆圧縮: mozjpeg(q{quality}) vs WebP lossy(q{quality}) ==");
    println!(
        "{:<14} {:>9} {:>9} {:>9} {:>7} {:>7} {:>7} {:>7} {:>8} {:>8}",
        "image", "orig(KB)", "jpeg(KB)", "webp(KB)", "webp/jpg", "jp_PSNR", "wp_PSNR", "jp_SSIM", "wp_SSIM", "size%"
    );

    let mut tot_orig = 0u64;
    let mut tot_jpeg = 0u64;
    let mut tot_webp = 0u64;
    let (mut sum_jp_psnr, mut sum_wp_psnr, mut sum_jp_ssim, mut sum_wp_ssim) =
        (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let (mut jpeg_ms_tot, mut webp_ms_tot) = (0.0f64, 0.0f64);

    for path in &paths {
        let rgb = image::open(path)
            .unwrap_or_else(|e| panic!("画像を開けません {:?}: {e}", path))
            .to_rgb8();
        let orig_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        let (jpeg, jpeg_ms) = time(|| encode_mozjpeg(&rgb, quality));
        let (wp, webp_ms) = time(|| encode_webp_lossy(&rgb, quality));

        let jpeg_dec = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        let webp_dec = image::load_from_memory(&wp).unwrap().to_rgb8();

        let jp_psnr = psnr(&rgb, &jpeg_dec);
        let wp_psnr = psnr(&rgb, &webp_dec);
        let jp_ssim = ssim_luma(&rgb, &jpeg_dec);
        let wp_ssim = ssim_luma(&rgb, &webp_dec);

        tot_orig += orig_bytes;
        tot_jpeg += jpeg.len() as u64;
        tot_webp += wp.len() as u64;
        sum_jp_psnr += jp_psnr;
        sum_wp_psnr += wp_psnr;
        sum_jp_ssim += jp_ssim;
        sum_wp_ssim += wp_ssim;
        jpeg_ms_tot += jpeg_ms;
        webp_ms_tot += webp_ms;

        println!(
            "{:<14} {:>9.1} {:>9.1} {:>9.1} {:>7.2} {:>7.2} {:>7.2} {:>7.4} {:>8.4} {:>7.1}%",
            short(path),
            orig_bytes as f64 / 1024.0,
            jpeg.len() as f64 / 1024.0,
            wp.len() as f64 / 1024.0,
            wp.len() as f64 / jpeg.len() as f64,
            jp_psnr,
            wp_psnr,
            jp_ssim,
            wp_ssim,
            (wp.len() as f64 / jpeg.len() as f64 - 1.0) * 100.0
        );
    }

    let n = paths.len() as f64;
    println!(
        "\n  合計  : orig {:.1}KB  jpeg {:.1}KB  webp {:.1}KB  → WebPは対mozjpeg {:+.1}%",
        tot_orig as f64 / 1024.0,
        tot_jpeg as f64 / 1024.0,
        tot_webp as f64 / 1024.0,
        (tot_webp as f64 / tot_jpeg as f64 - 1.0) * 100.0
    );
    println!(
        "  平均  : PSNR jpeg {:.2}dB / webp {:.2}dB   SSIM jpeg {:.4} / webp {:.4}",
        sum_jp_psnr / n,
        sum_wp_psnr / n,
        sum_jp_ssim / n,
        sum_wp_ssim / n
    );
    println!(
        "  時間  : mozjpeg {:.1}ms/枚 平均   WebP {:.1}ms/枚 平均  (各{}回平均)",
        jpeg_ms_tot / n,
        webp_ms_tot / n,
        TIMING_RUNS
    );

    // ---- 可逆比較: oxipng vs WebP lossless ----
    println!("\n== 可逆圧縮: oxipng(preset2) vs WebP lossless ==");
    println!(
        "{:<14} {:>9} {:>10} {:>10} {:>9} {:>9} {:>10}",
        "image", "orig(KB)", "oxipng(KB)", "webpL(KB)", "ox_ms", "wp_ms", "webpL/oxi"
    );

    let (mut tot_oxi, mut tot_wpl) = (0u64, 0u64);
    let (mut oxi_ms_tot, mut wpl_ms_tot) = (0.0f64, 0.0f64);

    for path in &paths {
        let src = std::fs::read(path).unwrap();
        let rgba = image::open(path).unwrap().to_rgba8();

        let (oxi, oxi_ms) = time(|| {
            let mut opt = Options::from_preset(2);
            opt.force = true;
            optimize_from_memory(&src, &opt).unwrap()
        });
        let (wpl, wpl_ms) = time(|| encode_webp_lossless(&rgba));

        tot_oxi += oxi.len() as u64;
        tot_wpl += wpl.len() as u64;
        oxi_ms_tot += oxi_ms;
        wpl_ms_tot += wpl_ms;

        println!(
            "{:<14} {:>9.1} {:>10.1} {:>10.1} {:>8.1} {:>8.1} {:>9.2}",
            short(path),
            src.len() as f64 / 1024.0,
            oxi.len() as f64 / 1024.0,
            wpl.len() as f64 / 1024.0,
            oxi_ms,
            wpl_ms,
            wpl.len() as f64 / oxi.len() as f64
        );
    }

    println!(
        "\n  合計  : oxipng {:.1}KB  webp-lossless {:.1}KB  → WebP可逆は対oxipng {:+.1}%",
        tot_oxi as f64 / 1024.0,
        tot_wpl as f64 / 1024.0,
        (tot_wpl as f64 / tot_oxi as f64 - 1.0) * 100.0
    );
    println!(
        "  時間  : oxipng {:.1}ms/枚 平均   WebP-lossless {:.1}ms/枚 平均",
        oxi_ms_tot / n,
        wpl_ms_tot / n
    );
}

fn short(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().to_string()
}

/// 関数を TIMING_RUNS 回実行し、(最後の結果, 平均ミリ秒) を返す。
fn time<T>(mut f: impl FnMut() -> T) -> (T, f64) {
    let mut last = f();
    let start = Instant::now();
    for _ in 0..TIMING_RUNS {
        last = f();
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / TIMING_RUNS as f64;
    (last, ms)
}

/// 既存 rgb_image::compress と同じ設定で mozjpeg エンコードする。
fn encode_mozjpeg(rgb: &RgbImage, quality: f32) -> Vec<u8> {
    let width = rgb.width() as usize;
    let height = rgb.height() as usize;
    let mut comp = Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_quality(quality);
    comp.set_size(width, height);
    let mut comp = comp.start_compress(Vec::new()).unwrap();
    comp.write_scanlines(rgb.as_raw()).unwrap();
    comp.finish().unwrap()
}

fn encode_webp_lossy(rgb: &RgbImage, quality: f32) -> Vec<u8> {
    let encoder = Encoder::from_rgb(rgb.as_raw(), rgb.width(), rgb.height());
    encoder.encode(quality).to_vec()
}

fn encode_webp_lossless(rgba: &image::RgbaImage) -> Vec<u8> {
    let encoder = Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
    encoder.encode_lossless().to_vec()
}

fn psnr(a: &RgbImage, b: &RgbImage) -> f64 {
    assert_eq!(a.dimensions(), b.dimensions());
    let mut sse = 0.0f64;
    for (pa, pb) in a.pixels().zip(b.pixels()) {
        for c in 0..3 {
            let d = pa[c] as f64 - pb[c] as f64;
            sse += d * d;
        }
    }
    let n = (a.width() as f64) * (a.height() as f64) * 3.0;
    let mse = sse / n;
    if mse <= f64::EPSILON {
        return f64::INFINITY;
    }
    10.0 * (255.0f64 * 255.0 / mse).log10()
}

/// 輝度チャンネル上の 8x8 非重複ウィンドウ平均 SSIM。
fn ssim_luma(a: &RgbImage, b: &RgbImage) -> f64 {
    let (w, h) = a.dimensions();
    let luma = |img: &RgbImage| -> Vec<f64> {
        img.pixels()
            .map(|p| 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64)
            .collect()
    };
    let la = luma(a);
    let lb = luma(b);

    let c1 = (0.01 * 255.0f64).powi(2);
    let c2 = (0.03 * 255.0f64).powi(2);
    let win = 8usize;
    let (mut acc, mut count) = (0.0f64, 0u64);

    let mut y = 0usize;
    while y + win <= h as usize {
        let mut x = 0usize;
        while x + win <= w as usize {
            let (mut ma, mut mb) = (0.0f64, 0.0f64);
            for j in 0..win {
                for i in 0..win {
                    let idx = (y + j) * w as usize + (x + i);
                    ma += la[idx];
                    mb += lb[idx];
                }
            }
            let nwin = (win * win) as f64;
            ma /= nwin;
            mb /= nwin;

            let (mut va, mut vb, mut cov) = (0.0f64, 0.0f64, 0.0f64);
            for j in 0..win {
                for i in 0..win {
                    let idx = (y + j) * w as usize + (x + i);
                    let da = la[idx] - ma;
                    let db = lb[idx] - mb;
                    va += da * da;
                    vb += db * db;
                    cov += da * db;
                }
            }
            va /= nwin - 1.0;
            vb /= nwin - 1.0;
            cov /= nwin - 1.0;

            let s = ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            acc += s;
            count += 1;
            x += win;
        }
        y += win;
    }
    if count == 0 {
        1.0
    } else {
        acc / count as f64
    }
}
