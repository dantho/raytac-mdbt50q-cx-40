#![no_std]
#![no_main]

use defmt::{info, unwrap, warn};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_nrf::interrupt::Priority;
use embassy_nrf::mode::Async;
use embassy_nrf::{bind_interrupts, peripherals, rng};
use embassy_time::{Duration, Timer};
use nrf_sdc::mpsl::{self, MultiprotocolServiceLayer, Peripherals as MpslPeripherals};
use nrf_sdc::{self as sdc};
use static_cell::StaticCell;
use trouble_host::prelude::*;

use defmt_rtt as _;

/// Magic value the Nordic bootloader checks in GPREGRET to re-enter DFU.
const BOOTLOADER_DFU_START: u8 = 0xB1;

/// Reboot into the DFU bootloader so a failure is visible on USB instead of
/// hanging silently with no debugger attached.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    embassy_nrf::pac::POWER
        .gpregret()
        .write(|w| w.set_gpregret(BOOTLOADER_DFU_START));
    cortex_m::peripheral::SCB::sys_reset()
}

/// Local name shown to scanners.
const DEVICE_NAME: &str = "MDBT50Q-CX-40";

/// Static random address: the two most significant bits of the MSB must be 1.
const BLE_ADDRESS: [u8; 6] = [0xC0, 0x4A, 0x50, 0x51, 0xD5, 0xF0];

const CONNECTIONS_MAX: usize = 1;
const L2CAP_CHANNELS_MAX: usize = 2;
const ADV_SETS_MAX: usize = 1;
const BONDS_MAX: usize = 0;
const L2CAP_MTU: u16 = 27;
const L2CAP_TXQ: u8 = 3;
const L2CAP_RXQ: u8 = 3;

/// Working memory handed to the SoftDevice Controller. Oversized on purpose:
/// too small is a hard error, too large only logs the exact figure to trim to.
const SDC_MEM: usize = 4096;

bind_interrupts!(struct Irqs {
    RNG => rng::InterruptHandler<peripherals::RNG>;
    EGU0_SWI0 => mpsl::LowPrioInterruptHandler;
    CLOCK_POWER => mpsl::ClockInterruptHandler;
    RADIO => mpsl::HighPrioInterruptHandler;
    TIMER0 => mpsl::HighPrioInterruptHandler;
    RTC0 => mpsl::HighPrioInterruptHandler;
});

#[embassy_executor::task]
async fn mpsl_task(mpsl: &'static MultiprotocolServiceLayer<'static>) -> ! {
    mpsl.run().await
}

