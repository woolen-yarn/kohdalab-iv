# Manual DC output

Each Source card has a **DC output** disclosure beside **Device**. Closed controls use
the existing header row; the measurement panel and Run / Save controls retain
their layout. Opening DC reveals the level, unit, source compliance and ramp
step. A DC ON badge identifies a fixed output independently of the sweep state.
The editor has its own height-limited scroll viewport; Output ON/OFF stays
outside it, and the live plots occupy separate layout space between Device and the settings.
The active Meter card precedes additional fixed-bias cards, so opening a gate
editor does not insert it between the main Source and Meter live monitors.

## Per-digit adjustment

The DC level has one up/down pair for each displayed decimal place, plus a
separate polarity button. Carry and borrow are automatic. While output is ON,
a valid digit adjustment is applied immediately using the configured ramp; no
extra Apply click is required. While OFF, the same controls edit the requested
level without turning output on. Output ON always remains explicit.

Select a hardware range using the left/right buttons around its central display. The range is manual
and remains fixed; entering a level never selects another range. The available ranges come from
the selected instrument profile. GS200/GS210 voltage ranges are 10 mV, 100 mV,
1 V, 10 V and 30 V; current ranges are 1 mA, 10 mA, 100 mA and 200 mA.
Range, function and compliance changes are locked while output is ON.

The digits use the selected range's maximum and resolution. For example, the
10 mV range has 100 nV increments and four decimals in mV; 100 mV has 1 µV
increments and three decimals in mV; 1 V has 10 µV increments and five decimals
in V. The 1 mA range has 10 nA increments and five decimals in mA. The display unit is fixed by the selected range: voltage ranges below 1 V use
mV, other voltage ranges use V, and the supported current ranges use mA.
There is no independent unit dropdown. The layout stays fixed within a range and
never switches to exponential notation. Out-of-range or sub-resolution values
are rejected, not rounded into another output. If an OFF editor's level is
incompatible with a newly selected range, it resets to zero.

The 10/100 mV divider ranges have a fixed 200 mA current limit and approximately
2 ohms output resistance. The app rejects enabling one of these ranges with a
lower requested Compliance; select at least the 1 V range or explicitly choose
200 mA. It never raises Compliance automatically.
See [GS200 User’s Manual, sections 2.3, 5.2 and 5.3](https://cdn.tmi.yokogawa.com/1/6218/files/IMGS210-01EN.pdf).

One **Output** toggle shows the verified ON/OFF state. It lights amber only when
ON is confirmed. Unknown output is red and offers only OFF. Clicking ON enables
the requested level; clicking the lit control requests priority OFF.
**Zero**, alongside Output, ramps an enabled source to zero on the same range
and keeps output ON. While OFF or disconnected it only zeros the editor and
never enables output. It is locked during sweeps or pending DC operations.

The optional JSON command field `range` is the native range command value, such
as `1.0` for GS210's 1 V range. GUI requests always supply it. Existing API calls
that omit it retain their previous automatic-planning behavior. An explicit
range cannot be changed while output is ON.

Click a digit or Type… for direct entry. Set validates the value against the
source grid before committing. Digit actions are disabled while typing, and a
verified level or selected range change invalidates the old text buffer. During a
pending source update or sweep, setting changes remain locked; the application
does not accumulate a queue of future levels. OFF retains its priority path.

## An IV source before a sweep

Connect the selected source, open DC, set its function/level/protection and press
**Output OFF** control to switch it ON. Digit or typed commits ramp an enabled
output directly; no Apply button is needed. Turn output OFF before changing
range, function or compliance. Source 1
supports voltage and current output through the existing instrument drivers;
the second source in Dual V-I uses voltage output.

Manual DC does not create a measurement CSV. Start remains available when the
instruments and sweep settings are valid. On Start the measurement engine takes
ownership of the sweep source, verifies OFF and zero, configures the planned
sweep and runs the ordinary measurement. DC changes are locked until that
operation completes; they are never queued to execute afterward. The normal
sweep completion policy turns the sweep sources OFF. It does not restore the
previous manual DC level automatically.

## A separate fixed gate bias

Add a Source in the left panel, choose its model/resource under Device, then
open DC and connect it. A source not assigned to the sweep can use the existing
independent voltage-bias connection. There is one independent bias connection,
in addition to the one or two IV sources. It is a separate instrument, not a
second connection to a source already owned by the sweep.

Click the Output toggle to enable the chosen gate voltage. Its card stays visibly active
and shows the source level even when the DC disclosure is closed. The bias is
held through successful IV sweeps and ordinary manual sweep Stop. Configuration
changes are locked during measurement; change gate bias between runs. Click the
lit Output toggle for OFF, or Disconnect explicitly when finished.

## Safety and saved settings

OFF is a priority safety request. During an active sweep or output ramp it
cancels that operation; it may trigger the existing all-output safety cleanup.
A later ON/Start request cannot clear a pending OFF or run afterward unnoticed.
An idle source-specific OFF preserves other healthy fixed outputs. A source
configuration, communication or protection fault uses the fail-closed shutdown
policy, including the independent gate. An unverified OFF is reported as unknown,
not as successfully disabled.

### Sweep and gate shutdown / スイープ用ソースとゲートの停止

On normal completion or Stop, the engine ramps the sweep source toward zero
before requesting and verifying Output OFF. In balanced mode, each step
commands both sources in sequence, using the configured ratio, before the next
step. OFF is verified for both sources. Protection trips, communication errors
and emergency OFF requests skip the ramp.

The zero-return budget is 10 seconds. At the cutoff the engine proceeds to OFF
and readback, shows a warning, and saves the last programmed levels in
`zero_return_timeout` in the run JSON and in the run log. The ramp budget
prevents new level writes after the cutoff. An in-flight instrument command
still uses its transport timeout.

A command error, including input validation, requests immediate OFF for a held
gate. If the gate’s `:OUTP?` poll fails while idle, the engine attempts gate
OFF and reports the verified result as `Off` or `Unknown`.

正常終了または通常の Stop では、スイープ用ソースをゼロへ戻してから Output OFF を要求し、
読み戻して確認します。balanced 測定では、設定した比率に従って各ステップで 2 台へ順に指令し、
両方の OFF を確認します。保護作動・通信エラー・緊急 OFF ではゼロ戻しを省略します。

ゼロ戻しの上限は 10 秒です。打ち切った場合も OFF を要求して読み戻し、画面に警告を表示します。
最後に指令したレベルを run JSON の `zero_return_timeout` と run ログに記録します。
打ち切り後は新たなレベルを書き込みません。送信済みの機器コマンドには転送タイムアウトが適用されます。

入力検証を含むコマンドエラーでは、保持中のゲート出力に即時 OFF を要求します。
待機中のゲートの `:OUTP?` が失敗した場合もゲート OFF を試み、読み戻しの結果に応じて
`Off` または `Unknown` を報告します。

### Saved settings / 保存設定

Saved profiles contain requested DC values and units, never an instruction to
reconnect or re-enable an output. Loading a profile or restarting the application
does not turn DC ON. Disconnect before rebinding/removing a connected bias card.
Source levels are programmed levels/readbacks, not independent terminal-voltage
measurements. Existing hardware compliance and sample-safe limits still apply.

The GUI and Python clients share the same Rust engine and its device ownership.
Python ownership locks ordinary GUI edits; the existing GUI Stop / Take control
path remains available. Measurement CSV and raw CSV columns are unchanged.
