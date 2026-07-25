use std::num::NonZeroUsize;
use std::path::PathBuf;

use rayon::prelude::*;

use crate::error::CompressError;
use crate::job::Job;
use crate::stats::CompressionStats;

/// 1ファイル分の実行結果。
pub struct Outcome {
    pub source: PathBuf,
    pub result: Result<CompressionStats, CompressError>,
}

/// 並列数の設定。
pub struct Concurrency {
    /// 画像の並列数。1ファイル1スレッドのCPUバウンドな処理なのでコア数まで上げられる。
    pub image_jobs: NonZeroUsize,
    /// ffmpeg を起動する処理（動画・音声）の同時実行数。
    /// ffmpeg 自体が内部でスレッドを使うため、無制限に並列化すると取り合いになって遅くなる。
    pub ffmpeg_jobs: NonZeroUsize,
}

impl Concurrency {
    /// 未指定時の画像の並列数。利用可能なコア数を使う。
    pub fn default_image_jobs() -> NonZeroUsize {
        std::thread::available_parallelism().unwrap_or(NonZeroUsize::new(1).unwrap())
    }
}

/// 決まったジョブを実行する。
///
/// 画像と ffmpeg で並列数が異なるため、種類ごとに分けて実行する。
/// 画像を先に片付けるのは、ffmpeg がコアを占有している間に画像が待たされるのを避けるため。
///
/// 結果は集計のために返すが、1行の表示はここで行う。
/// 並列実行では複数スレッドが同時に書き込むため、1回の `println!` にまとめて行が混ざらないようにする。
pub fn run_all(jobs: Vec<Job>, concurrency: &Concurrency) -> Vec<Outcome> {
    let (images, ffmpeg_jobs): (Vec<Job>, Vec<Job>) =
        jobs.into_iter().partition(|job| !job.action.needs_ffmpeg());

    let mut outcomes = run_in_pool(images, concurrency.image_jobs);
    outcomes.extend(run_in_pool(ffmpeg_jobs, concurrency.ffmpeg_jobs));

    outcomes
}

fn run_in_pool(jobs: Vec<Job>, parallelism: NonZeroUsize) -> Vec<Outcome> {
    if jobs.is_empty() {
        return Vec::new();
    }

    // 並列数が1ならスレッドプールを作らずそのまま回す
    if parallelism.get() == 1 {
        return jobs.into_iter().map(run_and_report).collect();
    }

    match rayon::ThreadPoolBuilder::new()
        .num_threads(parallelism.get())
        .build()
    {
        Ok(pool) => pool.install(|| jobs.into_par_iter().map(run_and_report).collect()),
        // スレッドを作れない環境では逐次で処理を続ける
        Err(e) => {
            eprintln!("並列実行を初期化できませんでした（逐次で続行します）: {e}");
            jobs.into_iter().map(run_and_report).collect()
        }
    }
}

fn run_and_report(job: Job) -> Outcome {
    let result = job.run();

    match &result {
        Ok(stats) => println!(
            "{}: {:?} -> {:?}\n  {}",
            job.action.label(),
            job.source,
            job.target,
            stats.summary_line()
        ),
        Err(e) => eprintln!("圧縮に失敗しました: {:?}: {e}", job.source),
    }

    Outcome {
        source: job.source,
        result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::Action;
    use std::path::Path;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// oxipng に通せる最小のPNGを用意して、実際に圧縮できるジョブを作る
    fn png_job(dir: &Path, name: &str) -> Job {
        let source = dir.join(name);
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(4, 4))
            .save(&source)
            .unwrap();

        Job {
            source,
            target: dir.join(format!("out-{name}")),
            action: Action::RgbaImage {
                max_long_side: None,
            },
        }
    }

    fn broken_job(dir: &Path, name: &str) -> Job {
        let source = dir.join(name);
        std::fs::write(&source, b"not a png").unwrap();

        Job {
            source,
            target: dir.join(format!("out-{name}")),
            action: Action::RgbaImage {
                max_long_side: None,
            },
        }
    }

    fn concurrency(image_jobs: usize, ffmpeg_jobs: usize) -> Concurrency {
        Concurrency {
            image_jobs: NonZeroUsize::new(image_jobs).unwrap(),
            ffmpeg_jobs: NonZeroUsize::new(ffmpeg_jobs).unwrap(),
        }
    }

    /// 並列数によらず、すべてのジョブが1回ずつ実行されること
    #[test]
    fn runs_every_job_once() {
        for (image_jobs, ffmpeg_jobs) in [(1, 1), (4, 2)] {
            let dir = temp_dir(&format!("compressor_task_{image_jobs}_{ffmpeg_jobs}"));
            let jobs = vec![
                png_job(&dir, "a.png"),
                png_job(&dir, "b.png"),
                png_job(&dir, "c.png"),
            ];

            let outcomes = run_all(jobs, &concurrency(image_jobs, ffmpeg_jobs));

            assert_eq!(outcomes.len(), 3, "jobs=({image_jobs}, {ffmpeg_jobs})");
            assert!(outcomes.iter().all(|outcome| outcome.result.is_ok()));
            for name in ["a.png", "b.png", "c.png"] {
                assert!(dir.join(format!("out-{name}")).exists(), "{name} が出力されていない");
            }

            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// ジョブが無くても落ちないこと
    #[test]
    fn handles_empty_jobs() {
        assert!(run_all(Vec::new(), &concurrency(4, 2)).is_empty());
    }

    /// 1件失敗しても他のジョブを止めないこと
    #[test]
    fn failure_does_not_stop_others() {
        let dir = temp_dir("compressor_task_failure");
        let jobs = vec![
            png_job(&dir, "a.png"),
            broken_job(&dir, "broken.png"),
            png_job(&dir, "b.png"),
        ];

        let outcomes = run_all(jobs, &concurrency(4, 2));

        assert_eq!(outcomes.len(), 3);
        assert_eq!(outcomes.iter().filter(|o| o.result.is_ok()).count(), 2);
        assert_eq!(outcomes.iter().filter(|o| o.result.is_err()).count(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 既定の並列数は必ず1以上になること
    #[test]
    fn default_image_jobs_is_positive() {
        assert!(Concurrency::default_image_jobs().get() >= 1);
    }
}
