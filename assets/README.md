# KohdaLab IV application icon

The blue and orange curves represent the forward and backward I-V sweep on a
dark instrument display. The project MIT license also applies to these assets.

- `kohdalab-iv.svg`: editable vector export.
- `kohdalab-iv.png`: transparent 1024 by 1024 master preview.
- `kohdalab-iv-window.png`: embedded 256 by 256 GUI icon.
- `kohdalab-iv.ico`: Windows 16, 24, 32, 48, 64, 128 and 256 pixel icon images.
- `kohdalab-iv.icns`: macOS high-resolution icon set.

The canonical geometry and colors are in `scripts/build_iv_icons.py`. With
Pillow installed, run that script from the repository root to regenerate all
formats together. Windows resource compilation runs automatically from Cargo;
Mac packaging copies the ICNS into the app Resources and sets CFBundleIconFile.

---

# KohdaLab IV アプリケーションアイコン

青と橙の曲線は、暗色の計測器表示上にある順方向・逆方向 I-V スイープを表します。プロジェクトの
MIT ライセンスはこれらのアセットにも適用されます。

- `kohdalab-iv.svg`: 編集可能なベクター出力。
- `kohdalab-iv.png`: 透過 1024 x 1024 のマスタープレビュー。
- `kohdalab-iv-window.png`: 埋め込み用 256 x 256 GUI アイコン。
- `kohdalab-iv.ico`: Windows の 16、24、32、48、64、128、256 ピクセルのアイコン画像。
- `kohdalab-iv.icns`: macOS 高解像度アイコンセット。

正規の形状と色は `scripts/build_iv_icons.py` にあります。Pillow をインストールした状態で
リポジトリルートからこのスクリプトを実行すると、すべての形式をまとめて再生成できます。
Windows リソースのコンパイルは Cargo から自動実行され、Mac のパッケージングは ICNS を
アプリの Resources へコピーして `CFBundleIconFile` を設定します。
