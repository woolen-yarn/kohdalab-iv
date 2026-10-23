# Hardware and transport

Stable **v1.2.0** supports GS210 and Agilent/Keysight 34411A. The current
**1.3 development source** also implements the additional models below. A protocol
implementation or successful mock test does not establish physical validation.
Both DC modes are available: source voltage / measure current and source current /
measure voltage. Model IDs are also used by the Notebook client.

| Instrument | Role / model ID | Direct USB | GPIB | Availability and validation |
| --- | --- | --- | --- | --- |
| Yokogawa GS210 | Source · `YOKOGAWA_GS210` | Control/communication mode | Yes | Stable + development; previous hardware operation retained |
| Yokogawa 7651 | Source · `YOKOGAWA_7651` | No | Legacy protocol | Development; protocol tests, physical validation pending |
| Agilent 34401A | Meter · `AGILENT_34401A` | No | Yes | Development; protocol tests, physical validation pending |
| Agilent / Keysight 34411A | Meter · `AGILENT_34411A`, `KEYSIGHT_34411A` | USBTMC | Yes | Stable + development; previous hardware operation retained |
| Keysight 34465A | Meter · `KEYSIGHT_34465A` | USBTMC | Yes | Development; protocol tests, physical validation pending; fast NPLC depends on options |
| ADCMT 7461A | Meter · `ADCMT_7461A` | Windows official 64-bit VISA, ADC mode | SCPI mode | Development; protocol tests, physical validation pending |
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

安定版 **v1.2.0** は GS210 と Agilent/Keysight 34411A をサポートします。現在の
**1.3 開発ソース**は、下記の追加モデルも実装しています。プロトコル実装またはモックテストの
成功は、実機検証を意味しません。電圧源・電流測定と電流源・電圧測定の両方の DC モードを利用できます。
モデル ID は Notebook クライアントでも使用します。

| 計測器 | 役割／モデル ID | 直接 USB | GPIB | 対応状況と検証 |
| --- | --- | --- | --- | --- |
| Yokogawa GS210 | ソース・`YOKOGAWA_GS210` | 通信モード | はい | 安定版＋開発版。従来の実機動作を維持 |
| Yokogawa 7651 | ソース・`YOKOGAWA_7651` | いいえ | レガシープロトコル | 開発版。プロトコルテスト済み、実機検証待ち |
| Agilent 34401A | メーター・`AGILENT_34401A` | いいえ | はい | 開発版。プロトコルテスト済み、実機検証待ち |
| Agilent / Keysight 34411A | メーター・`AGILENT_34411A`, `KEYSIGHT_34411A` | USBTMC | はい | 安定版＋開発版。従来の実機動作を維持 |
| Keysight 34465A | メーター・`KEYSIGHT_34465A` | USBTMC | はい | 開発版。プロトコルテスト済み、実機検証待ち。高速 NPLC はオプションに依存 |
| ADCMT 7461A | メーター・`ADCMT_7461A` | Windows 公式 64-bit VISA、ADC モード | SCPI モード | 開発版。プロトコルテスト済み、実機検証待ち |
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
