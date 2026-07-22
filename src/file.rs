use std::{collections::HashSet, fs, path::{Component, PathBuf}};

/// 指定されたディレクトリ内のファイルを再帰的に取得する。
/// 出力先が衝突した際にどちらが元の名前を取るかを実行ごとに変えないため、パス順にソートして返す。
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
    files.sort();
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

/// 拡張子を `ext` に置き換えた出力先のパスを決める。通常は拡張子を置き換えるだけだが、
/// 同一実行内で別の入力が既に同じ名前を取っている場合は元の拡張子を残して
/// 衝突（無言スキップ・上書き）を防ぐ。
/// 例: photo.jpg と photo.png → photo.webp, photo.png.webp
/// 例: song.m4a と song.mp3 → song.m4a, song.mp3.m4a
pub fn unique_target(base: &PathBuf, ext: &str, used: &mut HashSet<PathBuf>) -> PathBuf {
    let mut clean = base.clone();
    clean.set_extension(ext);
    if used.insert(clean.clone()) {
        return clean;
    }

    // 元の拡張子を残した候補。それも埋まっている場合は連番を付ける。
    let name = base.file_name().unwrap().to_string_lossy().into_owned();
    let mut candidate = base.with_file_name(format!("{name}.{ext}"));
    let mut counter = 1;
    while !used.insert(candidate.clone()) {
        counter += 1;
        candidate = base.with_file_name(format!("{name}-{counter}.{ext}"));
    }
    candidate
}

/// `--webp` 出力先のパスを決める。[`unique_target`] の webp 固定版。
pub fn webp_target(base: &PathBuf, used: &mut HashSet<PathBuf>) -> PathBuf {
    unique_target(base, "webp", used)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 走査結果が順序不定だと、衝突時にどちらが元の名前を取るか実行ごとに変わってしまう
    #[test]
    fn get_files_returns_sorted_paths() {
        let dir = std::env::temp_dir().join("compressor_get_files_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        for name in ["z.png", "a.png", "m.png"] {
            fs::write(dir.join(name), b"x").unwrap();
        }
        fs::write(dir.join("sub").join("b.png"), b"x").unwrap();

        let files = get_files(dir.to_str().unwrap());
        let mut sorted = files.clone();
        sorted.sort();
        assert_eq!(files, sorted, "get_files がソートされていない");
        assert_eq!(files.len(), 4);

        let _ = fs::remove_dir_all(&dir);
    }

    /// 衝突がなければ拡張子を置き換えるだけ
    #[test]
    fn replaces_extension() {
        let mut used = HashSet::new();
        let target = unique_target(&PathBuf::from("compress/song.mp3"), "m4a", &mut used);
        assert_eq!(target, PathBuf::from("compress/song.m4a"));
    }

    /// song.m4a と song.mp3 が同じ song.m4a に潰れないこと
    #[test]
    fn keeps_original_extension_on_collision() {
        let mut used = HashSet::new();
        let first = unique_target(&PathBuf::from("compress/song.m4a"), "m4a", &mut used);
        let second = unique_target(&PathBuf::from("compress/song.mp3"), "m4a", &mut used);
        assert_eq!(first, PathBuf::from("compress/song.m4a"));
        assert_eq!(second, PathBuf::from("compress/song.mp3.m4a"));
    }

    /// 非可逆音源は5拡張子すべてが m4a に集約されるため、3件以上の衝突も起こりうる
    #[test]
    fn distinct_targets_for_many_collisions() {
        let mut used = HashSet::new();
        let targets: Vec<PathBuf> = ["song.mp3", "song.aac", "song.ogg", "song.wma"]
            .iter()
            .map(|name| unique_target(&PathBuf::from(format!("compress/{name}")), "m4a", &mut used))
            .collect();

        let unique: HashSet<&PathBuf> = targets.iter().collect();
        assert_eq!(unique.len(), targets.len(), "出力先が重複した: {targets:?}");
    }

    /// 拡張子を残した候補まで埋まっている場合は連番でさらに回避する
    #[test]
    fn falls_back_to_counter() {
        let mut used = HashSet::new();
        unique_target(&PathBuf::from("compress/song.m4a"), "m4a", &mut used);
        unique_target(&PathBuf::from("compress/song.mp3.m4a"), "m4a", &mut used);
        let third = unique_target(&PathBuf::from("compress/song.mp3"), "m4a", &mut used);
        assert_eq!(third, PathBuf::from("compress/song.mp3-2.m4a"));
    }

    /// webp_target の委譲後も従来の例（photo.jpg と photo.png）どおりに動くこと
    #[test]
    fn webp_target_keeps_previous_behavior() {
        let mut used = HashSet::new();
        let jpg = webp_target(&PathBuf::from("compress/photo.jpg"), &mut used);
        let png = webp_target(&PathBuf::from("compress/photo.png"), &mut used);
        assert_eq!(jpg, PathBuf::from("compress/photo.webp"));
        assert_eq!(png, PathBuf::from("compress/photo.png.webp"));
    }
}
