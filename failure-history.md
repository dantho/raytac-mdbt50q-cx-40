# Failure history — Raytac MDBT50Q-CX-40 BLE advertising firmware

Session date: 2026-09-05. Status at end of session: **firmware flashes successfully
but does not advertise, and we cannot confirm the application executes at all.**

This is a record of what was tried, what was proven, and what is still unknown.
It is written to stop the next session repeating the same dead ends.

---

## 1. Goal

Bare-metal Rust BLE peripheral advertising as `MDBT50Q-CX-40`, using
`embassy-nrf` + `nrf-sdc`/`nrf-mpsl` (link-time SoftDevice Controller) +
`trouble-host`. No SoftDevice blob, no SWD probe, flashed over USB.

## 2. Hardware facts — CONFIRMED

Read directly from the bootloader via DFU `HARDWARE_VERSION` (0x0A) and
`FIRMWARE_VERSION` (0x0B). These are measurements, not inferences:

| Fact | Value |
|---|---|
| Part | `0x52840` (nRF52840), variant `AAD0` |
| ROM / RAM | 1 MB / 256 KB, 4 KB page size |
| Application base | **`0x001000`** |
| Bootloader | **`0x0F4000`**, length `0xA000` |
| SoftDevice | **none installed** |
| LF crystal | **external 32.768 kHz fitted** |
| Standard LED | **D1 on P0.06, active-low** |
| User switch | **SW1 on P1.06** |
| Power | **3.0 V default, high-voltage mode, LDO only** |

Consequences: `memory.x` (`FLASH ORIGIN = 0x1000`) was correct from the start,
and the `nrf-sdc` architecture is valid (no S140 to conflict with).

## 3. Flashing over USB — SOLVED

The module has two firmware identities, both VID `0x1915`:

- `PID 0x521A` — Raytac demo app, USB string `"nRF52 USB CDC BLE Demo"`.
  Blue LED pulses slowly. Speaks no DFU protocol.
- `PID 0x521F` — `"Open DFU Bootloader"`.

**The only working DFU trigger:** unplug → press and *hold* the button → plug in
while holding → hold ~5 s → release.

Flashing procedure that works:

```powershell
cargo objcopy --release -- -O ihex ble-adv.hex
nrfutil pkg generate --hw-version 52 --sd-req 0x00 `
  --application ble-adv.hex --application-version N ble-adv-dfu.zip
