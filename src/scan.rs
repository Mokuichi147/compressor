use std::path::Path;

use glob::{Pattern, PatternError};

/// 走査対象から除外するパターン。
///
/// `--exclude` で指定されたグロブに加えて、隠しディレクトリの扱いも持つ。
pub struct Excludes {
    patterns: Vec<Pattern>,
    include_hidden: bool,
}

impl Excludes {
    /// グロブをコンパイルする。書き方が不正な場合はエラーを返す。
    pub fn new(patterns: &[String], include_hidden: bool) -> Result<Self, PatternError> {
        Ok(Excludes {
            patterns: patterns
                .iter()
                .map(|pattern| Pattern::new(pattern))
                .collect::<Result<Vec<_>, _>>()?,
            include_hidden,
        })
    }

    /// 除外対象かどうか。
    ///
    /// パス全体だけでなく、途中のディレクトリにも当てる。
    /// `--exclude node_modules` のようにディレクトリ名だけを指定できるようにするため。
    pub fn is_excluded(&self, path: &Path) -> bool {
        if !self.include_hidden && has_hidden_component(path) {
            return true;
        }

        path.ancestors()
            .any(|ancestor| self.matches_any(ancestor))
    }

    /// 走査中にディレクトリへ降りるかどうか。
    /// 中身を見る前に枝ごと落とせるので、`.git` や `target` を無駄に歩かずに済む。
    pub fn should_enter(&self, dir: &Path) -> bool {
        if !self.include_hidden && is_hidden(dir) {
            return false;
        }

        !self.matches_any(dir)
    }

    fn matches_any(&self, path: &Path) -> bool {
        self.patterns.iter().any(|pattern| {
            // パス全体（"docs/**"）とファイル名・ディレクトリ名（"*.tmp"）の両方を見る
            pattern.matches_path(path)
                || path
                    .file_name()
                    .is_some_and(|name| pattern.matches(&name.to_string_lossy()))
        })
    }
}

/// 先頭が `.` のディレクトリ・ファイルか。`.` や `..` 自体は隠し扱いにしない。
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .map(|name| {
            let name = name.to_string_lossy();
            name.starts_with('.') && name != "." && name != ".."
        })
        .unwrap_or(false)
}

/// パスのどこかに隠しディレクトリ・ファイルを含むか。
fn has_hidden_component(path: &Path) -> bool {
    path.ancestors().any(is_hidden)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn excludes(patterns: &[&str]) -> Excludes {
        let patterns: Vec<String> = patterns.iter().map(|p| p.to_string()).collect();
        Excludes::new(&patterns, false).unwrap()
    }

    /// 何も指定しなければ通常のファイルは除外されないこと
    #[test]
    fn keeps_normal_paths() {
        let excludes = excludes(&[]);
        assert!(!excludes.is_excluded(&PathBuf::from("photo.jpg")));
        assert!(!excludes.is_excluded(&PathBuf::from("sub/dir/photo.jpg")));
    }

    /// 隠しディレクトリ配下は既定で除外されること（.git の中まで歩かないため）
    #[test]
    fn excludes_hidden_by_default() {
        let excludes = excludes(&[]);
        assert!(excludes.is_excluded(&PathBuf::from(".git/objects/x.png")));
        assert!(excludes.is_excluded(&PathBuf::from("sub/.cache/x.png")));
        assert!(excludes.is_excluded(&PathBuf::from(".hidden.jpg")));
        assert!(!excludes.should_enter(&PathBuf::from(".git")));
    }

    /// 明示的に指定すれば隠しディレクトリも対象にできること
    #[test]
    fn can_include_hidden() {
        let excludes = Excludes::new(&[], true).unwrap();
        assert!(!excludes.is_excluded(&PathBuf::from(".git/objects/x.png")));
        assert!(excludes.should_enter(&PathBuf::from(".git")));
    }

    /// ディレクトリ名だけの指定で、その配下すべてが除外されること
    #[test]
    fn excludes_by_directory_name() {
        let excludes = excludes(&["target"]);
        assert!(excludes.is_excluded(&PathBuf::from("target/release/a.png")));
        assert!(!excludes.should_enter(&PathBuf::from("target")));
        assert!(!excludes.is_excluded(&PathBuf::from("src/a.png")));
    }

    /// 拡張子のグロブが、階層の深さによらず効くこと
    #[test]
    fn excludes_by_glob() {
        let excludes = excludes(&["*.tmp"]);
        assert!(excludes.is_excluded(&PathBuf::from("a.tmp")));
        assert!(excludes.is_excluded(&PathBuf::from("sub/dir/a.tmp")));
        assert!(!excludes.is_excluded(&PathBuf::from("a.jpg")));
    }

    /// パス全体に対するグロブも書けること
    #[test]
    fn excludes_by_path_glob() {
        let excludes = excludes(&["docs/**"]);
        assert!(excludes.is_excluded(&PathBuf::from("docs/img/a.png")));
        assert!(!excludes.is_excluded(&PathBuf::from("src/img/a.png")));
    }

    /// 複数指定できること
    #[test]
    fn accepts_multiple_patterns() {
        let excludes = excludes(&["target", "*.tmp"]);
        assert!(excludes.is_excluded(&PathBuf::from("target/a.png")));
        assert!(excludes.is_excluded(&PathBuf::from("src/a.tmp")));
        assert!(!excludes.is_excluded(&PathBuf::from("src/a.png")));
    }

    /// 不正なグロブは起動時にエラーにすること（黙って無視すると気付けない）
    #[test]
    fn rejects_invalid_pattern() {
        assert!(Excludes::new(&["[".to_string()], false).is_err());
    }
}
