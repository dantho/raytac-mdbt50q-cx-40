//! Bare-metal blink probe: no embassy, no interrupts, no clocks, no BLE.
//!
//! Sole purpose is to answer "does our code execute at 0x1000 at all?".
//! The optional D2 LED is P0.08 and is active-low.

#![no_std]
#![no_main]

use core::ptr::write_volatile;

use cortex_m_rt::entry;
use nrf_pac as _;

const P0_BASE: usize = 0x5000_0000;
const OUTSET: usize = 0x508;
const OUTCLR: usize = 0x50C;
const DIRSET: usize = 0x518;

const LED_P0_08: u32 = 1 << 8;

/// ~0.5 s of busy-wait at the 64 MHz post-reset clock.
const HALF_SECOND: u32 = 32_000_000;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[inline(always)]
fn reg(base: usize, offset: usize, val: u32) {
    unsafe { write_volatile((base + offset) as *mut u32, val) }
}

#[entry]
fn main() -> ! {
    reg(P0_BASE, DIRSET, LED_P0_08);

    loop {
        reg(P0_BASE, OUTCLR, LED_P0_08);
        cortex_m::asm::delay(HALF_SECOND);

        reg(P0_BASE, OUTSET, LED_P0_08);
        cortex_m::asm::delay(HALF_SECOND);
    }
}
