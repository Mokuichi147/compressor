use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};
use clap::Parser;
mod file;
mod scan;
mod utilities;
mod error;
mod rgb_image;
mod rgba_image;
mod webp_image;
mod gif_image;
mod video;
mod audio;
mod job;
mod stats;

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

    /// 走査から除外するグロブ（複数指定可）。ディレクトリ名だけの指定でその配下ごと除外できる
    #[clap(long, num_args = 1..)]
    exclude: Vec<String>,

    /// 隠しディレクトリ・ファイル（`.` 始まり）も対象にする
    #[clap(long)]
    include_hidden: bool,

    /// 長辺の上限（ピクセル）。超える画像・動画を縮小する。0 で縮小しない。
    /// 未指定の場合、動画のみ 1920 に収める（画像は縮小しない）
    #[clap(long)]
    max_long_side: Option<u32>,

    /// 実際には圧縮せず、どのファイルをどこにどの形式で出力するかだけを表示する
    #[clap(long)]
    dry_run: bool,
}

/// 失敗したファイルを集めておき、実行の最後にまとめて報告する。
///
/// バッチ処理を止めないために失敗しても続行するが、
/// 大量のファイルを処理すると失敗のログが成功ログに埋もれてしまうため。
#[derive(Default)]
struct Failures {
    messages: Vec<String>,
}

impl Failures {
    fn record(&mut self, path: &Path, error: impl std::fmt::Display) {
        let message = format!("{:?}: {error}", path);
        eprintln!("圧縮に失敗しました: {message}");
        self.messages.push(message);
    }

    /// 失敗の一覧を表示する。1件でもあれば true を返す（終了コードに使う）。
    fn report(&self) -> bool {
        if self.messages.is_empty() {
            return false;
        }

        eprintln!();
        eprintln!("{} 件が失敗しました:", self.messages.len());
        for message in &self.messages {
            eprintln!("  {message}");
        }

        true
    }
}

impl AppArgs {
    fn settings(&self) -> job::Settings {
        job::Settings {
            quality: self.quality,
            webp: self.webp,
            hevc: self.hevc,
            crf: self.crf,
            opus: self.opus,
            audio_bitrate: self.audio_bitrate.clone(),
            max_long_side: self.max_long_side,
        }
    }
}

fn main() -> ExitCode {
    let started_at = Instant::now();
    let mut totals = stats::Totals::default();

    let args = AppArgs::parse();
    let settings = args.settings();
    let mut failures = Failures::default();

    let excludes = match scan::Excludes::new(&args.exclude, args.include_hidden) {
        Ok(excludes) => excludes,
        Err(e) => {
            eprintln!("--exclude の指定が不正です: {e}");
            std::process::exit(2);
        }
    };

    let mut input_files = args.input_file.clone().unwrap_or_default();
    if input_files.is_empty() {
        input_files = file::get_files(".", &excludes);
    }

    std::fs::create_dir_all(&args.output_dir).unwrap();
    let root_dir = PathBuf::from(".");

    // 出力先そのものを入力にしないための基準。
    // 文字列の部分一致で判定すると、-o に絶対パスや末尾スラッシュ付きを渡したときに機能しない。
    let output_root = std::fs::canonicalize(&args.output_dir).ok();

    // ffmpeg が無いと動画・音声は1件ずつ同じ理由で失敗する。
    // ファイルごとに同じメッセージを並べても読みにくいので、最初に1回だけ報告して以降はスキップする。
    let ffmpeg_available = utilities::is_ffmpeg_available();
    if !ffmpeg_available {
        eprintln!("FFmpegが見つかりません。動画・音声・アニメーションGIFはスキップします。");
    }
    let mut skipped_without_ffmpeg = 0usize;

    // 生成済みの出力先を記録し、同名衝突を回避する。
    // 出力の拡張子は入力より種類が少ない（jpeg→jpg, mov/mkv→mp4, mp3/ogg→m4a など）ため、
    // 拡張子だけ違う同名ファイルは出力先が衝突しうる。
    let mut used_outputs: HashSet<PathBuf> = HashSet::new();

    for input_file in input_files.iter() {
        // -i で明示的に渡されたファイルにも除外指定を効かせる
        if excludes.is_excluded(input_file) {
            continue;
        }

        let source = match file::get_absolute_path(input_file) {
            Ok(path) => path,
            Err(e) => {
                failures.record(input_file, e);
                continue;
            }
        };

        // 圧縮済みのファイルはスキップする
        if output_root.as_ref().is_some_and(|root| source.starts_with(root)) {
            continue;
        }

        let relative_path = file::get_relative_path(&root_dir, input_file);
        let output_base = PathBuf::from(&args.output_dir).join(relative_path);

        let job = match job::plan(&source, &output_base, &settings, &mut used_outputs) {
            Ok(Some(job)) => job,
            // 対象外の形式
            Ok(None) => continue,
            Err(e) => {
                failures.record(&source, e);
                continue;
            }
        };

        if !ffmpeg_available && job.action.needs_ffmpeg() {
            skipped_without_ffmpeg += 1;
            continue;
        }

        // 実行するかどうかを決めてからログを出す。
        // 逆にすると、スキップしたファイルも圧縮したかのように表示されてしまう。
        if job.target.exists() && !args.force {
            println!("skip: {:?}（既に存在。--force で再圧縮）", job.target);
            continue;
        }

        println!("{}: {:?} -> {:?}", job.action.label(), job.source, job.target);
        if args.dry_run {
            continue;
        }

        // 入力がサブディレクトリ配下の場合、出力先の親ディレクトリを作成する
        if let Some(parent) = job.target.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                failures.record(&job.target, e);
                continue;
            }
        }

        match job.run() {
            Ok(stats) => {
                println!("  {}", stats.summary_line());
                totals.add(&stats);
            }
            Err(e) => failures.record(&job.source, e),
        }
    }

    if let Some(summary) = totals.summary_line(started_at.elapsed().as_secs_f64()) {
        println!();
        println!("{summary}");
    }

    let failed = failures.report();

    if skipped_without_ffmpeg > 0 {
        eprintln!();
        eprintln!("FFmpegが無いため {skipped_without_ffmpeg} 件をスキップしました");
    }

    // スキップした分も「頼まれたのに圧縮していない」ため、成功として返さない
    if failed || skipped_without_ffmpeg > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 失敗が無ければ報告せず、終了コードも成功のままにすること
    #[test]
    fn no_report_without_failures() {
        assert!(!Failures::default().report());
    }

    /// 失敗を記録したら報告し、非ゼロ終了を促すこと
    #[test]
    fn reports_recorded_failures() {
        let mut failures = Failures::default();
        failures.record(Path::new("a.jpg"), "壊れています");
        failures.record(Path::new("b.mp4"), "ffmpegが失敗");

        assert_eq!(failures.messages.len(), 2);
        assert!(failures.messages[0].contains("a.jpg"));
        assert!(failures.messages[0].contains("壊れています"));
        assert!(failures.report());
    }
}
