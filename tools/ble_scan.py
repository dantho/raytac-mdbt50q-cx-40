import asyncio
from bleak import BleakScanner

TARGET = "F0:D5:51:50:4A:C0"


async def main():
    print("scanning 10s ...\n")
    found = await BleakScanner.discover(timeout=10.0, return_adv=True)

    rows = sorted(found.values(), key=lambda kv: kv[1].rssi or -999, reverse=True)
    for dev, adv in rows:
        hit = "  <<< TARGET" if dev.address.upper() == TARGET else ""
        print(f"{dev.address}  rssi={adv.rssi:>4}  name={adv.local_name!r}{hit}")
        if adv.service_uuids:
            print(f"      service_uuids   : {adv.service_uuids}")
        if adv.manufacturer_data:
            print(f"      manufacturer    : {adv.manufacturer_data}")
        if adv.service_data:
            print(f"      service_data    : {adv.service_data}")

    print(f"\ntotal devices: {len(found)}")
    match = [d for d, _ in rows if d.address.upper() == TARGET]
    print("TARGET FOUND" if match else "TARGET NOT FOUND")


asyncio.run(main())
