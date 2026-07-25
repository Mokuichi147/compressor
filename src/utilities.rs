use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use crate::error::CompressError;

/// 圧縮結果と元データのうち、小さいほうを書き出す。
///
/// 既に圧縮済みのファイルを再エンコードすると、サイズが増えたうえに画質だけ落ちることがある。
/// 入力と出力が同じ形式のときにのみ使えることに注意（webp変換などでは元データを書けない）。
pub fn write_smaller(
    output_path: &Path,
    compressed: &[u8],
    original: &[u8],
) -> Result<(), CompressError> {
    let data = if compressed.len() < original.len() {
        compressed
    } else {
        original
    };

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(data)?;

    Ok(())
}

/// FFmpegが使えるかを判定する。プロセス起動を伴うため一度だけ実行して結果を使い回す。
pub fn is_ffmpeg_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| Command::new("ffmpeg").arg("-version").output().is_ok())
}

pub fn get_aspect_ratio(width: u32, height: u32) -> f32 {
    if width == 0 || height == 0 {
        return 0.0;
    }

    (width as f32) / (height as f32)
}

/// 入力の音声ストリームの情報。
pub struct AudioStreamInfo {
    /// ffprobe が返すコーデック名（"aac", "mp3" など）
    pub codec_name: String,
    /// ビットレート（bps）。取得できない場合は `None`
    pub bitrate_bps: Option<u64>,
}

/// 先頭の音声ストリームの情報を ffprobe で取得する。取得できない場合は `None`。
pub fn probe_audio_stream(path: &Path) -> Option<AudioStreamInfo> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_name,bit_rate",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut codec_name = None;
    let mut bitrate_bps = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "codec_name" => codec_name = Some(value.trim().to_string()),
            // 取得できない場合は "N/A" が入るためパースに失敗させる
            "bit_rate" => bitrate_bps = value.trim().parse::<u64>().ok(),
            _ => {}
        }
    }

    let codec_name = codec_name?;

    // ogg/wma などはストリーム単位のビットレートを持たないことがある。
    // その場合はファイル全体のビットレートで代用する。
    let bitrate_bps = bitrate_bps.or_else(|| probe_format_bitrate(path));

    Some(AudioStreamInfo {
        codec_name,
        bitrate_bps,
    })
}

/// ファイル全体のビットレート（bps）を ffprobe で取得する。
fn probe_format_bitrate(path: &Path) -> Option<u64> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=bit_rate",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

/// `"128k"` / `"1.5M"` / `"128000"` のようなビットレート指定を bps に変換する。
/// ffmpeg と同じく k=1000, M=1000000 として扱う。
pub fn parse_bitrate(spec: &str) -> Option<u64> {
    let spec = spec.trim();

    // 接尾辞は必ず ASCII のためバイト境界の心配はいらない
    let (number, multiplier) = match spec.chars().last()? {
        'k' | 'K' => (&spec[..spec.len() - 1], 1_000f64),
        'm' | 'M' => (&spec[..spec.len() - 1], 1_000_000f64),
        _ => (spec, 1f64),
    };

    let value: f64 = number.parse().ok()?;
    if value <= 0.0 {
        return None;
    }

    Some((value * multiplier) as u64)
}

/// 指定ビットレートを、元の音声ビットレートを超えないように丸める。
///
/// 非可逆音源を非可逆で再エンコードする場合、元より高いビットレートを指定しても
/// 失われた音質は戻らず、ファイルサイズだけが増える。
/// 元のビットレートが分からない場合は指定値をそのまま使う。
pub fn capped_bitrate(requested: &str, source_bps: Option<u64>) -> String {
    let (Some(requested_bps), Some(source_bps)) = (parse_bitrate(requested), source_bps) else {
        return requested.to_string();
    };

    if source_bps < requested_bps {
        source_bps.to_string()
    } else {
        requested.to_string()
    }
}

