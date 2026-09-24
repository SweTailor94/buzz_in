use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use messages::{ButtonState, Command};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

const ADDRESS: &str = "127.0.0.1:49591";
const BUTTON_COUNT: usize = 6;

type ButtonOrder = [ButtonState; 5];
type SharedButtonOrder = Arc<Mutex<ButtonOrder>>;

#[derive(Clone, Copy)]
struct Button {
    label: &'static str,
    key: char,
    color: Color,
    action: ButtonAction,
}

#[derive(Clone, Copy)]
enum ButtonAction {
    Press(ButtonState),
    Reset,
}

const BUTTONS: [Button; BUTTON_COUNT] = [
    Button {
        label: "Red",
        key: 'r',
        color: Color::Red,
        action: ButtonAction::Press(ButtonState::Red),
    },
    Button {
        label: "Green",
        key: 'g',
        color: Color::Green,
        action: ButtonAction::Press(ButtonState::Green),
    },
    Button {
        label: "Blue",
        key: 'b',
        color: Color::Blue,
        action: ButtonAction::Press(ButtonState::Blue),
    },
    Button {
        label: "Yellow",
        key: 'y',
        color: Color::Yellow,
        action: ButtonAction::Press(ButtonState::Yellow),
    },
    Button {
        label: "White",
        key: 'w',
        color: Color::White,
        action: ButtonAction::Press(ButtonState::White),
    },
    Button {
        label: "Reset",
        key: 'x',
        color: Color::Gray,
        action: ButtonAction::Reset,
    },
];

fn main() -> std::io::Result<()> {
    let button_order = Arc::new(Mutex::new([ButtonState::None; 5]));

    start_tcp_server(Arc::clone(&button_order))?;
    run_tui(button_order)
}

fn start_tcp_server(button_order: SharedButtonOrder) -> std::io::Result<()> {
    let listener = TcpListener::bind(ADDRESS)?;

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let button_order = Arc::clone(&button_order);
                    thread::spawn(move || {
                        if let Err(error) = handle_client(stream, button_order) {
                            eprintln!("Client connection failed: {error}");
                        }
                    });
                }
                Err(error) => eprintln!("Incoming connection failed: {error}"),
            }
        }
    });

    Ok(())
}

fn handle_client(mut stream: TcpStream, button_order: SharedButtonOrder) -> std::io::Result<()> {
    let mut buffer = [0; 32];

    loop {
        let bytes_read = stream.read(&mut buffer)?;

        if bytes_read == 0 {
            return Ok(());
        }

        let command = match Command::from_bytes(&buffer[..bytes_read]) {
            Ok(command) => command,
            Err(error) => {
                eprintln!("Failed to parse command: {error:?}");
                continue;
            }
        };

        match command {
            Command::PollButtonState => {
                let response = {
                    let order = *button_order
                        .lock()
                        .expect("button state mutex should not be poisoned");

                    if order
                        .iter()
                        .any(|button| !matches!(button, ButtonState::None))
                    {
                        Command::Buttons(order)
                    } else {
                        Command::None
                    }
                };

                send_command(&mut stream, &response)?;
            }
            Command::Reset => reset_buttons(&button_order),
            _ => {}
        }
    }
}

fn send_command(stream: &mut TcpStream, command: &Command) -> std::io::Result<()> {
    let bytes = command.to_vec().map_err(|error| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{error:?}"))
    })?;

    stream.write_all(bytes.as_slice())
}

fn run_tui(button_order: SharedButtonOrder) -> std::io::Result<()> {
    enable_raw_mode()?;

    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_tui_loop(&mut terminal, button_order);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    button_order: SharedButtonOrder,
) -> std::io::Result<()> {
    let mut button_areas = [Rect::default(); BUTTON_COUNT];

    loop {
        let order = *button_order
            .lock()
            .expect("button state mutex should not be poisoned");

        terminal.draw(|frame| {
            let area = frame.area();
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(5),
                    Constraint::Min(5),
                    Constraint::Length(3),
                ])
                .split(area);

            let title = Paragraph::new("Buzz-In Server")
                .alignment(Alignment::Center)
                .style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .block(Block::default().borders(Borders::BOTTOM));
            frame.render_widget(title, layout[0]);

            button_areas = draw_buttons(frame, layout[1]);

            let order_text = format!("Current press order: {}", format_button_order(&order));
            let order_widget = Paragraph::new(order_text)
                .alignment(Alignment::Center)
                .block(Block::default().title("State").borders(Borders::ALL));
            frame.render_widget(order_widget, layout[2]);

            let help = Paragraph::new("Keys: R G B Y W, X reset, Q quit")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(help, layout[3]);
        })?;

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char(character) => handle_key(character, &button_order),
                _ => {}
            },
            Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::Down(_)) => {
                handle_click(mouse.column, mouse.row, &button_areas, &button_order);
            }
            _ => {}
        }
    }

    Ok(())
}

