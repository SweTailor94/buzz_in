#![no_std]
#![no_main]

use assign_resources::assign_resources;
use core::fmt::Display;
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, Either5, Either6, Select};
use embassy_net::tcp::TcpSocket;
use embassy_net::{Ipv4Address, Ipv4Cidr, StackResources};
use embassy_rp::clocks::RoscRng;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::{DMA_CH0, PIO0, USB};
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_rp::{Peri, bind_interrupts, peripherals};
use embassy_rp::pio::Pio;
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program, RgbwPioWs2812};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
// use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::Instant;
use embassy_usb::class::cdc_ncm::embassy_net::{Device, Runner, State as NetState};
use embassy_usb::class::cdc_ncm::{CdcNcmClass, State};
use embassy_usb::{Builder, Config, UsbDevice};
use embedded_io_async::{Error, ErrorKind, Write};
use heapless::Vec;
use messages::{ButtonState, Command};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};
use smart_leds::RGB8;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
     PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
     DMA_IRQ_0 => embassy_rp::dma::InterruptHandler<DMA_CH0>;
});

type MyDriver = Driver<'static, peripherals::USB>;

const MTU: usize = 1514;

#[embassy_executor::task]
async fn usb_task(mut device: UsbDevice<'static, MyDriver>) -> ! {
    device.run().await
}

#[embassy_executor::task]
async fn usb_ncm_task(class: Runner<'static, MyDriver, MTU>) -> ! {
    class.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, Device<'static, MTU>>) -> ! {
    runner.run().await
}

static STATE_REPORT: Signal<CriticalSectionRawMutex, Command> = Signal::new();
static RESET_SIGNAL: Signal<CriticalSectionRawMutex, Command> = Signal::new();

assign_resources! {
    buttons_and_leds: ButtonsAndLeds{
        red_button: PIN_2,
        red_led: PIN_10,
        green_button: PIN_3,
        green_led: PIN_11,
        blue_button: PIN_4,
        blue_led: PIN_12,
        yellow_button: PIN_5,
        yellow_led: PIN_13,
        white_button: PIN_6,
        white_led: PIN_14,
        reset_button: PIN_7,
    }
}

#[derive(Eq, PartialEq)]
enum ButtoTaskOperation {
    Normal,
    Timing,
}

