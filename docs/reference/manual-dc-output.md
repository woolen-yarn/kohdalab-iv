# Manual DC output

Each Source card has a **DC** disclosure beside **Device**. Closed controls use
the existing header row; the measurement panel and Run / Save controls retain
their layout. Opening DC reveals the level, unit, source compliance and ramp
step. A DC ON badge identifies a fixed output independently of the sweep state.

## An IV source before a sweep

Connect the selected source, open DC, set its function/level/protection and press
**Output ON**. **Apply** ramps an already enabled output to the new level. Turn
output OFF before changing function, compliance or output range. Source 1
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

Press Output ON to enable the chosen gate voltage. Its card stays visibly active
and shows the source level even when the DC disclosure is closed. The bias is
held through successful IV sweeps and ordinary manual sweep Stop. Configuration
changes are locked during measurement; change gate bias between runs. Use OFF
or Disconnect explicitly when finished.

## Safety and saved settings

OFF is a priority safety request. During an active sweep or output ramp it
cancels that operation; it may trigger the existing all-output safety cleanup.
A later ON/Start request cannot clear a pending OFF or run afterward unnoticed.
An idle source-specific OFF preserves other healthy fixed outputs. A source
configuration, communication or protection fault uses the fail-closed shutdown
policy, including the independent gate. An unverified OFF is reported as unknown,
not as successfully disabled.

Saved profiles contain requested DC values and units, never an instruction to
reconnect or re-enable an output. Loading a profile or restarting the application
does not turn DC ON. Disconnect before rebinding/removing a connected bias card.
Source levels are programmed levels/readbacks, not independent terminal-voltage
measurements. Existing hardware compliance and sample-safe limits still apply.

The GUI and Python clients share the same Rust engine and its device ownership.
Python ownership locks ordinary GUI edits; the existing GUI Stop / Take control
path remains available. Measurement CSV and raw CSV columns are unchanged.