fn draw_buttons(frame: &mut ratatui::Frame<'_>, area: Rect) -> [Rect; BUTTON_COUNT] {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, BUTTON_COUNT as u32); BUTTON_COUNT])
        .split(area);

    let mut areas = [Rect::default(); BUTTON_COUNT];

    for (index, button) in BUTTONS.iter().enumerate() {
        let area = inset(chunks[index], 1, 0);
        areas[index] = area;

        let button_widget = Paragraph::new(format!(
            "{} ({})",
            button.label,
            button.key.to_ascii_uppercase()
        ))
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(text_color(button.color))
                .bg(button.color)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));

        frame.render_widget(Clear, area);
        frame.render_widget(button_widget, area);
    }

    areas
}

fn handle_key(character: char, button_order: &SharedButtonOrder) {
    let character = character.to_ascii_lowercase();

    if let Some(button) = BUTTONS.iter().find(|button| button.key == character) {
        apply_action(button.action, button_order);
    }
}

fn handle_click(
    column: u16,
    row: u16,
    areas: &[Rect; BUTTON_COUNT],
    button_order: &SharedButtonOrder,
) {
    if let Some(index) = areas.iter().position(|area| contains(*area, column, row)) {
        apply_action(BUTTONS[index].action, button_order);
    }
}

fn apply_action(action: ButtonAction, button_order: &SharedButtonOrder) {
    match action {
        ButtonAction::Press(button) => press_button(button_order, button),
        ButtonAction::Reset => reset_buttons(button_order),
    }
}

fn press_button(button_order: &SharedButtonOrder, button: ButtonState) {
    let mut order = button_order
        .lock()
        .expect("button state mutex should not be poisoned");

    if order.iter().any(|pressed| same_button(*pressed, button)) {
        return;
    }

    if let Some(slot) = order
        .iter_mut()
        .find(|pressed| matches!(pressed, ButtonState::None))
    {
        *slot = button;
    }
}

fn reset_buttons(button_order: &SharedButtonOrder) {
    let mut order = button_order
        .lock()
        .expect("button state mutex should not be poisoned");

    *order = [ButtonState::None; 5];
}

fn format_button_order(order: &ButtonOrder) -> String {
    let pressed = order
        .iter()
        .copied()
        .filter(|button| !matches!(button, ButtonState::None))
        .map(button_label)
        .collect::<Vec<_>>();

    if pressed.is_empty() {
        "None".to_string()
    } else {
        pressed.join(" -> ")
    }
}

fn button_label(button: ButtonState) -> &'static str {
    match button {
        ButtonState::Red => "Red",
        ButtonState::Green => "Green",
        ButtonState::Blue => "Blue",
        ButtonState::Yellow => "Yellow",
        ButtonState::White => "White",
        ButtonState::None => "None",
    }
}

fn same_button(left: ButtonState, right: ButtonState) -> bool {
    matches!(
        (left, right),
        (ButtonState::Red, ButtonState::Red)
            | (ButtonState::Green, ButtonState::Green)
            | (ButtonState::Blue, ButtonState::Blue)
            | (ButtonState::Yellow, ButtonState::Yellow)
            | (ButtonState::White, ButtonState::White)
            | (ButtonState::None, ButtonState::None)
    )
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

fn inset(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    Rect {
        x: area.x.saturating_add(horizontal),
        y: area.y.saturating_add(vertical),
        width: area.width.saturating_sub(horizontal.saturating_mul(2)),
        height: area.height.saturating_sub(vertical.saturating_mul(2)),
    }
}

fn text_color(background: Color) -> Color {
    match background {
        Color::Yellow | Color::White => Color::Black,
        _ => Color::White,
    }
}
