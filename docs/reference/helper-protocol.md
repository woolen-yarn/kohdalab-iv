# Standalone GPIB helper interface / 独立 GPIB ヘルパーインターフェース

> **日本語の案内:** フレーム、バイト列、コマンド名、終了条件はプロトコル契約です。実装時は
> 英語本文の正確な表記を変更せず使用してください。

The native GPIB utility is a separate executable licensed under GPLv2. It owns
USB/GPIB bus operations and accepts generic byte-oriented commands from any
client, including a shell or a client unrelated to KohdaLab IV. It does not
implement the GUI, sweep planning, CSV schema, plotting or application IPC.

`--available` reports adapter presence without opening it. `--idn ADDRESS...`
opens the adapter and sends `*IDN?`; this performs controller initialization
and is not a passive USB-descriptor read. Without arguments it opens the
adapter and emits a `KIV\tOK\t` startup line, then accepts one request per line:

```text
OPERATION ADDRESS TIMEOUT_MS HEX_PAYLOAD
```

ADDRESS is 1-30 and TIMEOUT_MS is 1-3600000. Empty payload is `-`; otherwise
bytes are hexadecimal pairs. Responses are `KIV<TAB>OK<TAB>HEX_BYTES` or
`KIV<TAB>ERR<TAB>HEX_ERROR_TEXT`. ASCII commands such as `*IDN?` are caller-owned
bytes; no source-language objects or shared memory cross the process boundary.

| Operation | Behavior |
| --- | --- |
| WRITE | Send caller-provided command bytes |
| QUERY | Send bytes, then read the instrument response |
| READ | Read pending instrument bytes |
| LIST | Discover primary listeners and return resource strings |
| LOCAL | Return the addressed instrument to local control |
| CLEAR | Addressed device clear |
| REN | Payload `30` (ASCII `0`) releases remote enable |
| EXIT | Release remote enable, close adapter and exit |

Do not experiment with WRITE/CLEAR on a connected source without understanding
the instrument command's effects. The Rust client adds timeouts, exclusive
device leases and output cleanup, but those are client behavior rather than
the helper's licensing boundary. A compatible implementation can be selected
through `KOHDALAB_IV_HELPER_DIR` by providing a compatible
`kohdalab-gpib-helper[.exe]` alongside the USBTMC helper. All helper source/build instructions are in
the distribution's `support/SOURCE.zip`.

## Optional Windows official VISA helper

`kohdalab-visa-helper.exe` is a separate MIT helper. It dynamically loads the
system-installed 64-bit VISA runtime from System32; no vendor DLL/SDK is shipped.
`--available` returns 1/0. `--list` returns a KIV payload of canonical USB0/GPIB0
resource strings (optional tab-separated labels). Missing VISA returns an empty
successful list. A single resource argument opens an exclusive VISA session and
accepts the same bounded KIV byte protocol (address 0 for this single endpoint).
Discovery/opening sends no SCPI. GPIB LOCAL/close uses addressed GTL, avoiding a
bus-wide REN release that could affect another VISA session. The Rust parent
retains command deadlines, process isolation, identity checks and safe shutdown.
The GUI resource prefix `VISA::` selects this helper and is stripped before
calling the vendor API. Once selected, no failed I/O is replayed on WinUSB.

## 日本語 — 全文対訳

ネイティブGPIB utilityはGPLv2の独立実行ファイルです。USB/GPIBバス操作を所有し、shellやKohdaLab IVと無関係なクライアントを含む任意の利用側から、汎用バイト命令を受け付けます。GUI、スイープ計画、CSV形式、描画、アプリIPCは実装しません。

`--available`は開かずにアダプターの有無を報告します。`--idn ADDRESS...`はアダプターを開いて`*IDN?`を送り、コントローラーを初期化するため、受動的USB記述子読取ではありません。引数なしではアダプターを開き、`KIV\tOK\t`開始行を出した後、1行1要求を受け付けます。

```text
OPERATION ADDRESS TIMEOUT_MS HEX_PAYLOAD
```

ADDRESSは1–30、TIMEOUT_MSは1–3600000です。空payloadは`-`、それ以外はバイトを16進2桁ずつ表します。応答は`KIV<TAB>OK<TAB>HEX_BYTES`または`KIV<TAB>ERR<TAB>HEX_ERROR_TEXT`です。`*IDN?`等のASCIIも呼出側のバイト列で、言語オブジェクトや共有メモリーはプロセス境界を越えません。

| 操作 | 挙動 |
| --- | --- |
| WRITE | 呼出側の命令バイトを送る |
| QUERY | 送信後に機器応答を読む |
| READ | 未読の機器バイトを読む |
| LIST | 一次listenerを探索しリソース文字列を返す |
| LOCAL | 指定機器をlocalへ戻す |
| CLEAR | 指定機器のdevice clear |
| REN | payload `30`（ASCII `0`）でremote enableを解除 |
| EXIT | remote enable解除、アダプターclose、終了 |

接続中の電源に対し、効果を理解せずWRITE/CLEARを試さないでください。Rust側はtimeout、排他的device lease、出力後処理を追加しますが、これはclientの挙動でありhelperのライセンス境界ではありません。`KOHDALAB_IV_HELPER_DIR`に、USBTMC helperと互換な`kohdalab-gpib-helper[.exe]`を揃えて別実装を選べます。全helperソースとビルド手順は配布の`support/SOURCE.zip`にあります。

### 任意のWindows公式VISA helper

`kohdalab-visa-helper.exe`は独立したMIT helperです。System32の64-bitシステムVISAを動的に読み込み、ベンダーDLL・SDKは配布しません。`--available`は1/0、`--list`はcanonical USB0/GPIB0リソース（任意のタブ区切りlabel）のKIV payloadを返します。VISAがなければ空の成功listです。リソースを1つ渡すと排他的VISA sessionを開き、同じ上限付きKIVバイトプロトコルを受け付けます（単一接続先なのでaddressは0）。探索・openではSCPIを送りません。GPIBのLOCAL・closeは指定先GTLを使い、他VISA sessionへ影響するバス全体のREN解除を避けます。Rust親側は命令期限、プロセス分離、識別確認、安全な終了を保持します。GUIの`VISA::`はhelper選択prefixで、ベンダーAPIに渡す前に除きます。選択後に失敗したI/OをWinUSBで再実行しません。
