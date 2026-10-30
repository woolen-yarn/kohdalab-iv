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
python -m pip install "kohdalab-iv-native @ git+https://github.com/woolen-yarn/kohdalab-iv.git@preview-v1.3.0-alpha.17#subdirectory=bindings/python"
uv add "kohdalab-iv-native @ git+https://github.com/woolen-yarn/kohdalab-iv.git@preview-v1.3.0-alpha.17#subdirectory=bindings/python"
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

Use a new output filename for every run. Rust refuses to overwrite a CSV. Each
new run also creates `.run.json` and `.raw.csv`; keep all three files
together. The 26-column `analysis_v4` CSV preserves finite `f64` values,
accepted-reading counts, ddof=1 `meter_stddev`, and explicit quality flags; the raw CSV
retains raw per-repeat replies and known provenance. Neither representation
turns instrument resolution into an uncertainty claim.
`compliance` is the external Meter software stop (A in `dc_iv`, V in `dc_vi`).
`source_compliance` independently requests Source hardware protection in the same
units. For example, `{"source_compliance": 5.0, "compliance": 0.2}` in `dc_vi`
requests 5 V source protection and a 0.2 V external Meter stop. These example
limits are not a recommendation for any sample. Omit/null `source_compliance`
to retain legacy coupled behavior. The engine reports actual range/protection in
run metadata; GS210 low-voltage ranges have fixed 200 mA hardware protection.
An explicitly smaller Source current limit selects a compatible range, which can
change target resolution. A GS210 H/L trip returns `Source limit stop`; its
limited target is retained only as raw evidence, not as an analysis point.

Configuration defaults are queried from Rust, rather than maintained separately
in Python. `snapshot` returns the latest state; `live()` waits for a new sample.
`meter_voltage_range` and `meter_current_range` select fixed SI ranges; `None`
means Auto. They are independent and retained in run metadata. See
[Meter ranging](../../docs/reference/meter-ranging.md) for supported presets,
an example, and compatibility checks.
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

## Fixed gate bias and balanced IV

Recent engines can keep a fixed gate source on while two IV sources sweep an
equal-and-opposite differential voltage. All devices remain in one Rust engine
session, including shared GPIB adapters; do not create separate `NativeIV`
sessions for the gate or second source.

```python
with NativeIV() as iv:
    iv.connect(simulation=True)             # Source 1 + meter
    iv.connect_source2(simulation=True)     # Source 2
    iv.connect_gate(simulation=True)        # fixed gate source, initially OFF
    iv.set_gate_voltage(0.10, compliance=1e-3, ramp_step=1e-3)
    result = iv.run("balanced.csv", {
        "mode": "dc_iv", "start": -1e-3, "stop": 1e-3, "step": 1e-3,
        "source_compliance": 1e-3,
        "balanced_source": {
            "source_model": "YOKOGAWA_GS210", "source_compliance": 1e-3,
        },
    })
    iv.gate_output_off()
```

For a differential target `D`, Rust plans Source 1 as `D / 2` and Source 2 as
`-D / 2`. It validates both hardware grids before enabling either sweep output.
Gate output remains on during the IV run and must be explicitly turned off when
the bias is no longer needed; context shutdown also requests gate cleanup.
`set_gate_voltage` rejects a live compliance or range change rather than
cycling an enabled gate. Use only explicit, sample-approved current
compliances. The gate is voltage-source only and this client does not infer a
gate leakage current without an independent meter or `/MON` connection.

Open [the gate and balanced-IV simulation tutorial](examples/gate-balanced-control.ipynb)
for a complete no-hardware example.

## Explicit model selection

KohdaLab IV 1.4.0 accepts explicit models. Keep the bundled client and
application at compatible release versions:

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
python -m pip install "kohdalab-iv-native @ git+https://github.com/woolen-yarn/kohdalab-iv.git@preview-v1.3.0-alpha.17#subdirectory=bindings/python"
uv add "kohdalab-iv-native @ git+https://github.com/woolen-yarn/kohdalab-iv.git@preview-v1.3.0-alpha.17#subdirectory=bindings/python"
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

実行ごとに新しい出力ファイル名を使ってください。Rust は CSV の上書きを拒否します。新しいrunでは
`.run.json`と`.raw.csv`も作成するため、3ファイルを一緒に保管してください。26列の`analysis_v4` CSVは有限`f64`値、
accepted reading count、ddof=1の`meter_stddev`、quality flagを保持し、raw CSVは
uncertaintyの主張には変えません。設定既定値は
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

