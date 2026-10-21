# Python / Notebook control

The application and instrument engine are Rust. This optional client uses Python
3.10+ and its standard library to control the installed or portable `iv-cli`
executable over JSON lines. It does not need PyVISA, NumPy, or the former Python
application.

## Install the client

Install the Windows or macOS KohdaLab IV application first. This package is only
a client: it never downloads, builds, or replaces the Rust measurement engine.

For a checked-out repository:

```text
python -m pip install ./bindings/python
uv pip install ./bindings/python
```

Install directly from the matching GitHub release:

```text
python -m pip install "kohdalab-iv-native @ git+https://github.com/woolen-yarn/kohdalab-iv.git@preview-v1.3.0-alpha.7#subdirectory=bindings/python"
uv add "kohdalab-iv-native @ git+https://github.com/woolen-yarn/kohdalab-iv.git@preview-v1.3.0-alpha.7#subdirectory=bindings/python"
```

The discovery enhancements below are included in v1.3.0-alpha.6 and newer.
Install the matching client from the release tag or its bundled `python-control`
folder. Release tags are frozen snapshots.

The client resolves `iv-cli` in this order:

1. The explicit `NativeIV(executable=...)` argument.
2. `KOHDALAB_IV_EXE`.
3. An `iv-cli` or `iv-cli.exe` already on `PATH`.
4. On Windows, the registered installer location (both 32-bit and 64-bit
   registry views), then the native Program Files installation.
5. On macOS, `~/Applications/KohdaLab IV.app`, then
   `/Applications/KohdaLab IV.app`.

Discovery only checks known paths. It does not scan hardware or recursively
search Downloads. Explicit arguments and `KOHDALAB_IV_EXE` take priority;
invalid overrides raise an error instead of silently selecting another app.
Use `resolve_executable()` to inspect the selected path without starting it.
For a portable installation elsewhere, pass the CLI path, its containing folder,
or the macOS `.app` folder. Keep the complete portable package together.
Close the GUI's hardware connections before using the same devices in a Notebook.

```python
from pathlib import Path
from kohdalab_iv_native import NativeIV

with NativeIV() as iv:  # Finds an installed Windows or macOS application automatically.
    iv.connect(simulation=True)
    print(iv.live())  # no output enabled, no CSV created
    result = iv.run(Path("example.csv"), {
        "start": -0.001, "stop": 0.001, "step": 0.001,
        "pattern": "linear",
    })
    print(result["points"], result["output"])
```

Use a new output filename for every run. Rust refuses to overwrite a CSV.
Configuration defaults are queried from Rust, rather than maintained separately
in Python. `snapshot` returns the latest state; `live()` waits for a new sample.
`stop()`, `output_off()` and `disconnect()` send the respective engine commands.
`run()` waits for completion and sends stop/OFF on timeout or interruption.
The context manager requests shutdown and waits for cleanup; it never kills a
hardware engine whose output cleanup is still pending.

For hardware, explicitly use `iv.connect("GPIB0::5::INSTR", "GPIB0::9::INSTR")`,
or the direct USB resource strings returned by `iv-cli --list`. Choose the actual
addresses and verify the permitted voltage/current limits before calling `run`.
A source-level reading is the programmed source value, not an independent output
measurement. Continuous acquisition configures the meter but does not enable
source output. During a sweep, the Rust engine serializes monitoring and sweep I/O.

Open [the simulation tutorial](examples/native-control.ipynb) in Jupyter.
No hardware commands are executed by that example. Direct CLI alternatives:

```text
iv-cli --default-config
iv-cli --list
iv-cli --measure --simulate --output example.csv
```

After connecting, `iv.set_mode("dc_vi")` selects source current / measure voltage;
`iv.set_mode("dc_iv")` selects source voltage / measure current. Rust verifies
output OFF and zero level before switching. Use these calls while idle.

## Development model selection

The 1.3 development engine accepts explicit models (both the client and engine must be the matching development version):

```python
iv.connect("GPIB0::3::INSTR", "GPIB0::7::INSTR",
           source_model="YOKOGAWA_7651", meter_model="ADCMT_7461A")
```

The client carries those models into each run. The engine rejects a run whose model settings disagree with the connected devices. 7461A GPIB uses SCPI; Windows official VISA USB uses ADC commands and stream reads. Native ADCMT USB and a separate GS200 driver are not implemented. Hardware validation of the newly ported models is pending.

