# Compressor
指定したフォルダ内に存在するファイルを圧縮し、別のフォルダに保存します。

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