## Control the GUI already on screen

`GuiIV` attaches to the running GUI instead of starting another engine. Reading
`status()`, `config()`, `plan()` and `output()` does not take control. A
measurement-changing call, or entering the context manager, acquires an
exclusive Python lease while the GUI is idle. The GUI shows that ownership and
can take control back; a revoked client cannot silently reacquire it.

Select and connect the IV instruments in the GUI, then use its existing sweep settings:

```python
from kohdalab_iv_native import GuiIV

with GuiIV() as iv:
    # Change external parameters / wait for stability in your own code.
    result = iv.run(filename="field_0T_temperature_4K")
    print(result["path"])
```

The GUI displays the plot and saves the CSV normally. `run` raises on stopped,
limited or failed measurements. `stop()` and the GUI Stop button interrupt a run.
Gate source methods are also available on `GuiIV`; a normal run keeps the gate
bias while context exit turns outputs off. No external-parameter GUI is added.
See [dual-source and GUI control](../../docs/reference/dual-source-and-gui-control.md)
for wiring semantics, integer ratios, safety behavior and a gate-bias loop.

For automated experiments, the canonical API follows the same basic vocabulary
as KohdaLab TRKR: `acquire`, `release`, `start`, `result`, `wait`, and `stop`.
`start()`, `connect()`, and `disconnect()` accept an optional UUID `request_id`
and return identified job receipts; repeat identical calls with the same UUID are idempotent. Reusing a UUID with a different method or payload is rejected.
`configure`, `load_config`,
`set_plan`, `set_output`, `connect`, `disconnect`, and gate commands are also
available after acquiring control. `GuiIV.launch(profile=...)` starts the
installed GUI with its API enabled and attaches only to the descriptor matching
that new process.

By default, IV deliberately differs from TRKR when control is lost: IV's lease timeout,
`close()`, and `release()` request Stop and verified Output OFF before releasing
the lease. This preserves IV's source-output safety policy. A Notebook should
use `with GuiIV()` or explicitly call `close()`; a discarded client stops sending
heartbeats, so the GUI's timeout policy still takes effect.

See the [GUI API reference](../../docs/reference/gui-api.md) for receipt fields,
profile loading, safety policy, error codes and executable simulation checks.


## GUI stop conditions / GUIの停止条件

`GuiIV(...)` and `GuiIV.launch(...)` accept the same three independent boolean options as TRKR. IV keeps its source safety defaults: **all three are True**. TRKR defaults to False for all three, preserving continuation. Never change these defaults implicitly when attaching to an existing GUI: the client sends its selected policy when it acquires exclusive control. A nondefault policy is rejected before any job starts if an older GUI cannot confirm it.

`GuiIV(...)`と`GuiIV.launch(...)`はTRKRと同じ3つの停止条件を個別に受け付けます。**IVはすべてTrue、TRKRはすべてFalseが既定**です。IVの電源出力の保護とTRKRの継続動作を維持します。取得した制御権に条件を紐付け、旧GUIが既定以外の条件に対応していなければ測定開始前に拒否します。

| Option / 設定 | Trigger / 発動条件 |
| --- | --- |
| `stop_on_disconnect` | Heartbeat expires in the GUI (about 10 s), or a mutating convenience operation loses its acknowledgement / GUI側のheartbeat失効（約10秒）や操作の応答消失 |
| `stop_on_error` | Python exception leaves the `with` block; Python-side interruption of convenience operations / `with`ブロックからPython例外が抜ける、または便利メソッドのPython側中断 |
| `stop_on_timeout` | `wait()` / `run()` / Gate helper wait reaches its deadline / 測定・Gate操作の待機時間切れ |

```python
from kohdalab_iv_native import GuiIV, GuiTimeoutError

with GuiIV(stop_on_disconnect=True, stop_on_error=True,
           stop_on_timeout=False) as iv:
    iv.set_output(directory="D:/IV/data", filename="field_0.5T")
    job = iv.start()
    try:
        result = iv.wait(job["id"], timeout=600)
    except GuiTimeoutError:
        result = iv.stop_and_wait(job["id"], timeout=40)
```

