"""Read-only probe: ask COM3 what protocol it speaks.

Sends an MCUboot SMP 'image list' request (a read, no side effects) and dumps
whatever comes back, then dumps any unsolicited banner traffic.
"""

import sys
import base64
import struct
import serial


def crc16_xmodem(data: bytes) -> int:
    crc = 0
    for byte in data:
        crc ^= byte << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc


def smp_frame(op: int, group: int, cmd_id: int, payload: bytes) -> bytes:
    header = struct.pack(">BBHHBB", op, 0, len(payload), group, 0, cmd_id)
    body = header + payload
    framed = struct.pack(">H", len(body) + 2) + body + struct.pack(">H", crc16_xmodem(body))
    return b"\x06\x09" + base64.b64encode(framed) + b"\n"


def main() -> int:
    port = sys.argv[1] if len(sys.argv) > 1 else "COM3"

    with serial.Serial(port, 115200, timeout=2) as ser:
        ser.dtr = True
        ser.reset_input_buffer()

        # SMP image list: op=0 (read), group=1 (image), id=0 (state), payload={}
        request = smp_frame(0, 1, 0, b"\xa0")
        print(f"TX ({len(request)} bytes): {request!r}")
        ser.write(request)
        ser.flush()

        reply = ser.read(256)
        print(f"RX ({len(reply)} bytes): {reply!r}")

        print("Listening 3s for unsolicited output...")
        ser.timeout = 3
        print(f"RX ({len(ser.read(256))} bytes idle)")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
