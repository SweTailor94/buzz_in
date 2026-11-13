#![no_std]
#![no_main]

mod buttons_and_leds;

use assign_resources::assign_resources;
use core::fmt::Display;
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::select::Either;
use embassy_net::tcp::TcpSocket;
use embassy_net::{Ipv4Address, Ipv4Cidr, StackResources};
use embassy_rp::clocks::RoscRng;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::{DMA_CH0, PIO0, USB};
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_rp::{Peri, bind_interrupts, peripherals};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
// use embassy_sync::channel::Channel;
use buttons_and_leds::{ButtonEvent, buttons_task, leds_task};
use embassy_sync::signal::Signal;
use embassy_usb::class::cdc_ncm::embassy_net::{Device, Runner, State as NetState};
use embassy_usb::class::cdc_ncm::{CdcNcmClass, State};
use embassy_usb::driver::Event;
use embassy_usb::{Builder, Config, UsbDevice};
use embedded_io_async::{Error, ErrorKind, Write};
use heapless::Vec;
use messages::{ButtonState, Command};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

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
static BUTTON_EVENTS: Channel<CriticalSectionRawMutex, ButtonEvent,6> = Channel::new();
static BUTTON_STATE_STRINGS: [&str; 6] = ["Red", "Green", "Blue", "Yellow", "White", "None"];


assign_resources! {
    leds: Leds{
        red_led: PIN_10,
        green_led: PIN_11,
        blue_led: PIN_12,
        yellow_led: PIN_13,
        white_led: PIN_14,
        ws2812_pin: PIN_16,
        ws2812_pio: PIO0,
        ws2812_dma: DMA_CH0,
    },
    buttons: Buttons{
        red_button: PIN_2,
        green_button: PIN_3,
        blue_button: PIN_4,
        yellow_button: PIN_5,
        white_button: PIN_6,
        reset_button: PIN_7,
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut rng = RoscRng;
    let resources = split_resources!(p);
    // Create the driver, from the HAL.
    let driver = Driver::new(p.USB, Irqs);

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

    // Start buttons and leds tasks
    spawner.spawn(buttons_task(resources.buttons).unwrap());
    spawner.spawn(leds_task(resources.leds).unwrap());

    // And now we can use it!

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    // let mut buf = [0; 4096];
    let mut send_button_state:[ButtonState;5] = [ButtonState::None;5];

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
                                trace!("BUTTON_EVENTS.send(ButtonEvent::Reset)");
                                BUTTON_EVENTS.send(ButtonEvent::Reset).await;
                            }
                            Command::PollButtonState => {
                                server_client
                                    .send_message(Command::Buttons(send_button_state.clone()))
                                    .await;
                            }
                            _ => {}
                        }
                    }
                    Either::First(Err(e)) => {
                        // socket closed
                        break;
                    }
                    Either::Second(button_state) => {
                        match button_state {
                            Command::Buttons(button_state) => {

                                send_button_state = button_state;
                                trace!("New send button state: {:?}", send_button_state.iter().map(|state| match state{
                                    ButtonState::Red => BUTTON_STATE_STRINGS[0],
                                    ButtonState::Green => BUTTON_STATE_STRINGS[1],
                                    ButtonState::Blue => BUTTON_STATE_STRINGS[2],
                                    ButtonState::Yellow => BUTTON_STATE_STRINGS[3],
                                    ButtonState::White => BUTTON_STATE_STRINGS[4],
                                    ButtonState::None => BUTTON_STATE_STRINGS[5],
                                }).collect::<Vec<&str,5>>());
                            }
                            _ => {}
                        }
                        STATE_REPORT.reset();
                    }
                }
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
        self.data.clear();
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
