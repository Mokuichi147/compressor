# Compressor
指定したフォルダ内に存在するファイルを圧縮し、別のフォルダに保存します。

## サポートするファイル形式
- [x] jpg, jpeg
- [x] png
- [x] webp（`--webp` 指定時に jpg/jpeg/png から出力）
- [x] mov, mp4, avi, mkv, webm
- [x] wav, aiff, aif, flac, mp3, m4a, aac, ogg, wma
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
  -o, --output-dir <OUTPUT_DIR>     圧縮済みファイルの保存先 [default: compress]
  -i, --input-file <INPUT_FILE>...  圧縮したいファイル（入力のない場合は全て）
  -q, --quality <QUALITY>           RGB画像の圧縮率 [default: 70.0]
  -f, --force                       圧縮済みファイルを上書きして再圧縮するか
  -w, --webp                        画像をWebPで出力する（jpg/jpeg→非可逆, png→可逆）
      --hevc                        動画をHEVC(H.265)で出力する（既定はAV1）
      --crf <CRF>                   動画の品質。低いほど高品質・大きいファイル（既定: AV1=40, HEVC=28）
      --audio-lossless              音声を可逆圧縮する（既定: WAV/AIFF/FLACのみ可逆、MP3/AAC等は非可逆）
      --audio-lossy                 音声を非可逆圧縮する（既定: WAV/AIFF/FLACのみ可逆、MP3/AAC等は非可逆）
      --opus                        音声をOpusで出力する（既定はAAC。非可逆圧縮時のみ有効）
      --audio-bitrate <BITRATE>     音声の非可逆圧縮時のビットレート [default: 128k]
  -h, --help                        Print help
```

`--webp` を付けると画像をWebPで出力します（jpg/jpeg は品質指定の非可逆、png は可逆）。
拡張子は `.webp` になります。動画は対象外です。

### 画像のメタデータと向き
- **jpg/jpeg → jpg**: Exif（撮影日時・GPS・カメラ情報など）と ICC プロファイルを元ファイルから引き継ぎます。Exif の Orientation もそのまま残るため、縦向きに撮影した写真の向きが変わることはありません。
- **WebP 出力時**: WebP には Exif を引き継がないため、代わりに Orientation をピクセルに焼き込みます（見た目の向きは保たれますが、撮影日時や GPS は失われます）。
- 圧縮結果が元より大きくなる場合は、元のファイルをそのまま出力します（すでに圧縮済みの画像を再エンコードして、サイズも画質も悪化させないため）。この判定は同じ形式で出力する場合のみ働くため、`--webp` による形式変換には適用されません。

### 動画の圧縮
動画は CRF を尊重するソフトウェアエンコーダで圧縮します（出力は `.mp4`）。

- 既定は **AV1**（`libsvtav1`）。同等品質で HEVC より大幅に小さくなります。
- `--hevc` を付けると **HEVC/H.265**（`libx265`, `hvc1` タグ）で出力します。iOS など旧来デバイスでの再生互換性が高い反面、AV1 より圧縮率は劣ります。
- `--crf` で品質を調整できます。値が低いほど高品質・大きいファイルになります。

> CRF スケールはコーデックで異なります（AV1 の方が同じ数値でも高品質寄り）。そのため未指定時の既定値はコーデックごとに分けています（AV1=40, HEVC=28）。

> AV1 はハードウェア再生対応が限られる機器があります（M1/M2 Mac、iPhone 15 Pro 未満、一部 Android/旧TV など）。これらでの再生互換を重視する場合は `--hevc` を使ってください。

### 音声の圧縮
動画と同様、音声も FFmpeg を使って圧縮します。

- 既定では入力の種類に応じて自動で可逆/非可逆を選びます。
  - **WAV / AIFF / FLAC**（可逆音源）→ **FLAC** で可逆圧縮
  - **MP3 / AAC / M4A / OGG / WMA**（非可逆音源）→ **AAC** で非可逆再エンコード
- `--audio-lossless` を付けると、入力の種類に関わらず常に FLAC（可逆）で出力します。
- `--audio-lossy` を付けると、入力の種類に関わらず常に非可逆（既定は AAC）で出力します。
- `--opus` を付けると、非可逆圧縮時のコーデックを AAC の代わりに Opus にします（同ビットレートで AAC より高音質になりやすい）。
- `--audio-bitrate` で非可逆圧縮時のビットレートを調整できます（既定: `128k`）。FLAC 可逆圧縮時は無視されます。
- カバーアート（アルバムアート）は FLAC / AAC 出力では引き継がれます。Opus 出力では取り除かれます。
- 出力の拡張子が集約されるため、`song.mp3` と `song.m4a` のように同名で拡張子だけ違うファイルは出力先が衝突します。この場合、2 つ目以降は `song.mp3.m4a` のように元の拡張子を残した名前になります（`--webp` と同じ挙動）。

> 非可逆音源（MP3 など）を可逆圧縮しても失われた音質は復元されません。`--audio-lossless` はファイル形式を揃えたい場合などに使ってください。

## ライセンス
Dual-licensed under [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT).