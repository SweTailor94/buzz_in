use eframe::egui::Context;

use crate::AppState;
use crate::button_states::Button;
use messages::{ButtonState, Command};
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn};
use std::time::Duration;

const BUZZ_IN_SERVER_ADDR: &str = "127.0.0.1:49591"; //"10.42.0.100:49591"; //
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const RECONNECT_DELAY: Duration = Duration::from_secs(1);
const SOCKET_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_MESSAGE_SIZE: usize = 32;

pub fn start_buttons_task(
    repaint_signal_clone: Arc<Mutex<Option<Context>>>,
    app_state_for_thread: Arc<Mutex<AppState>>,
) {
    spawn(move || {
        loop {
            // Connect to buzz in server TCP socket @ 10.42.0.100:49591
            let mut stream = match TcpStream::connect(BUZZ_IN_SERVER_ADDR) {
                Ok(stream) => stream,
                Err(err) => {
                    eprintln!("Failed to connect to buzz in server: {err}");
                    sleep(RECONNECT_DELAY);
                    continue;
                }
            };

            if let Err(err) = configure_stream(&stream) {
                eprintln!("Failed to configure buzz in TCP socket: {err}");
                sleep(RECONNECT_DELAY);
                continue;
            }

            loop {
                match reset_server_if_armed(&mut stream, &app_state_for_thread) {
                    Ok(true) => break, // Reset has no response; reconnect before polling.
                    Ok(false) => {}
                    Err(err) => {
                        eprintln!("Failed to reset buzz in server: {err}");
                        break;
                    }
                }

                let response = match poll_button_state(&mut stream) {
                    Ok(response) => response,
                    Err(err) => {
                        eprintln!("Buzz in TCP connection failed: {err}");
                        break;
                    }
                };

                match response {
                    Command::Buttons(order) => {
                        update_button_state(&app_state_for_thread, &repaint_signal_clone, order);
                    }
                    // Do nothing for these
                    Command::Reset | Command::None | Command::PollButtonState => {}
                }

                sleep(POLL_INTERVAL);
            }
        }
    });
}

fn configure_stream(stream: &TcpStream) -> io::Result<()> {
    stream.set_read_timeout(Some(SOCKET_TIMEOUT))?;
    stream.set_write_timeout(Some(SOCKET_TIMEOUT))?;
    stream.set_nodelay(true)
}

fn poll_button_state(stream: &mut TcpStream) -> io::Result<Command> {
    // send cmd to and get the response
    send_message(stream, &Command::PollButtonState)?;
    read_message(stream)
}

fn reset_server_if_armed(
    stream: &mut TcpStream,
    app_state_for_thread: &Arc<Mutex<AppState>>,
) -> io::Result<bool> {
    let should_reset = {
        let mut app_state = app_state_for_thread.lock().unwrap();
        if app_state.do_reset {
            app_state.do_reset = false;
            true
        } else {
            false
        }
    };

    if should_reset {
        println!("Sending Reset");
        if let Err(err) = send_message(stream, &Command::Reset) {
            return Err(err);
        }
    }

    Ok(should_reset)
}

fn send_message(stream: &mut TcpStream, message: &Command) -> io::Result<()> {
    let encoded_message = message.to_vec().map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to serialize {} command: {err}", message.as_str()),
        )
    })?;

    stream.write_all(encoded_message.as_slice())?;
    stream.flush()
}

fn read_message(stream: &mut TcpStream) -> io::Result<Command> {
    let mut read_buffer = [0u8; MAX_MESSAGE_SIZE];
    let mut data = Vec::with_capacity(MAX_MESSAGE_SIZE);

    loop {
        let read = stream.read(&mut read_buffer)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "buzz in server closed the TCP connection",
            ));
        }

        if data.len() + read > MAX_MESSAGE_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "received a buzz in message larger than 32 bytes",
            ));
        }

        data.extend_from_slice(&read_buffer[..read]);

        if let Ok(command) = Command::from_bytes(data.as_slice()) {
            return Ok(command);
        }
    }
}

fn update_button_state(
    app_state_for_thread: &Arc<Mutex<AppState>>,
    repaint_signal_clone: &Arc<Mutex<Option<Context>>>,
    order: [ButtonState; 5],
) {
    let button_order = order.map(button_from_state);

    let should_repaint = {
        let mut app_state = app_state_for_thread.lock().unwrap();
        if button_order.iter().all(|button| *button == Button::None) {
            app_state.answer_index = 0;
        }

        let button_pressed = button_order
            .get(app_state.answer_index)
            .copied()
            .unwrap_or(Button::None);

        if app_state.button == button_pressed && app_state.button_order == button_order {
            false
        } else {
            app_state.button_order = button_order;
            app_state.button = button_pressed;
            true
        }
    };

    if should_repaint {
        let repaint_signal = repaint_signal_clone.lock().unwrap();
        if let Some(repaint_signal) = repaint_signal.as_ref() {
            repaint_signal.request_repaint();
        }
    }
}

fn button_from_state(button_state: ButtonState) -> Button {
    match button_state {
        ButtonState::Red => Button::Red,
        ButtonState::Green => Button::Green,
        ButtonState::Blue => Button::Blue,
        ButtonState::Yellow => Button::Yellow,
        ButtonState::White => Button::White,
        ButtonState::None => Button::None,
    }
}
