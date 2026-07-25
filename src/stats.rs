use std::path::Path;
use std::time::Instant;

use crate::error::CompressError;

/// 1ファイル分の圧縮結果。
pub struct CompressionStats {
    /// 元のファイルサイズ（バイト）
    pub original_size: u64,
    /// 圧縮後のファイルサイズ（バイト）
    pub compressed_size: u64,
    /// 圧縮にかかった時間（秒）
    pub duration_seconds: f64,
}

impl CompressionStats {
    /// 入出力ファイルのサイズを実測して結果を組み立てる。
    /// 圧縮方式によらず同じ形で結果を返せるよう、書き出し後のファイルから測る。
    pub fn measure(
        input_path: &Path,
        output_path: &Path,
        started_at: Instant,
    ) -> Result<Self, CompressError> {
        Ok(CompressionStats {
            original_size: std::fs::metadata(input_path)?.len(),
            compressed_size: std::fs::metadata(output_path)?.len(),
            duration_seconds: started_at.elapsed().as_secs_f64(),
        })
    }

    /// サイズ削減率（%）。増えた場合は負の値になる。
    pub fn size_reduction_percent(&self) -> f64 {
        if self.original_size == 0 {
            return 0.0;
        }

        100.0 * (1.0 - (self.compressed_size as f64 / self.original_size as f64))
    }

    /// `2.1MB -> 480.3KB (-77.6%)` の形式。
    /// 圧縮に1秒以上かかった場合だけ所要時間も添える（画像で毎回出すと煩いため）。
    pub fn summary_line(&self) -> String {
        let base = format!(
            "{} -> {} ({:+.1}%)",
            format_size(self.original_size),
            format_size(self.compressed_size),
            -self.size_reduction_percent(),
        );

        if self.duration_seconds >= 1.0 {
            format!("{base} {:.1}s", self.duration_seconds)
        } else {
            base
        }
    }
}

/// 実行全体の集計。
#[derive(Default)]
pub struct Totals {
    pub files: usize,
    pub original_size: u64,
    pub compressed_size: u64,
}

impl Totals {
    pub fn add(&mut self, stats: &CompressionStats) {
        self.files += 1;
        self.original_size += stats.original_size;
        self.compressed_size += stats.compressed_size;
    }

    /// 実行の最後に出す合計行。1件も処理していない場合は `None`。
    pub fn summary_line(&self, elapsed_seconds: f64) -> Option<String> {
        if self.files == 0 {
            return None;
        }

        let total = CompressionStats {
            original_size: self.original_size,
            compressed_size: self.compressed_size,
            duration_seconds: 0.0,
        };

        Some(format!(
            "{} ファイル: {} -> {} ({:+.1}%) / {:.1}s",
            self.files,
            format_size(self.original_size),
            format_size(self.compressed_size),
            -total.size_reduction_percent(),
            elapsed_seconds,
        ))
    }
}

/// バイト数を読みやすい単位にする（1024区切り）。
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];

    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{}B", bytes)
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(original: u64, compressed: u64, duration: f64) -> CompressionStats {
        CompressionStats {
            original_size: original,
            compressed_size: compressed,
            duration_seconds: duration,
        }
    }

    /// 単位が繰り上がること
    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(0), "0B");
        assert_eq!(format_size(512), "512B");
        assert_eq!(format_size(1024), "1.0KB");
        assert_eq!(format_size(1536), "1.5KB");
        assert_eq!(format_size(1024 * 1024), "1.0MB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.0GB");
    }

    /// 削減率の計算
    #[test]
    fn computes_reduction() {
        assert!((stats(1000, 250, 0.0).size_reduction_percent() - 75.0).abs() < 0.01);
        assert!((stats(1000, 1000, 0.0).size_reduction_percent()).abs() < 0.01);
    }

    /// サイズが増えた場合は負の削減率、表示は + になること
    #[test]
    fn shows_growth_as_positive_sign() {
        let grown = stats(1000, 1200, 0.0);
        assert!(grown.size_reduction_percent() < 0.0);
        assert!(grown.summary_line().contains("+20.0%"), "{}", grown.summary_line());
    }

    /// 0バイトの入力でゼロ除算しないこと
    #[test]
    fn handles_empty_input() {
        assert_eq!(stats(0, 0, 0.0).size_reduction_percent(), 0.0);
    }

    /// 所要時間は1秒以上のときだけ添えること
    #[test]
    fn shows_duration_only_when_slow() {
        assert_eq!(stats(1000, 250, 0.03).summary_line(), "1000B -> 250B (-75.0%)");
        assert_eq!(stats(1000, 250, 4.25).summary_line(), "1000B -> 250B (-75.0%) 4.2s");
    }

    /// 合計は処理したファイルの分だけ積み上がること
    #[test]
    fn accumulates_totals() {
        let mut totals = Totals::default();
        totals.add(&stats(1000, 250, 0.0));
        totals.add(&stats(3000, 750, 0.0));

        assert_eq!(totals.files, 2);
        assert_eq!(
            totals.summary_line(1.5).unwrap(),
            "2 ファイル: 3.9KB -> 1000B (-75.0%) / 1.5s"
        );
    }

    /// 1件も処理していなければ合計行を出さないこと
    #[test]
    fn no_summary_without_files() {
        assert!(Totals::default().summary_line(0.1).is_none());
    }
}
