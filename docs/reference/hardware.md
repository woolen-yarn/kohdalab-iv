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
