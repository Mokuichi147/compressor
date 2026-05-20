# Compressor
指定したフォルダ内に存在するファイルを圧縮し、別のフォルダに保存します。

## サポートするファイル形式
- [x] jpg, jpeg
- [x] png
- [x] webp（`--webp` 指定時に jpg/jpeg/png から出力）
- [x] mov, mp4, avi, mkv, webm
- [x] gif（静止GIFは画像として、アニメGIFは動画として圧縮）

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
  -o, --output-dir <OUTPUT_DIR>     圧縮済みファイルの保存先 [default: compress]
  -i, --input-file <INPUT_FILE>...  圧縮したいファイル（入力のない場合は全て）
  -q, --quality <QUALITY>           RGB画像の圧縮率 [default: 70.0]
  -f, --force                       圧縮済みファイルを上書きして再圧縮するか
  -w, --webp                        画像をWebPで出力する（jpg/jpeg→非可逆, png→可逆）
  -h, --help                        Print help
```

`--webp` を付けると画像をWebPで出力します（jpg/jpeg は品質指定の非可逆、png は可逆）。
拡張子は `.webp` になります。動画は対象外です。

GIFは内容に応じて自動で振り分けます。静止GIF（1フレーム）は画像として扱い、既定では
oxipngで最適化した `.png`、`--webp` 指定時は可逆WebPを出力します。アニメーションGIF
（2フレーム以上）は動画として扱い、ffmpegで `.mp4` に変換します（動画と同様 `--webp` の対象外、
ffmpegが必要）。

## ライセンス
Dual-licensed under [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT).