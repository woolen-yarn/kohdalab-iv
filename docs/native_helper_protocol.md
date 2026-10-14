# Standalone GPIB helper interface

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
