//! One-shot liveness probe. Answers two questions in a single flash:
//!   1. Does our code execute at 0x1000 at all?
//!   2. Is the module's LED on P0.18 (the only pin the blink sweep skipped)?
//!
//! GPREGRET is armed *before* touching P0.18, so even if that pin turns out to
//! be nRESET and the chip resets instantly, we still land in the bootloader.
//! Any outcome that reaches the bootloader proves the application runs.

#![no_std]
#![no_main]

use core::ptr::write_volatile;

use cortex_m_rt::entry;
use nrf_pac as _;

const POWER_GPREGRET: usize = 0x4000_051C;
const BOOTLOADER_DFU_START: u32 = 0xB1;

const P0_BASE: usize = 0x5000_0000;
const OUTSET: usize = 0x508;
const OUTCLR: usize = 0x50C;
const DIRSET: usize = 0x518;
const P0_18: u32 = 1 << 18;

/// ~0.25 s at the 64 MHz post-reset clock.
const QUARTER_SECOND: u32 = 16_000_000;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[inline(always)]
fn reg(addr: usize, val: u32) {
    unsafe { write_volatile(addr as *mut u32, val) }
}

#[entry]
fn main() -> ! {
    reg(POWER_GPREGRET, BOOTLOADER_DFU_START);

    reg(P0_BASE + DIRSET, P0_18);
    for _ in 0..10 {
        reg(P0_BASE + OUTCLR, P0_18);
        cortex_m::asm::delay(QUARTER_SECOND);
        reg(P0_BASE + OUTSET, P0_18);
        cortex_m::asm::delay(QUARTER_SECOND);
    }

    cortex_m::peripheral::SCB::sys_reset()
}