## Move an existing Notebook

See [the migration tutorial](examples/migrate-to-native.ipynb). Keep analysis and
plotting cells; replace legacy Python instrument objects / PyVISA calls with one
`NativeIV` context. This is a new API, not a drop-in replacement. Rust owns
communication, limits, CSV persistence and output cleanup. `iv.live()` returns a
sample dictionary; `iv.run()` returns run status, while the saved CSV holds the
measurement rows. Use `start`, `stop` (the former End field), and `step` in SI
units, `settle_s` for wait time, `nplc` for integration time and `repeat` for
meter readings averaged at each unchanged source level. Query
`iv.default_config` for the complete current schema. Use the context manager
inside a single cell or function so interruptions pass through cleanup.

The package installation must target your Notebook kernel's Python environment.
In Jupyter, `%pip install /path/to/kohdalab-iv/bindings/python` installs this client
into the active kernel. Restart the kernel after upgrading an imported client.
No Python measurement engine or VISA dependency is installed by this package.

The client checks protocol compatibility from the engine's first normal state,
before connecting to instruments. Older engines without a declaration retain
the legacy v1 behavior. An incompatible declaration produces an English upgrade
message and requests engine shutdown. `iv.protocol` exposes the accepted contract;
`iv.snapshot["error"]` remains the original error text, while newer engines also
provide `error_kind` and `activity` for programmatic diagnostics. These additions
do not change the existing client methods or CSV columns.

---

# Python / Notebook 制御

アプリケーションおよび計測器エンジンは Rust です。この任意クライアントは Python 3.10+ と
標準ライブラリーを使用し、JSON lines でインストール済みまたはポータブルの `iv-cli` 実行ファイルを
制御します。PyVISA、NumPy、以前の Python アプリケーションは必要ありません。

## クライアントをインストールする

先に Windows または macOS の KohdaLab IV アプリケーションをインストールしてください。このパッケージは
クライアントのみであり、Rust 測定エンジンをダウンロード、ビルド、置換することはありません。

チェックアウトしたリポジトリでは次を実行します。

```text
python -m pip install ./bindings/python
uv pip install ./bindings/python
```

対応する GitHub リリースから直接インストールする場合は次を使います。

```text
python -m pip install "kohdalab-iv-native @ git+https://github.com/woolen-yarn/kohdalab-iv.git@preview-v1.3.0-alpha.7#subdirectory=bindings/python"
uv add "kohdalab-iv-native @ git+https://github.com/woolen-yarn/kohdalab-iv.git@preview-v1.3.0-alpha.7#subdirectory=bindings/python"
```

下記の検出機能拡張は v1.3.0-alpha.6 以降に含まれます。リリースタグまたは同梱の
`python-control` フォルダーから、対応するクライアントをインストールしてください。リリースタグは
固定されたスナップショットです。

クライアントは次の順序で `iv-cli` を解決します。

1. 明示した `NativeIV(executable=...)` 引数。
2. `KOHDALAB_IV_EXE`。
3. `PATH` 上にある `iv-cli` または `iv-cli.exe`。
4. Windows では、登録されたインストーラー場所（32-bit と 64-bit の両レジストリビュー）、次にネイティブ Program Files インストール。
5. macOS では、`~/Applications/KohdaLab IV.app`、次に `/Applications/KohdaLab IV.app`。

検出は既知のパスだけを確認します。ハードウェアをスキャンしたり Downloads を再帰検索したりしません。
明示的な引数と `KOHDALAB_IV_EXE` が優先されます。無効な指定は別のアプリケーションを黙って選ばず、
エラーになります。`resolve_executable()` は起動せずに選択パスを調べます。別の場所のポータブル
インストールには CLI パス、そのフォルダー、macOS の `.app` フォルダーを渡してください。完全な
ポータブルパッケージを一緒に保ちます。同じデバイスを Notebook で使う前に GUI のハードウェア接続を閉じます。

```python
from pathlib import Path
from kohdalab_iv_native import NativeIV

with NativeIV() as iv:  # インストール済み Windows または macOS アプリケーションを自動検出します。
    iv.connect(simulation=True)
    print(iv.live())  # 出力は有効化せず、CSV も作成しません。
    result = iv.run(Path("example.csv"), {
        "start": -0.001, "stop": 0.001, "step": 0.001,
        "pattern": "linear",
    })
    print(result["points"], result["output"])
```

