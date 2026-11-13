use crate::{Buttons, Irqs, Leds, BUTTON_EVENTS, STATE_REPORT};
use defmt::info;
use embassy_futures::select::select_array;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::pio::Pio;
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
use messages::{ButtonState, Command};
use smart_leds::RGB8;

pub enum ButtonEvent {
    Reset,
    ButtonPress(u8),
}

#[embassy_executor::task]
pub async fn buttons_task(peri: Buttons) -> ! {
    let mut red_button = Input::new(peri.red_button, Pull::Up);
    let mut green_button = Input::new(peri.green_button, Pull::Up);
    let mut blue_button = Input::new(peri.blue_button, Pull::Up);
    let mut yellow_button = Input::new(peri.yellow_button, Pull::Up);
    let mut white_button = Input::new(peri.white_button, Pull::Up);
    let mut reset_button = Input::new(peri.reset_button, Pull::Up);

    loop{
        let (_,index) = select_array([
            red_button.wait_for_falling_edge(),
            green_button.wait_for_falling_edge(),
            blue_button.wait_for_falling_edge(),
            yellow_button.wait_for_falling_edge(),
            white_button.wait_for_falling_edge(),
            reset_button.wait_for_falling_edge(),
        ]).await;
        if index == 5 {
            BUTTON_EVENTS.send(ButtonEvent::Reset).await;
        } else {
            BUTTON_EVENTS.send(ButtonEvent::ButtonPress(index as u8)).await;
        }
    }
}


#[embassy_executor::task]
pub async fn leds_task(peri: Leds) -> ! {
    let mut led_red = Output::new(peri.red_led, Level::Low);
    let mut led_green = Output::new(peri.green_led, Level::Low);
    let mut led_blue = Output::new(peri.blue_led, Level::Low);
    let mut led_yellow = Output::new(peri.yellow_led, Level::Low);
    let mut led_white = Output::new(peri.white_led, Level::Low);

    // set up Neopixel
    let mut pio = Pio::new(peri.ws2812_pio, Irqs);
    let program = PioWs2812Program::new(&mut pio.common);
    let mut ws2812 = PioWs2812::new(
        &mut pio.common,
        pio.sm0,
        peri.ws2812_dma,
        Irqs,
        peri.ws2812_pin,
        &program,
    );

    const RED: RGB8 = RGB8::new(128, 0, 0);
    const GREEN: RGB8 = RGB8::new(0, 128, 0);
    const BLUE: RGB8 = RGB8::new(0, 0, 128);
    const YELLOW: RGB8 = RGB8::new(128, 128, 0);
    const WHITE: RGB8 = RGB8::new(128, 128, 128);
    const OFF: RGB8 = RGB8::new(0, 0, 0);

    let mut ws2812_buffer = [
        WHITE.clone(),
        YELLOW.clone(),
        BLUE.clone(),
        GREEN.clone(),
        RED.clone(),
    ];
    ws2812.write(&ws2812_buffer).await;
    // order: red, green, blue, yellow, white
    let mut has_pressed = [true; 5];
    let mut next_in_turn = 0;
    let mut press_order: [ButtonState; 5] = [
        ButtonState::Red,
        ButtonState::Green,
        ButtonState::Blue,
        ButtonState::Yellow,
        ButtonState::White,
    ];


    loop {
        for (i, state) in press_order.iter().enumerate() {
            ws2812_buffer[4 - i] = match state {
                ButtonState::Red => RED,
                ButtonState::Green => GREEN,
                ButtonState::Blue => BLUE,
                ButtonState::Yellow => YELLOW,
                ButtonState::White => WHITE,
                _ => OFF,
            };
        }
        ws2812.write(&ws2812_buffer).await;
        STATE_REPORT.signal(Command::Buttons(press_order.clone()));

        let event = BUTTON_EVENTS.receive().await;
        match event {
            ButtonEvent::ButtonPress(index) =>
                match index {
                    0 => {
                        // Red
                        info!("Hit RED");
                        led_red.set_low();
                        if !has_pressed[0] {
                            has_pressed[0] = true;
                            press_order[next_in_turn] = ButtonState::Red;
                            next_in_turn += 1;
                        }
                    }
                    1 => {
                        // Green
                        info!("Hit GREEN");
                        led_green.set_low();
                        if !has_pressed[1] {
                            has_pressed[1] = true;
                            press_order[next_in_turn] = ButtonState::Green;
                            next_in_turn += 1;
                        }
                    }
                    2 => {
                        // Blue
                        info!("Hit BLUE");
                        led_blue.set_low();
                        if !has_pressed[2] {
                            has_pressed[2] = true;
                            press_order[next_in_turn] = ButtonState::Blue;
                            next_in_turn += 1;
                        }
                    }
                    3 => {
                        // Yellow
                        info!("Hit YELLOW");
                        led_yellow.set_low();
                        if !has_pressed[3] {
                            has_pressed[3] = true;
                            press_order[next_in_turn] = ButtonState::Yellow;
                            next_in_turn += 1;
                        }
                    }
                    4 => {
                        // White
                        info!("Hit WHITE");
                        led_white.set_low();
                        if !has_pressed[4] {
                            has_pressed[4] = true;
                            press_order[next_in_turn] = ButtonState::White;
                            next_in_turn += 1;
                        }
                    }
                    _ => continue ,
                }

            ButtonEvent::Reset => {
                for i in 0..5 {
                    has_pressed[i] = false;
                    press_order[i] = ButtonState::None;
                }
                next_in_turn = 0;
                led_red.set_high();
                led_green.set_high();
                led_blue.set_high();
                led_yellow.set_high();
                led_white.set_high();
                while  BUTTON_EVENTS.try_receive().is_ok() {
                    // This empties any events in the queue
                }
            },
        }
    }
}