/// 拡張子が（大文字小文字を無視して）同じかどうか。
/// 「元より大きければ元を出す」保護は形式が変わらない場合にしか使えないため、その判定に用いる。
pub fn same_extension(a: &Path, b: &Path) -> bool {
    fn extension(path: &Path) -> Option<String> {
        path.extension().map(|e| e.to_string_lossy().to_lowercase())
    }

    match (extension(a), extension(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// 出力が元より大きければ、元ファイルをそのまま出力先へコピーする。
///
/// [`write_smaller`] のファイル版。入力と出力が同じ形式のときにのみ使えることに注意。
/// 置き換えた場合は `true` を返す。
pub fn replace_with_original_if_larger(
    input_path: &Path,
    output_path: &Path,
) -> Result<bool, CompressError> {
    let original_size = std::fs::metadata(input_path)?.len();
    let compressed_size = std::fs::metadata(output_path)?.len();

    if compressed_size <= original_size {
        return Ok(false);
    }

    std::fs::copy(input_path, output_path)?;

    Ok(true)
}

/// 出力ファイルの更新日時を元ファイルに合わせる。
///
/// メタデータを持てない形式でも「撮影した順に並べる」用途を保てるようにするため、
/// すべての圧縮処理の最後に呼ぶ。
pub fn copy_modified_time(input_path: &Path, output_path: &Path) -> Result<(), CompressError> {
    let modified = std::fs::metadata(input_path)?.modified()?;

    // 更新日時の変更には書き込み用に開いたハンドルが必要
    let file = File::options().write(true).open(output_path)?;
    file.set_modified(modified)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ffmpeg と同じ単位（k=1000）で解釈すること
    #[test]
    fn parses_bitrate_spec() {
        assert_eq!(parse_bitrate("128k"), Some(128_000));
        assert_eq!(parse_bitrate("128K"), Some(128_000));
        assert_eq!(parse_bitrate("128000"), Some(128_000));
        assert_eq!(parse_bitrate("1.5M"), Some(1_500_000));
        assert_eq!(parse_bitrate(" 96k "), Some(96_000));
    }

    /// 不正な指定は None を返し、呼び出し側で指定値をそのまま使わせること
    #[test]
    fn rejects_invalid_bitrate_spec() {
        assert_eq!(parse_bitrate(""), None);
        assert_eq!(parse_bitrate("abc"), None);
        assert_eq!(parse_bitrate("0k"), None);
        assert_eq!(parse_bitrate("-64k"), None);
    }

    /// 元より高いビットレートを指定しても元の値まで下げること
    #[test]
    fn caps_bitrate_to_source() {
        assert_eq!(capped_bitrate("128k", Some(64_000)), "64000");
    }

    /// 元のほうが高い場合は指定値をそのまま使うこと
    #[test]
    fn keeps_requested_bitrate_when_source_is_higher() {
        assert_eq!(capped_bitrate("128k", Some(320_000)), "128k");
        assert_eq!(capped_bitrate("128k", Some(128_000)), "128k");
    }

    /// 元のビットレートが不明なら指定値をそのまま使うこと
    #[test]
    fn keeps_requested_bitrate_when_source_is_unknown() {
        assert_eq!(capped_bitrate("128k", None), "128k");
    }

    /// 形式が変わる場合に元データを書いてしまわないよう、拡張子を比較できること
    #[test]
    fn compares_extensions_case_insensitively() {
        assert!(same_extension(Path::new("a.mp4"), Path::new("b.mp4")));
        assert!(same_extension(Path::new("a.MP4"), Path::new("b.mp4")));
        assert!(!same_extension(Path::new("a.mov"), Path::new("b.mp4")));
        assert!(!same_extension(Path::new("a"), Path::new("b.mp4")));
    }

    /// 出力が元より大きい場合は元で置き換えること
    #[test]
    fn replaces_when_output_is_larger() {
        let dir = std::env::temp_dir().join("compressor_replace_larger");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let input = dir.join("in.mp4");
        let output = dir.join("out.mp4");
        std::fs::write(&input, vec![b'a'; 100]).unwrap();
        std::fs::write(&output, vec![b'b'; 200]).unwrap();

        assert!(replace_with_original_if_larger(&input, &output).unwrap());
        assert_eq!(std::fs::read(&output).unwrap(), vec![b'a'; 100]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 出力のほうが小さければ触らないこと
    #[test]
    fn keeps_output_when_smaller() {
        let dir = std::env::temp_dir().join("compressor_keep_smaller");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let input = dir.join("in.mp4");
        let output = dir.join("out.mp4");
        std::fs::write(&input, vec![b'a'; 200]).unwrap();
        std::fs::write(&output, vec![b'b'; 100]).unwrap();

        assert!(!replace_with_original_if_larger(&input, &output).unwrap());
        assert_eq!(std::fs::read(&output).unwrap(), vec![b'b'; 100]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 出力の更新日時が元ファイルに揃うこと
    #[test]
    fn copies_modified_time_from_input() {
        let dir = std::env::temp_dir().join("compressor_mtime");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let input = dir.join("in.jpg");
        let output = dir.join("out.jpg");
        std::fs::write(&input, b"original").unwrap();
        std::fs::write(&output, b"compressed").unwrap();

        // 元ファイルの更新日時を過去にずらしてから引き継げているかを見る
        let past = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        File::options()
            .write(true)
            .open(&input)
            .unwrap()
            .set_modified(past)
            .unwrap();

        copy_modified_time(&input, &output).unwrap();

        assert_eq!(std::fs::metadata(&output).unwrap().modified().unwrap(), past);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 元ファイルが無い場合はErrを返し、panicしないこと
    #[test]
    fn missing_input_yields_error() {
        let dir = std::env::temp_dir().join("compressor_mtime_missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let output = dir.join("out.jpg");
        std::fs::write(&output, b"compressed").unwrap();

        assert!(copy_modified_time(&dir.join("missing.jpg"), &output).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
