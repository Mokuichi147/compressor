use std::{fs, path::PathBuf};

/// 指定されたディレクトリ内のファイルを再帰的に取得する
pub fn get_files(dir: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path: std::path::PathBuf = entry.path();
                if path.is_file() {
                    files.push(path);
                } else if path.is_dir() {
                    for filepath in get_files(path.to_str().unwrap()) {
                        files.push(filepath);
                    }
                }
            }
        }
    }
    files
}

/// 絶対パスを取得する
pub fn get_absolute_path(dir: &PathBuf) -> PathBuf {
    fs::canonicalize(dir).unwrap()
}

/// 絶対パス2つから相対パスを取得する
pub fn get_relative_path(from: &PathBuf, to: &PathBuf) -> PathBuf {
    to.strip_prefix(from).ok().map(|p| p.to_path_buf()).unwrap()
}