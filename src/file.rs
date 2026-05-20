use std::{fs, path::{Component, PathBuf}};

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

/// `to` を `from` 起点の相対パスにする。
/// 接頭辞が一致しない場合（`-i` で `./` なしや絶対パスを渡した場合）でも
/// panic せず、ルート・`.`・`..` を取り除いて output_dir 配下に収まる相対パスを返す。
pub fn get_relative_path(from: &PathBuf, to: &PathBuf) -> PathBuf {
    if let Ok(stripped) = to.strip_prefix(from) {
        return stripped.to_path_buf();
    }

    let mut relative = PathBuf::new();
    for component in to.components() {
        if let Component::Normal(part) = component {
            relative.push(part);
        }
    }
    relative
}