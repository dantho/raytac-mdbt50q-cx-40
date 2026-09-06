"""Query the nRF5 DFU bootloader for its memory layout.

Sends HARDWARE_VERSION (0x0A) and FIRMWARE_VERSION (0x0B) over the SLIP-framed
serial DFU protocol. nrfutil never issues these, but they report the address and
length of every installed image, which is what we need to link against.
"""

import struct
import sys

import serial

END, ESC, ESC_END, ESC_ESC = 0xC0, 0xDB, 0xDC, 0xDD

PORT = sys.argv[1] if len(sys.argv) > 1 else "COM4"

IMG_TYPES = {0x00: "SoftDevice", 0x01: "Application", 0x02: "Bootloader", 0xFF: "none/unknown"}


def slip_encode(data: bytes) -> bytes:
    out = bytearray()
    for b in data:
        if b == END:
            out += bytes([ESC, ESC_END])
        elif b == ESC:
            out += bytes([ESC, ESC_ESC])
        else:
            out.append(b)
    out.append(END)
    return bytes(out)


def slip_read(ser) -> bytes | None:
    out, esc = bytearray(), False
    while True:
        c = ser.read(1)
        if not c:
            return bytes(out) if out else None
        b = c[0]
        if b == END:
            if out:
                return bytes(out)
            continue
        if esc:
            out.append({ESC_END: END, ESC_ESC: ESC}.get(b, b))
            esc = False
        elif b == ESC:
            esc = True
        else:
            out.append(b)


def cmd(ser, payload: bytes) -> bytes | None:
    ser.reset_input_buffer()
    ser.write(slip_encode(payload))
    ser.flush()
    return slip_read(ser)


with serial.Serial(PORT, 115200, timeout=2) as ser:
    hw = cmd(ser, bytes([0x0A]))
    print(f"HARDWARE_VERSION raw: {hw.hex() if hw else None}")
    if hw and len(hw) >= 23 and hw[0] == 0x60 and hw[2] == 0x01:
        part, variant, rom, ram, page = struct.unpack("<IIIII", hw[3:23])
        print(f"  part=0x{part:X} variant=0x{variant:X}")
        print(f"  rom={rom} bytes (0x{rom:X})  ram={ram} bytes (0x{ram:X})  page=0x{page:X}")

    print()
    for i in range(4):
        r = cmd(ser, bytes([0x0B, i]))
        if not r or r[0] != 0x60 or r[2] != 0x01:
            print(f"image {i}: no response / error -> {r.hex() if r else None}")
            continue
        img_type = r[3]
        version, addr, length = struct.unpack("<III", r[4:16])
        name = IMG_TYPES.get(img_type, f"0x{img_type:02X}")
        print(f"image {i}: {name:<12} version={version} addr=0x{addr:06X} len=0x{length:X} ({length} bytes)")
