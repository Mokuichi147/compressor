# Compressor
指定したフォルダ内に存在するファイルを圧縮し、別のフォルダに保存します。

## サポートするファイル形式
- [x] jpg, jpeg
- [x] png
- [x] mov, mp4, avi, mkv, webm
- [ ] gif

## 使い方
### セットアップ
```sh
git clone https://github.com/Mokucihi147/compressor.git
cd compressor
cargo install --path .
```

### オプション
```
Usage: compressor [OPTIONS]

Options:
  -o, --output-dir <OUTPUT_DIR>     [default: compress]
  -i, --input-file <INPUT_FILE>...  
  -q, --quality <QUALITY>           [default: 70.0]
  -h, --help                        Print help
```

## ライセンス
Dual-licensed under [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT).