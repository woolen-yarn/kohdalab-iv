# Hardware and transport

The table lists model IDs, connection routes, and validation evidence for KohdaLab IV.
Physical validation requires measurements on instruments. Protocol tests use mock
devices.
A protocol implementation or successful mock test does not establish physical validation.
Both DC modes are available: source voltage / measure current and source current /
measure voltage. Model IDs are also used by the Notebook client.

| Instrument | Role / model ID | Direct USB | GPIB | Availability and validation |
| --- | --- | --- | --- | --- |
| Yokogawa GS210 | Source · `YOKOGAWA_GS210` | Control/communication mode | Yes | Earlier hardware operation recorded |
| Yokogawa 7651 | Source · `YOKOGAWA_7651` | No | Legacy protocol | Protocol tests; physical validation pending |
| Agilent 34401A | Meter · `AGILENT_34401A` | No | Yes | Protocol tests; physical validation pending |
| Agilent / Keysight 34411A | Meter · `AGILENT_34411A`, `KEYSIGHT_34411A` | USBTMC | Yes | Earlier hardware operation recorded |
| Keysight 34465A | Meter · `KEYSIGHT_34465A` | USBTMC | Yes | Protocol tests; physical validation pending; fast NPLC depends on options |
| ADCMT 7461A | Meter · `ADCMT_7461A` | Windows official 64-bit VISA, ADC mode (Auto only) | SCPI mode; DCV fixed 0.1 V/1 V and range readback | Protocol tests; GPIB DCV range commands checked on hardware with output OFF; sweep validation pending |
| Simulated source | Source · `SIMULATED_SOURCE` | Not needed | Not needed | Built-in, automated simulation tests |
| Simulated meter | Meter · `SIMULATED_METER` | Not needed | Not needed | Built-in, automated simulation tests |

GS210 USB must use communication mode, rather than storage mode. Native WinUSB
ADCMT USB and a separate GS200 driver are not implemented. Discovery and model
selection are separate: a discovered resource still needs a matching model.

| USB-GPIB adapter | Requirement |
| --- | --- |
| NI GPIB-USB-HS+ | Supported native helper or an available Windows official VISA runtime |
| Agilent / Keysight 82357B | Must already be initialized using separately authorized software/firmware |

The distribution never obtains or uploads 82357B firmware. Windows prefers an
available official 64-bit VISA runtime and otherwise uses supported native
WinUSB devices. macOS Apple Silicon uses the native USB/GPIB helpers. Multiple
simultaneous GPIB controllers are not supported. See
[instrument precision and limitations](instrument-precision.md).

---

# ハードウェアと転送

KohdaLab IV のモデル ID、接続経路、検証記録を表に示します。実機検証には
計測器での測定が必要です。プロトコルテストには模擬機器を使用します。
プロトコル実装またはモックテストの成功は、実機検証を意味しません。電圧源・電流測定と電流源・電圧測定の両方の DC モードを利用できます。
モデル ID は Notebook クライアントでも使用します。

| 計測器 | 役割／モデル ID | 直接 USB | GPIB | 対応状況と検証 |
| --- | --- | --- | --- | --- |
| Yokogawa GS210 | ソース・`YOKOGAWA_GS210` | 通信モード | はい | 過去の実機動作の記録あり |
| Yokogawa 7651 | ソース・`YOKOGAWA_7651` | いいえ | レガシープロトコル | プロトコルテスト済み、実機検証待ち |
| Agilent 34401A | メーター・`AGILENT_34401A` | いいえ | はい | プロトコルテスト済み、実機検証待ち |
| Agilent / Keysight 34411A | メーター・`AGILENT_34411A`, `KEYSIGHT_34411A` | USBTMC | はい | 過去の実機動作の記録あり |
| Keysight 34465A | メーター・`KEYSIGHT_34465A` | USBTMC | はい | プロトコルテスト済み、実機検証待ち。高速 NPLC はオプションに依存 |
| ADCMT 7461A | メーター・`ADCMT_7461A` | Windows 公式 64-bit VISA、ADC モード（Auto のみ） | SCPI モード、DCV 固定 0.1 V/1 V とレンジ読み返し | プロトコルテスト済み、GPIB の DCV レンジ命令は出力 OFF で実機確認済み、掃引の実機検証待ち |
| Simulated source | ソース・`SIMULATED_SOURCE` | 不要 | 不要 | 内蔵、自動シミュレーションテスト |
| Simulated meter | メーター・`SIMULATED_METER` | 不要 | 不要 | 内蔵、自動シミュレーションテスト |

GS210 USB はストレージモードではなく通信モードを使う必要があります。ネイティブ WinUSB による
ADCMT USB と、独立した GS200 ドライバーは実装していません。検出とモデル選択は別です。検出された
リソースにも、対応するモデルを選ぶ必要があります。

| USB-GPIB アダプター | 要件 |
| --- | --- |
| NI GPIB-USB-HS+ | サポート対象のネイティブヘルパー、または利用可能な Windows 公式 VISA ランタイム |
| Agilent / Keysight 82357B | 別途使用権が認められたソフトウェア／ファームウェアで、あらかじめ初期化済みであること |

本配布物が 82357B ファームウェアを取得またはアップロードすることはありません。Windows では利用可能な
公式 64-bit VISA ランタイムを優先し、それ以外では対応済みのネイティブ WinUSB デバイスを使います。
macOS Apple Silicon ではネイティブ USB/GPIB ヘルパーを使用します。複数の GPIB コントローラーを
同時に使用することはできません。[計測器精度と制約](instrument-precision.md)を参照してください。
