# Legacy instrument port (development) / レガシー計測器移植（開発）

> **日本語の案内:** この記録は旧実装からの移植範囲を示します。モデル ID と互換性条件は
> 翻訳せず原文のまま使用してください。

This port uses the working Python drivers in git revision `b754cfa^` as its protocol baseline. It does not restore the old Python measurement application. The single Rust engine serves the desktop, CLI and optional Python/Notebook client.

## Implementation and validation

- GS210 / 34411A retain their existing command sequences.
- 7651 identifies with `OS;E`, drains the panel response through `END`, and uses `F1` / `F5`, integer range codes, `LA` in mA and `LV` in V. Programmed levels stay in SI units. `OC;E` bit 4 verifies output state; output OFF is sent before zeroing. It never receives GS210 SCPI output commands.
- 34401A uses its legacy SCPI configuration and GPIB local control; no `SYST:LOC` is sent.
- 34465A uses the 34411A-family SCPI sequence and reported range/resolution. Faster NPLC values can require options or firmware; a configuration error stops the run.
- 7461A GPIB requires SCPI command mode. Requested NPLC maps to FAST (<=0.02), MED (<=0.2), SLOW (<=1) or SSLOW; this is a sampling-rate policy, not confirmation of exact requested integration time. USB requires official Windows 64-bit VISA and ADC commands, with `H0`, `F1`/`F5`, `R0`, `ITP`, delayed raw stream reads, and delayed `ERR?` readback. A reading is discarded after each source change before accepting the next sample. Unknown measurement resolution stays unknown; no panel precision is invented.

Model identity is checked before configuration. The connected models must agree with the run models. Range, source resolution, hardware limit, software limit, meter input limit and NPLC are checked before creating the run and enabling output. Legacy source range codes are kept distinct from physical SI ranges when choosing ramp increments.

Protocol fixture tests cover model mismatch, 7651 limit units, model/range queries, output on/off verification and zeroing, ADC SCPI setup, USB ADC setup, delayed status reads, stream read parsing, and discarded conversions. They do not establish physical output accuracy, front-panel parity, cable behavior or every firmware variant.

**Physical validation of the newly ported models is pending.** The released v1.2.0 artifacts are unchanged. GS200 has no separate historical driver and is not newly enabled. 34401A/7651 use GPIB; 34465A USBTMC support is a new extension validated by transport/setup tests only. Native WinUSB access to ADCMT USB is not implemented.

## Automation

Connection messages accept `source_model` and `meter_model`; per-role messages accept `model`. Missing fields retain GS210/34411A defaults for old clients. Run settings carry the same model keys. The Python client forwards its chosen models into runs.

```python
with NativeIV(executable="/path/to/development/iv-cli") as iv:
    iv.connect("GPIB0::3::INSTR", "GPIB0::7::INSTR",
               source_model="YOKOGAWA_7651", meter_model="AGILENT_34401A")
```

Close any GUI holding those devices first. Connection enables monitoring, not source output. Choose permitted limits and verify hardware configuration before a real sweep.

CLI identification and measurement accept `--source-model YOKOGAWA_7651 --meter-model AGILENT_34401A` with the normal resource arguments. Identification still performs safe cleanup before releasing the devices.

---

# レガシー計測器移植（開発）

この移植は git revision `b754cfa^` の動作中 Python drivers を protocol baseline とします。古い Python measurement application を復元するものではありません。単一 Rust engine が desktop、CLI、任意 Python/Notebook client を提供します。

## 実装と検証

- GS210 / 34411A は既存 command sequences を保持します。
- 7651 は `OS;E` で識別し、panel response を `END` まで drain し、`F1` / `F5`、integer range codes、mA の `LA`、V の `LV` を使います。programmed levels は SI units のままです。`OC;E` bit 4 が output state を検証し、zeroing 前に output OFF を送ります。GS210 SCPI output commands を受けることはありません。
- 34401A は legacy SCPI configuration と GPIB local control を使い、`SYST:LOC` は送信しません。
- 34465A は 34411A-family SCPI sequence と報告された range/resolution を使います。高速 NPLC は options/firmware を要する場合があり、configuration error は run を停止します。
- 7461A GPIB は SCPI command mode が必要です。requested NPLC は FAST (<=0.02)、MED (<=0.2)、SLOW (<=1)、SSLOW へ対応し、これは exact requested integration time の確認ではなく sampling-rate policy です。USB には official Windows 64-bit VISA と ADC commands、`H0`、`F1`/`F5`、`R0`、`ITP`、delayed raw stream reads、delayed `ERR?` readback が必要です。各 source change 後に一読値を破棄して次 sample を受け入れます。unknown measurement resolution は unknown のままとし panel precision を作りません。

model identity は configuration 前に確認します。connected models は run models と一致する必要があります。range、source resolution、hardware limit、software limit、meter input limit、NPLC は run 作成と output 有効化前に確認します。ramp increments を選ぶ際、legacy source range codes は physical SI ranges と区別します。protocol fixture tests は model mismatch、7651 limit units、model/range queries、output on/off verification と zeroing、ADC SCPI setup、USB ADC setup、delayed status reads、stream read parsing、discarded conversions を扱います。physical output accuracy、front-panel parity、cable behavior、全 firmware variants を確立するものではありません。

**新規移植モデルの physical validation は保留中です。** リリース済み v1.2.0 artifacts は変わりません。GS200 には独立 historical driver がなく新規有効化もしません。34401A/7651 は GPIB を使い、34465A USBTMC support は transport/setup tests のみで検証した新しい extension です。ADCMT USB への native WinUSB access は未実装です。

## 自動化

connection messages は `source_model` と `meter_model`、role ごとの messages は `model` を受け入れます。fields がない場合、old clients に対して GS210/34411A defaults を保持します。run settings は同じ model keys を保持し、Python client は選んだ models を runs に渡します。

```python
with NativeIV(executable="/path/to/development/iv-cli") as iv:
    iv.connect("GPIB0::3::INSTR", "GPIB0::7::INSTR",
               source_model="YOKOGAWA_7651", meter_model="AGILENT_34401A")
```

先にこれら devices を保持する GUI を閉じてください。connection は monitoring を有効にしますが source output は有効にしません。real sweep 前に permitted limits を選び hardware configuration を確認します。CLI identification と measurement は通常 resource arguments と共に `--source-model YOKOGAWA_7651 --meter-model AGILENT_34401A` を受け入れます。identification は devices を解放する前に引き続き safe cleanup を行います。