`start()` → `wait(id)` is the shared receipt-based flow. The existing IV `run()` convenience method still returns a Snapshot for compatibility. `stop(id)` acknowledges the request and returns the current receipt; it does not confirm shutdown. `stop_and_wait(id, timeout=40, poll_interval=0.1)` returns a completed/stopped receipt after worker cleanup and, for a job it stopped, verifies **all connected IV outputs OFF**, including Gate. An already terminal job is returned only after read-only Output OFF confirmation, without sending another Stop or changing its outputs. If another operation or retained Gate bias is ON, confirmation times out rather than silently turning it off. A failed cleanup remains an error, even if a Stop acknowledgement arrived. Ordinary manual Stop retains its existing Gate-bias behavior.

共通の処理結果を使う流れは`start()`→`wait(id)`です。IVの既存`run()`は互換性のためSnapshotを返します。`stop(id)`は受付と現在の処理結果を返すだけで、停止確認ではありません。`stop_and_wait()`は終了処理を待ち、今回停止した処理については**Gateを含む接続済み全出力のOFFを確認**します。終了済みの処理でも読取によるOFF確認を行い、Stopの再送・出力の変更はしません。保持されたGateバイアスや別処理の出力がONなら、勝手にOFFにせず確認タイムアウトを返します。停止確認に失敗した場合は、Stop受付済みでもエラーです。通常のGUI StopでGateバイアスを保持する動作は変えません。

IV `close()` / `release()` and a **normal** `with` exit always verify Output OFF, independently of these flags. To let an active job continue after a Python exception, explicitly disable **both** `stop_on_error` and `stop_on_disconnect`; exception exit then detaches through the GUI and retains its edit lock until the active job completes. If either condition is enabled, cleanup takes priority. Python exit never closes the GUI. Device protection and save-error cleanup are never disabled by these options. No policy automatically retries a mutation, reacquires revoked ownership, or restarts a measurement. The GUI lease timer requires a responsive GUI; it is not a hardware interlock.

IVの`close()`・`release()`・**正常な**`with`終了では、フラグと独立してOutput OFFを確認します。Python例外時にも測定を継続させる場合は、`stop_on_error`と`stop_on_disconnect`を**両方False**にします。その場合はGUIに切離しを通知し、処理完了までGUIの編集ロックを保ちます。いずれかがTrueなら終了処理を優先します。Pythonを終了してもGUIは閉じません。装置保護・保存異常時の終了処理は無効化できません。操作の自動再試行・制御権の再取得・測定再開は行いません。

All GUI exceptions inherit `GuiError`, the shared alias of the existing `GuiIVError` (`RuntimeError`). They expose `.code` and `.receipt`. `GuiTimeoutError` also inherits `TimeoutError`: `.receipt` is the last observed state, and `.stop_error` preserves any failed automatic Stop/OFF request. Timeout/transport loss never proves that the physical outputs are OFF. Inspect the GUI and instruments if confirmation fails.

すべてのGUI例外は`GuiError`（既存`GuiIVError`の共通別名、`RuntimeError`派生）から派生し、`.code`と`.receipt`を持ちます。`GuiTimeoutError`は`TimeoutError`でもあり、最後の状態と自動Stop/OFF要求の失敗原因`.stop_error`を保持します。タイムアウト・通信断だけでは装置出力のOFFは確認できません。

| Exception / 例外 | Shared code / 共通コード |
| --- | --- |
| `GuiTransportError` | `transport`, `protocol` |
| `GuiOwnershipError` | `ownership_lost` |
| `GuiDeviceError` | `device_error` |
| `GuiProtectionError` | `protection` |
| `GuiSaveError` | `save_failed` |
| `GuiInitializationError` | `initialization_failed` |
| `GuiRunStopped` | `stopped` |
| `GuiTimeoutError` | `timeout` |
| `GuiError` / `GuiIVError` | Unknown/unclassified codes, including `job_failed` / その他・未分類 |

Legacy IV codes remain readable aliases. Error messages and original recording evidence are retained; exception classes are selected from codes, not Python text matching. Existing engine diagnostic categories remain diagnostic, not a decision to bypass protection. CSV columns and saved JSON configuration schemas are unchanged.

旧IVコードも別名として読み取れます。例外型はコードから選択し、Pythonで文面推測はしません。既存エンジンの診断分類は保護を迂回する判定には使いません。CSV列・保存済みJSONのschemaは変更しません。


KohdaLab IV 1.4.0 separates Source settings from optional measured V/I;
see [instrument assignment and CSV fields](../../docs/guides/source-and-meter-values.md).
Use the bundled client from the same release. It reads `analysis_v4` and legacy
`analysis_v3`; older applications and clients retain their documented schema.