fn build_sdc<'d>(
    p: sdc::Peripherals<'d>,
    rng: &'d mut rng::Rng<'static, Async>,
    mpsl: &'d MultiprotocolServiceLayer,
    mem: &'d mut sdc::Mem<SDC_MEM>,
) -> Result<sdc::SoftdeviceController<'d>, sdc::Error> {
    sdc::Builder::new()?
        .support_adv()
        .support_peripheral()
        .peripheral_count(CONNECTIONS_MAX as u8)?
        .buffer_cfg(L2CAP_MTU, L2CAP_MTU, L2CAP_TXQ, L2CAP_RXQ)?
        .build(p, rng, mpsl, mem)
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    #[cfg(feature = "selftest-panic")]
    panic!("selftest");

    // MPSL owns priority P0/P1 for its timing-critical radio handlers, so every
    // application interrupt must sit below it.
    let mut config = embassy_nrf::config::Config::default();
    config.gpiote_interrupt_priority = Priority::P2;
    config.time_interrupt_priority = Priority::P2;
    let p = embassy_nrf::init(config);

    // The spec sheet confirms an external 32.768 kHz crystal on XL1/XL2.
    let lfclk_cfg = mpsl::raw::mpsl_clock_lfclk_cfg_t {
        source: mpsl::raw::MPSL_CLOCK_LF_SRC_XTAL as u8,
        rc_ctiv: 0,
        rc_temp_ctiv: 0,
        accuracy_ppm: mpsl::raw::MPSL_DEFAULT_CLOCK_ACCURACY_PPM as u16,
        skip_wait_lfclk_started: mpsl::raw::MPSL_DEFAULT_SKIP_WAIT_LFCLK_STARTED != 0,
    };

    let mpsl_p = MpslPeripherals::new(p.RTC0, p.TIMER0, p.TEMP, p.PPI_CH19, p.PPI_CH30, p.PPI_CH31);
    static MPSL: StaticCell<MultiprotocolServiceLayer> = StaticCell::new();
    let mpsl = MPSL.init(unwrap!(MultiprotocolServiceLayer::new(
        mpsl_p, Irqs, lfclk_cfg
    )));
    unwrap!(spawner.spawn(mpsl_task(mpsl)));

    let sdc_p = sdc::Peripherals::new(
        p.PPI_CH17,
        p.PPI_CH18,
        p.PPI_CH20,
        p.PPI_CH21,
        p.PPI_CH22,
        p.PPI_CH23,
        p.PPI_CH24,
        p.PPI_CH25,
        p.PPI_CH26,
        p.PPI_CH27,
        p.PPI_CH28,
        p.PPI_CH29,
    );

    static RNG: StaticCell<rng::Rng<'static, Async>> = StaticCell::new();
    let rng = RNG.init(rng::Rng::new(p.RNG, Irqs));

    static SDC_MEMORY: StaticCell<sdc::Mem<SDC_MEM>> = StaticCell::new();
    let sdc_mem = SDC_MEMORY.init(sdc::Mem::new());
    let sdc = unwrap!(build_sdc(sdc_p, rng, mpsl, sdc_mem));

    static RESOURCES: StaticCell<
        HostResources<
            DefaultPacketPool,
            CONNECTIONS_MAX,
            L2CAP_CHANNELS_MAX,
            ADV_SETS_MAX,
            BONDS_MAX,
        >,
    > = StaticCell::new();
    let resources = RESOURCES.init(HostResources::new());

    let stack = trouble_host::new(sdc, resources)
        .set_random_address(Address::random(BLE_ADDRESS))
        .build();
    let mut peripheral = stack.peripheral();
    let runner = stack.runner();

    info!("advertising as {}", DEVICE_NAME);
    join(ble_runner(runner), advertise_forever(&mut peripheral)).await;
}

async fn ble_runner<C: Controller, P: PacketPool>(mut runner: Runner<'_, C, P>) -> ! {
    match runner.run().await {
        Ok(()) => panic!("BLE host runner returned unexpectedly"),
        Err(e) => panic!("BLE host runner failed: {:?}", defmt::Debug2Format(&e)),
    }
}

async fn advertise_forever<C: Controller, P: PacketPool>(peripheral: &mut Peripheral<'_, C, P>) {
    let mut adv_data = [0u8; 31];
    let adv_len = unwrap!(AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::CompleteLocalName(DEVICE_NAME.as_bytes()),
        ],
        &mut adv_data,
    ));

    let params = AdvertisementParameters {
        interval_min: Duration::from_millis(200),
        interval_max: Duration::from_millis(300),
        ..Default::default()
    };

    loop {
        let advertiser = match peripheral
            .advertise(
                &params,
                Advertisement::ConnectableScannableUndirected {
                    adv_data: &adv_data[..adv_len],
                    scan_data: &[],
                },
            )
            .await
        {
            Ok(a) => a,
            Err(e) => {
                warn!("advertise failed: {:?}", defmt::Debug2Format(&e));
                Timer::after_secs(1).await;
                continue;
            }
        };

        match advertiser.accept().await {
            Ok(conn) => {
                info!("central connected");
                // No GATT services are exposed yet; wait out the connection and
                // then resume advertising.
                loop {
                    if let ConnectionEvent::Disconnected { reason } = conn.next().await {
                        info!("central disconnected: {:?}", reason);
                        break;
                    }
                }
            }
            Err(e) => warn!("accept failed: {:?}", defmt::Debug2Format(&e)),
        }
    }
}