#[embassy_executor::task]
async fn button_and_leds_task(peri: ButtonsAndLeds) -> ! {
    let mut red_button = Input::new(peri.red_button, Pull::Up);
    let mut led_red = Output::new(peri.red_led, Level::High);
    let mut green_button = Input::new(peri.green_button, Pull::Up);
    let mut led_green = Output::new(peri.green_led, Level::High);
    let mut blue_button = Input::new(peri.blue_button, Pull::Up);
    let mut led_blue = Output::new(peri.blue_led, Level::High);
    let mut yellow_button = Input::new(peri.yellow_button, Pull::Up);
    let mut led_yellow = Output::new(peri.yellow_led, Level::High);
    let mut white_button = Input::new(peri.white_button, Pull::Up);
    let mut led_white = Output::new(peri.white_led, Level::High);
    let mut reset_button = Input::new(peri.reset_button, Pull::Up);
    let mut start_time = Instant::now();
    let mut operation = ButtoTaskOperation::Normal;

    loop {
        let hit = embassy_futures::select::select6(
            red_button.wait_for_falling_edge(),
            green_button.wait_for_falling_edge(),
            blue_button.wait_for_falling_edge(),
            yellow_button.wait_for_falling_edge(),
            white_button.wait_for_falling_edge(),
            RESET_SIGNAL.wait(),
        )
        .await;
        let ms = start_time.elapsed().as_millis();
        match hit {
            Either6::First(_) => {
                // Red
                info!("Hit RED");
                led_green.set_low();
                led_blue.set_low();
                led_yellow.set_low();
                led_white.set_low();
                if operation == ButtoTaskOperation::Normal {
                    STATE_REPORT.signal(Command::Buttons(ButtonState::Red));
                } else {
                    STATE_REPORT.signal(Command::TimingResult(ButtonState::Red, ms));
                }
            }
            Either6::Second(_) => {
                // Green
                info!("Hit GREEN");
                led_red.set_low();
                led_blue.set_low();
                led_yellow.set_low();
                led_white.set_low();
                if operation == ButtoTaskOperation::Normal {
                    STATE_REPORT.signal(Command::Buttons(ButtonState::Green));
                } else {
                    STATE_REPORT.signal(Command::TimingResult(ButtonState::Green, ms));
                }
            }
            Either6::Third(_) => {
                // Blue
                info!("Hit BLUE");
                led_red.set_low();
                led_green.set_low();
                led_yellow.set_low();
                led_white.set_low();
                if operation == ButtoTaskOperation::Normal {
                    STATE_REPORT.signal(Command::Buttons(ButtonState::Blue));
                } else {
                    STATE_REPORT.signal(Command::TimingResult(ButtonState::Blue, ms));
                }
            }
            Either6::Fourth(_) => {
                // Yellow
                info!("Hit YELLOW");
                led_red.set_low();
                led_green.set_low();
                led_blue.set_low();
                led_white.set_low();
                if operation == ButtoTaskOperation::Normal {
                    STATE_REPORT.signal(Command::Buttons(ButtonState::Yellow));
                } else {
                    STATE_REPORT.signal(Command::TimingResult(ButtonState::Yellow, ms));
                }
            }
            Either6::Fifth(_) => {
                // White
                info!("Hit WHITE");
                led_red.set_low();
                led_green.set_low();
                led_blue.set_low();
                led_yellow.set_low();
                if operation == ButtoTaskOperation::Normal {
                    STATE_REPORT.signal(Command::Buttons(ButtonState::White));
                } else {
                    STATE_REPORT.signal(Command::TimingResult(ButtonState::White, ms));
                }
            }
            Either6::Sixth(v) => {
                match v {
                    Command::Reset => {
                        continue;
                    }
                    Command::TimingIdle => {
                        info!("TimingIdle");
                        operation = ButtoTaskOperation::Timing;
                        // all off
                        led_red.set_low();
                        led_green.set_low();
                        led_blue.set_low();
                        led_yellow.set_low();
                        led_white.set_low();
                    }
                    Command::TimingStart => {
                        if operation == ButtoTaskOperation::Timing {
                            led_red.set_high();
                            led_green.set_high();
                            led_blue.set_high();
                            led_yellow.set_high();
                            led_white.set_high();
                            start_time = Instant::now();
                        }
                        continue;
                    }
                    _ => {
                        continue;
                    }
                }
            }
        }
        info!("Time {} ms", ms);
        loop {
            match operation {
                ButtoTaskOperation::Normal => {
                    let reset = embassy_futures::select::select(
                        reset_button.wait_for_falling_edge(),
                        RESET_SIGNAL.wait(),
                    )
                    .await;
                    match reset {
                        Either::First(_) => {
                            info!("Reset Button");
                        }
                        Either::Second(v) => {
                            match v {
                                Command::Reset => {
                                }
                                Command::TimingIdle => {
                                    info!("TimingIdle");
                                    operation = ButtoTaskOperation::Timing;
                                    // all off
                                    led_red.set_low();
                                    led_green.set_low();
                                    led_blue.set_low();
                                    led_yellow.set_low();
                                    led_white.set_low();
                                    continue;
                                }
                                Command::TimingStart => {
                                    continue;
                                }
                                _ => {
                                    continue;
                                }
                            }
                        }
                    }
                    STATE_REPORT.signal(Command::Buttons(ButtonState::Ready));

                    led_red.set_high();
                    led_green.set_high();
                    led_blue.set_high();
                    led_yellow.set_high();
                    led_white.set_high();
                    start_time = Instant::now();
                }

                ButtoTaskOperation::Timing => {
                    let reset = embassy_futures::select::select(
                        reset_button.wait_for_falling_edge(),
                        RESET_SIGNAL.wait(),
                    )
                    .await;
                    match reset {
                        Either::First(_) => {
                            info!("Reset Button back to normal");
                            operation = ButtoTaskOperation::Normal;
                            led_red.set_high();
                            led_green.set_high();
                            led_blue.set_high();
                            led_yellow.set_high();
                            led_white.set_high();
                            break;
                        }
                        Either::Second(v) => {
                            match v {
                                Command::Reset => {
                                    // back to normal
                                    operation = ButtoTaskOperation::Normal;
                                    led_red.set_high();
                                    led_green.set_high();
                                    led_blue.set_high();
                                    led_yellow.set_high();
                                    led_white.set_high();
                                    break
                                }
                                Command::TimingIdle => {
                                    info!("TimingIdle");
                                    operation = ButtoTaskOperation::Timing;
                                    // all off
                                    led_red.set_low();
                                    led_green.set_low();
                                    led_blue.set_low();
                                    led_yellow.set_low();
                                    led_white.set_low();
                                }
                                Command::TimingStart => {
                                    led_red.set_high();
                                    led_green.set_high();
                                    led_blue.set_high();
                                    led_yellow.set_high();
                                    led_white.set_high();
                                    start_time = Instant::now();

                                    break;
                                }
                                _ => {

                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut rng = RoscRng;
    let resources = split_resources!(p);
    // Create the driver, from the HAL.
    let driver = Driver::new(p.USB, Irqs);

    // set up Neopixel
    let mut pio = Pio::new(p.PIO0, Irqs);
    let program = PioWs2812Program::new(&mut pio.common);
    let mut  ws2812 = PioWs2812::new(&mut pio.common, pio.sm0, p.DMA_CH0, Irqs,  p.PIN_16, &program);

    let mut ws2812_buffer = [RGB8::new(128,0,0),RGB8::new(0, 128, 0), RGB8::new(0, 0, 128), RGB8::new(128,128,0), RGB8::new(128,128,128)];

    ws2812.write(&ws2812_buffer).await;

    // Create embassy-usb Config
    let mut config = Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Tailor94");
    config.product = Some("USB-Buzzer_buttons");
    config.serial_number = Some("12345678");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    // Create embassy-usb DeviceBuilder using the driver and config.
    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; 128]> = StaticCell::new();
    let mut builder = Builder::new(
        driver,
        config,
        &mut CONFIG_DESC.init([0; 256])[..],
        &mut BOS_DESC.init([0; 256])[..],
        &mut [], // no msos descriptors
        &mut CONTROL_BUF.init([0; 128])[..],
    );

    // Our MAC addr.
    let our_mac_addr = [0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC];
    // Host's MAC addr. This is the MAC the host "thinks" its USB-to-ethernet adapter has.
    let host_mac_addr = [0x88, 0x88, 0x88, 0x88, 0x88, 0x88];

    // Create classes on the builder.
    static STATE: StaticCell<State> = StaticCell::new();
    let class = CdcNcmClass::new(&mut builder, STATE.init(State::new()), host_mac_addr, 64);

    // Build the builder.
    let usb = builder.build();

    spawner.spawn(usb_task(usb).unwrap());

    static NET_STATE: StaticCell<NetState<MTU, 4, 4>> = StaticCell::new();
    let (runner, device) =
        class.into_embassy_net_device::<MTU, 4, 4>(NET_STATE.init(NetState::new()), our_mac_addr);
    spawner.spawn(usb_ncm_task(runner).unwrap());

    // let config = embassy_net::Config::dhcpv4(Default::default());
    let config = embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
        address: Ipv4Cidr::new(Ipv4Address::new(10, 42, 0, 100), 24),
        dns_servers: Vec::new(),
        gateway: Some(Ipv4Address::new(10, 42, 0, 1)),
    });

    // Generate random seed
    let seed = rng.next_u64();

    // Init network stack
    static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    let (stack, runner) =
        embassy_net::new(device, config, RESOURCES.init(StackResources::new()), seed);

    spawner.spawn(net_task(runner).unwrap());

    spawner.spawn(button_and_leds_task(resources.buttons_and_leds).unwrap());

    // And now we can use it!

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    // let mut buf = [0; 4096];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        // socket.set_timeout(Some(embassy_time::Duration::from_secs(10)));

        info!("Listening on TCP:1234...");
        if let Err(e) = socket.accept(49591).await {
            warn!("accept error: {:?}", e);
            continue;
        }

        info!("Received connection from {:?}", socket.remote_endpoint());

        let mut server_client = ServerClient {
            socket,
            buf: [0; 4096],
            data: Vec::new(),
        };

        loop {
            let mut send_button_state: Option<ButtonState> = None;
            {
                // this block needed to drop
                let got = embassy_futures::select::select(
                    server_client.read_message(),
                    STATE_REPORT.wait(),
                )
                .await;
                match got {
                    Either::First(Ok(command)) => {
                        info!("Received command: {}", command.as_str());
                        match command {
                            Command::Reset => {
                                RESET_SIGNAL.signal(Command::Reset);
                            }
                            Command::TimingIdle => {
                                // set up for fastest reaction:
                            }
                            Command::TimingStart => {
                                //
                            }
                            Command::Buttons(_) | Command::TimingResult(_, _) => {} // Respoinses
                        }
                    }
                    Either::First(Err(_)) => {
                        // socket closed
                        break;
                    }
                    Either::Second(button_state) => {
                        match button_state {
                            Command::Buttons(button_state) => {
                                send_button_state = Some(button_state);
                            }
                            Command::Reset => {}
                            Command::TimingIdle => {}
                            Command::TimingStart => {}
                            Command::TimingResult(_, _) => {}
                        }
                        STATE_REPORT.reset();
                    }
                }
            }
            match send_button_state {
                Some(button_state) => {
                    server_client
                        .send_message(Command::Buttons(button_state))
                        .await;
                }
                None => {}
            }
        }
    }
}

#[derive(Debug)]
enum ClientError {
    ReadEof,
    ReadError,
}
impl Display for ClientError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ClientError::ReadEof => f.write_str("Got EOF"),
            ClientError::ReadError => f.write_str("Read Error"),
        }
    }
}

struct ServerClient<'a> {
    socket: TcpSocket<'a>,
    buf: [u8; 4096],
    data: Vec<u8, 32>,
}

impl<'a> ServerClient<'a> {
    async fn read_message(&mut self) -> Result<Command, ClientError> {
        loop {
            let n = match self.socket.read(&mut self.buf).await {
                Ok(0) => {
                    warn!("read EOF");
                    return Err(ClientError::ReadEof);
                }
                Ok(n) => n,
                Err(e) => {
                    warn!("read error: {:?}", e);
                    match e.kind() {
                        ErrorKind::TimedOut => {
                            continue;
                        }
                        _ => return Err(ClientError::ReadError),
                    }
                }
            };
            info!("read {} from socket", n);
            let _ = self.data.extend_from_slice(&self.buf[..n]);

            match Command::from_bytes(self.data.as_slice()) {
                Ok(command) => {
                    info!("Got command: {}", command.as_str());
                    return Ok(command);
                }
                Err(_) => {
                    continue;
                }
            };
        }
    }

    async fn send_message(&mut self, message: Command) {
        let encoded_response: Vec<u8, 32> = message.to_vec().unwrap_or_default();
        self.socket.write_all(&encoded_response).await.unwrap();
        info!("Sent {} to socket", message.as_str());
    }
}