nrfutil dfu usb-serial -pkg ble-adv-dfu.zip -p COM<n>
```

`nrfutil` is the legacy Python tool (`pip install nrfutil`), not the new nRF Util.
The COM port number changes between enumerations — always re-check it.

### DFU triggers that DO NOT work (all tested)

- `nrfutil` against the running demo app → "No trigger interface found"
- DFU serial ping / MTU request → zero bytes returned
- MCUboot SMP probe → silent
- 1200-baud touch with DTR/RTS low → no re-enumeration whatsoever
- Baud rate changes → irrelevant; it is USB CDC, baud is ignored
- Pressing the button while the app runs → it is a *user* button, not RESET
  (USB never re-enumerates, so no reset occurred)

## 4. Firmware changes made (none verified to take effect)

Four changes were made in sequence. The board was silent before and after every
one of them, so **none of these is confirmed to have had any effect**.

1. **Interrupt priorities → P2.** `embassy_nrf::init(Default::default())`
   defaults `gpiote_interrupt_priority` and `time_interrupt_priority` to `P0`.
   MPSL reserves P0/P1 for its radio handlers, and embassy-nrf's own doc comment
   says the time driver "should be lower priority than softdevice if used".
   A real bug against a documented requirement. **Keep.**

2. **LFCLK `XTAL` → `RC`** (`rc_ctiv: 16`, `rc_temp_ctiv: 2`). This was
   speculative and is now reverted: the spec confirms the external 32.768 kHz
   crystal is fitted on XL1/XL2 (P0.00/P0.01). **Use XTAL.**

3. **`cortex-m-rt` feature `set-vtor`.** The image lives at `0x1000` but
   `cortex-m-rt` does not set `VTOR`. cortex-m-rt's docs describe this exact
   symptom: "main function executing but interrupt handlers not being used".
   Plausible and cheap. **Keep.**

4. **Panic handler → reboot into DFU.** Replaced `panic-probe` (which does
   `udf` → hard fault → silent lockup, useless without a probe) with a handler
   writing `GPREGRET = 0xB1` then `sys_reset()`. **This handler has never been
   observed to work** — see section 5.

## 5. Diagnostics attempted, and what each actually proved

| Probe | Result | Valid conclusion |
|---|---|---|
| BLE scan from PC (`bleak`) | Target `F0:D5:51:50:4A:C0` never seen | Firmware genuinely does not advertise |
| Deliberate `panic!()` on entry to `main` | Silence, no DFU return | **Ambiguous** — either app never runs, or `GPREGRET` is ignored |
| Blink sweep, all GPIO except `P0.00/01/09/10/18` | Dark | **Nothing** — the sweep was unnecessary; D1 is P0.06 |
| Blink sweep, all GPIO except `P0.18` | Dark | **Nothing decisive** — D1 is P0.06 and was included |
| Liveness probe: arm `GPREGRET`, blink `P0.18`, reset | No DFU return | **Ambiguous** — a reset loop looks identical to a dead app on USB |
| DFU `FIRMWARE_VERSION` query | App at `0x1000`, v6, 464 B | Decisive; see section 2 |

Only the last one was truly decisive, and it is the only probe that read state
out of the device rather than inferring it from silence.

### Reasoning errors worth not repeating

- **Diagnosing before establishing observability.** Four fixes were stacked
  without any channel to confirm the code even reaches `main`. Each cycle cost a
  manual button-hold and returned roughly one bit.
- **"No USB port after flashing = success."** It only ever proved the bootloader
  jumped somewhere. It was wrongly read as a working handoff.
- **"Dark LED = code not running."** Wrong: five pins were excluded from the
  sweep, so the result was uninformative. This produced a confident but false
  "the link address must be wrong" conclusion, which the bootloader query then
  refuted.
- **Trusting an unvalidated instrument.** Three "nothing panicked" readings all
  depended on a panic handler that was never shown to fire.
- **Assuming hardware from a part number.** The 32.768 kHz crystal, the LED pin,
  the RESET button, and the bootloader address were all assumed. The bootloader
  address was wrong (`0xE0000` assumed vs `0xF4000` actual); the others are
  still unverified.

## 6. Open questions

1. **Does the application execute at all?** Unresolved. It is correctly
   installed at `0x1000`, but no probe has produced a positive liveness signal.
2. **Does this bootloader honour `GPREGRET = 0xB1`?** Unproven. If it does not,
   every reset-based diagnostic is invisible.
3. **Why does the application produce no BLE advertising or visible LED?**
   Still unresolved; the app is at the confirmed address but has no validated
   runtime observability.

## 7. Recommended next steps

In order of value:

1. **Use the spec sheet and schematic.** D1 is P0.06 active-low, SW1 is P1.06,
   XL1/XL2 are P0.00/P0.01, and SWDIO/SWDCLK are underside test pads.
2. **SWD probe.** Four wires (SWDIO, SWDCLK, GND, VDD) plus a Pico running
   `debugprobe` gives `defmt` logs, breakpoints and memory inspection, ending
   the blind iteration. *Rejected this session: the module is encased in plastic
   and physically tiny.*
3. **USB CDC logging via `embassy-usb`.** No wires; same USB-C cable. Requires
   `SoftwareVbusDetect` because MPSL owns the `CLOCK_POWER` interrupt, plus an
   explicit HFCLK request since USB needs HFXO continuously. Caveat: it is built
   on the same `embassy_nrf::init` and interrupt machinery that may itself be
   failing, so it may produce nothing if the app never reaches `main`.
4. **Test P0.06 directly as the LED**, with `GPREGRET` armed first, and observe
   the LED rather than relying on USB re-enumeration.

## 8. Repo state

- [src/main.rs](src/main.rs) — BLE advertising firmware. Builds clean; contains
  changes 1–4 above. Feature `selftest-panic` panics immediately on entry.
- [src/blink.rs](src/blink.rs) — bare-metal GPIO sweep, no embassy.
- [src/probe.rs](src/probe.rs) — liveness probe (arm GPREGRET, blink
  `P0.18`, reset).
- [memory.x](memory.x) — corrected to the measured layout: app `0x1000`,
  length `0xF3000` up to the bootloader at `0xF4000`.
- Scratch scripts were preserved into `tools/`: `dfu_layout.py` (bootloader
  layout query — the most useful tool produced this session) and `ble_scan.py`
  (PC-side BLE scanner, needs `pip install bleak`).