実行ごとに新しい出力ファイル名を使ってください。Rust は CSV の上書きを拒否します。設定既定値は
Python 側で別管理せず Rust から問い合わせます。`snapshot` は最新状態を返し、`live()` は新しい
サンプルを待ちます。`stop()`、`output_off()`、`disconnect()` は対応するエンジンコマンドを送ります。
`run()` は完了を待ち、タイムアウトまたは割り込み時に stop/OFF を送ります。コンテキストマネージャーは
終了を要求してクリーンアップを待ち、出力クリーンアップが保留のハードウェアエンジンを kill しません。

実機では `iv.connect("GPIB0::5::INSTR", "GPIB0::9::INSTR")`、または `iv-cli --list` が返す直接 USB
リソース文字列を明示的に使います。実際のアドレスを選び、`run` の前に許容電圧／電流制限を確認してください。
ソースレベル読み値はプログラムされたソース値であり、独立した出力測定ではありません。連続取得は
メーターを設定しますが、ソース出力を有効化しません。スイープ中、Rust エンジンは監視とスイープ I/O を
直列化します。

Jupyter で[シミュレーションチュートリアル](examples/native-control.ipynb)を開いてください。この例は
ハードウェアコマンドを実行しません。直接 CLI の代替は以下です。

```text
iv-cli --default-config
iv-cli --list
iv-cli --measure --simulate --output example.csv
```

接続後、`iv.set_mode("dc_vi")` は電流源／電圧測定を、`iv.set_mode("dc_iv")` は電圧源／電流測定を
選択します。Rust は切替前に出力 OFF とゼロレベルを検証します。これらの呼び出しはアイドル時に使います。

## 開発版のモデル選択

1.3 開発エンジンは明示的なモデルを受け入れます（クライアントとエンジンの両方が対応する同一開発版である必要があります）。

```python
iv.connect("GPIB0::3::INSTR", "GPIB0::7::INSTR",
           source_model="YOKOGAWA_7651", meter_model="ADCMT_7461A")
```

クライアントはこれらのモデルを各実行に渡します。エンジンはモデル設定が接続デバイスと一致しない実行を
拒否します。7461A の GPIB は SCPI を使用し、Windows 公式 VISA USB は ADC コマンドとストリーム読み取りを
使用します。ネイティブ ADCMT USB と独立した GS200 ドライバーは実装していません。新たに移植したモデルの
ハードウェア検証は保留中です。

## 既存 Notebook を移行する

[移行チュートリアル](examples/migrate-to-native.ipynb)を参照してください。解析・プロットセルは保持し、
従来の Python 計測器オブジェクト／PyVISA 呼び出しを一つの `NativeIV` コンテキストに置き換えます。これは
新しい API であり、ドロップイン置換ではありません。Rust が通信、制限、CSV 永続化、出力クリーンアップを
所有します。`iv.live()` はサンプル辞書を、`iv.run()` は実行状態を返し、保存 CSV が測定行を保持します。
SI 単位の `start`、`stop`（旧 End フィールド）、`step`、待機時間の `settle_s`、積分時間の `nplc`、
同一ソースレベルで平均するメーター読み値の `repeat` を使います。現在の完全スキーマは
`iv.default_config` で問い合わせます。割り込みがクリーンアップを通るように、コンテキストマネージャーは
一つのセルまたは関数内で使ってください。

パッケージのインストール先は Notebook カーネルの Python 環境でなければなりません。Jupyter では
`%pip install /path/to/kohdalab-iv/bindings/python` によりこのクライアントをアクティブカーネルへ
インストールします。インポート済みクライアントを更新した後はカーネルを再起動します。このパッケージは
Python 測定エンジンや VISA 依存関係をインストールしません。

クライアントは計測器へ接続する前、エンジンの最初の通常状態からプロトコル互換性を確認します。宣言のない
古いエンジンは従来の v1 挙動を維持します。互換性のない宣言では英語の更新メッセージを出し、エンジンの
終了を要求します。`iv.protocol` は受け入れた契約を公開します。`iv.snapshot["error"]` は元のエラー文を
保持し、新しいエンジンはプログラム上の診断用に `error_kind` と `activity` も提供します。これらの追加は
既存のクライアントメソッドや CSV 列を変更しません。
