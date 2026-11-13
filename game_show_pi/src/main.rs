mod audio;
mod button_states;
mod buzz_in_client;
mod colors;
mod quiz_file;

use eframe::egui;
use eframe::emath::Vec2;
use eframe::epaint::ColorImage;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use eframe::egui::accesskit::Role::Audio;
use warp::Filter;

use crate::audio::AudioPlayer;
use crate::button_states::Button;
use crate::buzz_in_client::start_buttons_task;
use crate::colors::{blend_colors, get_colors, lighten_color};
use crate::quiz_file::{Quiz, get_quiz};

#[derive(Clone)]
struct AppState {
    pub button: Button,
    pub button_order: [Button; 5],
    pub answer_index: usize,
    pub play_button_sound: bool,
    pub armed: bool,
    pub do_reset: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            button: Button::None,
            button_order: [Button::None; 5],
            answer_index: 0,
            play_button_sound: false,
            armed: true,
            do_reset: false,
        }
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
enum GameState {
    NoGame,
    ShowPoints,
    ZoomingImage,
    ZoomStopped,
    ShowQuestionNoZooming,
    ShowAnswer,
    GameFinished,
}

#[tokio::main]
async fn main() -> eframe::Result<()> {
    let repaint_signal = Arc::new(Mutex::new(None::<egui::Context>)); // To store Context later
    let app_state = Arc::new(Mutex::new(AppState::default()));
    let repaint_signal_clone = repaint_signal.clone();

    let api_state = app_state.clone();

    // Start REST API server in background
    tokio::spawn(async move {
        run_api_server(api_state).await;
    });

    let app_state_for_thread = app_state.clone();
    start_buttons_task(repaint_signal_clone, app_state_for_thread);

    let options = eframe::NativeOptions {
        fullscreen: false,
        initial_window_size: Some(egui::vec2(800.0, 600.0)),
        ..Default::default()
    };
    eframe::run_native(
        "My Game Show App",
        options,
        Box::new(move |cc| {
            // Store the context so the thread can request repaints
            *repaint_signal.lock().unwrap() = Some(cc.egui_ctx.clone());
            let logo_texture = load_logo_texture(&cc.egui_ctx);
            let background_texture = load_texture(&cc.egui_ctx, "background.png");
            Box::new(MyApp::new(
                app_state.clone(),
                logo_texture,
                background_texture,
            ))
        }),
    )
}

struct MyApp {
    points: [i32; 5],
    app_state: Arc<Mutex<AppState>>,
    logo_texture: egui::TextureHandle,
    background_texture: egui::TextureHandle,
    overlay_texture: Option<egui::TextureHandle>,
    zoom: f32,
    zoom_speed: f32,
    min_zoom: f32,
    max_zoom: f32,
    image_offset: Vec2,
    zoom_timing: Instant,
    quiz: Option<Quiz>,
    current_question: usize,
    current_question_text: String,
    current_answer_text: String,
    current_managed_sound: Option<String>,
    current_question_correct_sound: Option<String>,
    question_sound_paused_for_buzz: bool,
    game_state: GameState,
    audio_player: Option<AudioPlayer>,
}

impl MyApp {
    pub fn new(
        app_state: Arc<Mutex<AppState>>,
        logo_texture: egui::TextureHandle,
        background_texture: egui::TextureHandle,
    ) -> Self {
        Self {
            points: [0; 5],
            app_state,
            logo_texture,
            background_texture,
            overlay_texture: None,
            zoom: 20.0,       // Start at maximum zoom
            zoom_speed: 0.98, // Multiplier per frame (< 1.0 means zoom out)
            min_zoom: 1.0,
            max_zoom: 20.0,
            image_offset: Vec2::new(0.0, 0.0),
            zoom_timing: Instant::now(),
            quiz: None,
            current_question: 0,
            current_question_text: "".to_string(),
            current_answer_text: "".to_string(),
            current_managed_sound: None,
            current_question_correct_sound: None,
            question_sound_paused_for_buzz: false,
            game_state: GameState::NoGame,
            audio_player: AudioPlayer::try_new(),
        }
    }

    pub fn reset_game(&mut self) {
        for x in self.points.iter_mut() {
            *x = 0;
        }
        if let Some(audio_player) = self.audio_player.as_mut() {
            audio_player.stop_managed_sound();
        }
        self.quiz = None;
        self.overlay_texture = None;
        self.current_managed_sound = None;
        self.current_question_correct_sound = None;
        self.question_sound_paused_for_buzz = false;
        self.game_state = GameState::NoGame;
        self.reset();
    }

    pub fn reset(&mut self) {
        let mut s = self.app_state.lock().unwrap();
        s.armed = true;
        s.button = Button::None;
        s.button_order = [Button::None; 5];
        s.answer_index = 0;
        s.do_reset = true;
    }

    fn advance_to_next_button(&mut self) -> Button {
        let mut state = self.app_state.lock().unwrap();
        if state.answer_index < state.button_order.len() - 1 {
            state.answer_index += 1;
            state.button = state.button_order[state.answer_index];
            if state.button != Button::None {
                state.play_button_sound = true;
            }
        } else {
            state.answer_index = state.button_order.len();
            state.button = Button::None;
        }

        state.button
    }

    fn load_image(&mut self, path: PathBuf, ctx: &egui::Context) {
        match image::open(&path) {
            Ok(img) => {
                println!("Loading image: {}", path.display());
                let size = [img.width() as usize, img.height() as usize];
                let rgba = img.to_rgba8();
                let pixels = rgba.as_flat_samples();

                let color_image = ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());

                self.overlay_texture =
                    Some(ctx.load_texture("loaded_image", color_image.clone(), Default::default()));
            }
            Err(e) => {
                println!("Failed to load image: {}", e);
            }
        }
    }

    fn next(&mut self, ctx: &egui::Context) {
        if let Some(audio_player) = self.audio_player.as_mut() {
            audio_player.stop_managed_sound();
        }
        let image = if let Some(quiz) = &self.quiz {
            println!(
                "Question {} of {} ",
                self.current_question + 1,
                quiz.questions.len()
            );
            if self.current_question >= quiz.questions.len() {
                println!("Quiz is over");
                self.game_state = GameState::GameFinished;
                return;
            }
            let index = self.current_question;
            self.current_question += 1;

            if let Some(zooming) = quiz.questions[index].shall_zoom {
                println!("Zooming {}", zooming);
                if zooming {
                    self.zoom = self.max_zoom;
                    self.game_state = GameState::ZoomingImage;
                } else {
                    self.zoom = self.min_zoom;
                    self.game_state = GameState::ShowQuestionNoZooming;
                }
            }
            self.current_question_text = quiz.questions[index].text.clone();
            self.current_answer_text = quiz.questions[index].answer.clone();
            self.current_managed_sound = quiz.questions[index].question_sound.clone();
            self.current_question_correct_sound = quiz.questions[index].correct_sound.clone();
            quiz.questions[index].image.clone()
        } else {
            self.current_managed_sound = None;
            self.current_question_correct_sound = None;
            None
        };
        if let Some(image) = image {
            let image = PathBuf::from(image.as_str());
            println!("loading image {}", image.display());
            self.load_image(image, ctx);
        } else {
            println!("No image for this question.");
            self.overlay_texture = None;
            self.game_state = GameState::ShowQuestionNoZooming;
        }
        if let (Some(audio_player), Some(sound_file)) = (
            self.audio_player.as_mut(),
            self.current_managed_sound.as_deref(),
        ) {
            audio_player.play_managed_sound(sound_file);
        }
        self.question_sound_paused_for_buzz = false;
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Get button state
        let (current_button, button_order) = {
            let mut s = self.app_state.lock().unwrap();
            if s.play_button_sound {
                s.play_button_sound = false;
                self.audio_player.as_mut().unwrap().play_one_shot_sound(match s.button {
                    Button::Red => "sound1.wav",
                    Button::Blue => "sound2.wav",
                    Button::Green => "sound3.wav",
                    Button::Yellow => "sound4.wav",
                    Button::White => "sound5.wav",
                    _ => "",
                });
            }
            (s.button, s.button_order)
        };
        if current_button != Button::None {
            if self.game_state == GameState::ZoomingImage {
                self.game_state = GameState::ZoomStopped;
            }
            if !self.question_sound_paused_for_buzz {
                if let Some(audio_player) = self.audio_player.as_ref() {
                    audio_player.pause_managed_sound();
                }
                self.question_sound_paused_for_buzz = true;
            }
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            // Draw background image -----
            let screen_rect = ui.max_rect();
            ui.painter().image(
                self.background_texture.id(),
                screen_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );

            // Draw menu buttons at the top ---------------
            ui.horizontal(|ui| {
                if ui.button("New game").clicked() {
                    self.reset_game();
                }
                if ui.button("Open quiz").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Quiz", &["json"])
                        .pick_file()
                    {
                        match get_quiz(&path) {
                            Ok(quiz) => {
                                self.quiz = Some(quiz);
                                self.current_question = 0;
                                self.current_question_text = "".to_string();
                                self.current_managed_sound = None;
                                self.current_question_correct_sound = None;
                                self.question_sound_paused_for_buzz = false;
                            }
                            Err(e) => {
                                rfd::MessageDialog::new()
                                    .set_title("Open file failed")
                                    .set_description(format!("{}", e))
                                    .show();
                            }
                        }

                        self.game_state = GameState::ShowPoints;
                    }
                }
                if self.quiz.is_some() {
                    if ui.button("Next question").clicked() {
                        self.next(ctx);
                    }
                }
            });
            // Draw The five Circles Representing Physical buttons ____________

            let colors = get_colors();

            let available_width = ui.available_width();
            let available_height = ui.available_height();

            // Calculate circle diameter based on available space
            // Calculate circle diameter based on available space (leaving room for text)
            let button_height = available_height * 0.15;
            let text_height = available_height * 0.25;
            let circle_area_height = available_height - text_height;
            let diameter = (available_width / 5.0).min(circle_area_height);
            let radius = diameter / 2.0;

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    for i in 0..5 {
                        let (clear_color, dimmed_color, button) = colors[i];
                        let color = button_display_color(
                            clear_color,
                            dimmed_color,
                            button,
                            current_button,
                            &button_order,
                        );

                        let (rect, _response) = ui.allocate_exact_size(
                            egui::vec2(available_width / 5.0, diameter),
                            egui::Sense::click(),
                        );

                        let painter = ui.painter();
                        let center = rect.center();

                        // Draw circle with radial gradient (lighter in center)
                        let outer_color = color;
                        let inner_color = lighten_color(color, 0.4);

                        // Draw multiple concentric circles to simulate gradient
                        let steps = 20;
                        for step in (0..steps).rev() {
                            let t = step as f32 / steps as f32;
                            let current_radius = radius * t;
                            let blended_color = blend_colors(inner_color, outer_color, t);
                            painter.circle_filled(center, current_radius, blended_color);
                        }

                        // painter.circle_filled(center, radius, color);

                        // Draw logo on the circle
                        let logo_size = radius * 1.4; // Logo takes up most of the circle
                        let logo_rect =
                            egui::Rect::from_center_size(center, egui::vec2(logo_size, logo_size));

                        let tint = if color == dimmed_color {
                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 100) // Dimmed logo
                        } else {
                            egui::Color32::WHITE // Clear logo
                        };

                        painter.image(
                            self.logo_texture.id(),
                            logo_rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            tint,
                        );

                        if let Some(order_number) = press_order_number(&button_order, button) {
                            let badge_radius = (radius * 0.22).max(14.0).min(36.0);
                            let badge_center =
                                egui::pos2(center.x + radius * 0.58, center.y - radius * 0.58);
                            painter.circle_filled(
                                badge_center,
                                badge_radius,
                                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 190),
                            );
                            painter.circle_stroke(
                                badge_center,
                                badge_radius,
                                egui::Stroke::new(2.0_f32, egui::Color32::WHITE),
                            );

                            let font_id = egui::FontId::proportional(badge_radius * 1.1);
                            let galley = painter.layout_no_wrap(
                                order_number.to_string(),
                                font_id,
                                egui::Color32::WHITE,
                            );
                            let text_pos = egui::pos2(
                                badge_center.x - galley.rect.width() / 2.0,
                                badge_center.y - galley.rect.height() / 2.0,
                            );
                            painter.galley(text_pos, galley);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    for i in 0..5 {
                        let (clear_color, dimmed_color, button) = colors[i];
                        let color = button_display_color(
                            clear_color,
                            dimmed_color,
                            button,
                            current_button,
                            &button_order,
                        );

                        let (rect, _response) = ui.allocate_exact_size(
                            egui::vec2(available_width / 5.0, text_height),
                            egui::Sense::hover(),
                        );

                        let painter = ui.painter();

                        // Calculate font size based on available space
                        let font_size = (text_height * 0.6).min(available_width / 10.0);

                        let text = self.points[i].to_string();
                        let font_id = egui::FontId::proportional(font_size);
                        let galley = painter.layout_no_wrap(text, font_id, color);

                        let text_pos = egui::pos2(
                            rect.center().x - galley.rect.width() / 2.0,
                            rect.center().y - galley.rect.height() / 2.0,
                        );

                        painter.galley(text_pos, galley);
                    }
                });
                // Draw Correct and Wrong buttons
                ui.add_space(button_height * 1.25);
                // Buttons row

                ui.horizontal(|ui| {
                    let button_width = available_width / 3.0 - 100.0;
                    ui.add_space((available_width - button_width * 3.0 - 20.0) / 2.0);
                    if current_button != Button::None {
                        // Correct Button (Green checkmark)
                        let correct_button = ui.add_sized(
                            [button_width, button_height],
                            egui::Button::new("").fill(egui::Color32::from_rgb(0, 180, 0)),
                        );

                        if correct_button.clicked() {
                            // Increment points for the current button
                            for (i, (_, _, b)) in colors.iter().enumerate() {
                                if *b == current_button {
                                    self.points[i] += 1;
                                    self.zoom = self.min_zoom;
                                }
                            }
                            if let Some(audio_player) = self.audio_player.as_mut() {
                                audio_player.stop_managed_sound();
                                audio_player.play_one_shot_sound("Correct.mp3");
                                let sound_file = self
                                    .current_question_correct_sound
                                    .as_deref()
                                    .unwrap_or("");
                                audio_player.play_managed_sound(sound_file);
                            }
                            self.question_sound_paused_for_buzz = false;
                            self.reset();
                            match self.game_state {
                                GameState::NoGame => {}
                                GameState::ShowPoints => {}
                                GameState::ZoomingImage
                                | GameState::ZoomStopped
                                | GameState::ShowQuestionNoZooming => {
                                    self.game_state = GameState::ShowAnswer
                                }
                                GameState::ShowAnswer => {}
                                GameState::GameFinished => {}
                            }
                        }

                        // Draw checkmark on OK button
                        let ok_rect = correct_button.rect;
                        let painter = ui.painter();
                        let center = ok_rect.center();
                        let size = ok_rect.height() * 0.4;

                        let check_stroke = egui::Stroke::new(size * 0.15, egui::Color32::WHITE);
                        painter.line_segment(
                            [
                                egui::pos2(center.x - size * 0.3, center.y),
                                egui::pos2(center.x - size * 0.05, center.y + size * 0.3),
                            ],
                            check_stroke,
                        );
                        painter.line_segment(
                            [
                                egui::pos2(center.x - size * 0.05, center.y + size * 0.3),
                                egui::pos2(center.x + size * 0.4, center.y - size * 0.35),
                            ],
                            check_stroke,
                        );
                    } else {
                        ui.add_space(button_width);
                    }

                    // If there is a quiz draw the button at bottom of screen
                    if self.quiz.is_some() {
                        ui.add_space(10.0);
                        let next_question_button = ui.add_sized(
                            [button_width, button_height],
                            egui::Button::new(match self.game_state {
                                GameState::ShowAnswer => "OK",
                                GameState::GameFinished => "Game Over",
                                _ => "Next",
                            }),
                        );

                        if current_button == Button::None && next_question_button.clicked() {
                            if self.game_state == GameState::ShowAnswer {
                                self.game_state = GameState::ShowPoints;
                                if let Some(audio_player) = self.audio_player.as_mut() {
                                    audio_player.stop_managed_sound();
                                }
                                
                            } else {
                                self.next(ctx);
                            }
                            self.reset()
                        }
                    }

                    if current_button != Button::None {
                        // Wrong answer Button (Red cross)
                        ui.add_space(10.0);
                        let not_ok_button = ui.add_sized(
                            [button_width, button_height],
                            egui::Button::new("").fill(egui::Color32::from_rgb(220, 0, 0)),
                        );

                        if not_ok_button.clicked() {
                            let next_button = self.advance_to_next_button();
                            if next_button == Button::None {
                                if self.game_state == GameState::ZoomStopped {
                                    self.game_state = GameState::ZoomingImage;
                                };
                                if let Some(audio_player) = self.audio_player.as_ref() {
                                    audio_player.play_one_shot_sound("Wrong.mp3");
                                    audio_player.resume_managed_sound();
                                }
                                self.question_sound_paused_for_buzz = false;
                            }
                        }

                        // Draw cross on NOT OK button
                        let not_ok_rect = not_ok_button.rect;
                        let painter = ui.painter();
                        let center = not_ok_rect.center();
                        let size = not_ok_rect.height() * 0.35;

                        let cross_stroke = egui::Stroke::new(size * 0.15, egui::Color32::WHITE);
                        painter.line_segment(
                            [
                                egui::pos2(center.x - size * 0.35, center.y - size * 0.35),
                                egui::pos2(center.x + size * 0.35, center.y + size * 0.35),
                            ],
                            cross_stroke,
                        );
                        painter.line_segment(
                            [
                                egui::pos2(center.x + size * 0.35, center.y - size * 0.35),
                                egui::pos2(center.x - size * 0.35, center.y + size * 0.35),
                            ],
                            cross_stroke,
                        );
                    }
                });
            });

            // Update zoom if animating ---------
            if self.game_state == GameState::ZoomingImage
                && Instant::now().duration_since(self.zoom_timing).as_millis() > 250
            {
                self.zoom_timing = Instant::now();
                self.zoom *= self.zoom_speed;
                if self.zoom < self.min_zoom {
                    self.zoom = self.min_zoom;
                    self.game_state = GameState::ShowQuestionNoZooming;
                }
            }
            ctx.request_repaint();

            // Draw owerlay if Question or answer shall be shown ---------
            if match self.game_state {
                GameState::ZoomingImage
                | GameState::ZoomStopped
                | GameState::ShowQuestionNoZooming
                | GameState::ShowAnswer => true,

                GameState::NoGame | GameState::ShowPoints | GameState::GameFinished => false,
            } {
                let screen_rect = ctx.screen_rect();

                // Calculate available space with 20% margin
                let margin = 0.20;
                let margin_x = screen_rect.width() * margin;
                let margin_y = 40.0 + (screen_rect.height() - 40.0) * margin;
                let available_width = screen_rect.width() * (1.0 - 2.0 * margin);
                let available_height = (screen_rect.height() - 40.0) * (1.0 - 2.0 * margin);

                // Define the bounded rectangle for the image
                let bounded_rect = egui::Rect::from_min_size(
                    egui::pos2(margin_x, margin_y),
                    Vec2::new(available_width, available_height),
                );
                // Create an overlay area where Image/question/answer is shown
                egui::Area::new("image_overlay")
                    .fixed_pos(bounded_rect.min)
                    .interactable(true)
                    .show(ctx, |ui| {
                        ui.set_clip_rect(bounded_rect);

                        ui.allocate_ui_at_rect(bounded_rect, |ui| {
                            // Calculate text size based on window size
                            let text_size = (available_width * 0.05).max(14.0).min(60.0);
                            let text = if self.game_state == GameState::ShowAnswer {
                                format!(
                                    "{}. Answer: {} ",
                                    self.current_question,
                                    self.current_answer_text.as_str()
                                )
                            } else {
                                format!(
                                    "{}. {}",
                                    self.current_question,
                                    self.current_question_text.as_str()
                                )
                            };
                            // Bottom panel with scaled text
                            egui::TopBottomPanel::bottom("bottom_text_panel").show_inside(
                                ui,
                                |ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.label(egui::RichText::new(text).size(text_size));
                                    });
                                },
                            );
                            // If there is an image draw it -------------------------
                            if let Some(texture) = &self.overlay_texture {
                                let texture_id = texture.id();
                                let image_size = texture.size_vec2();
                                // Central panel for image
                                egui::CentralPanel::default().show_inside(ui, |ui| {
                                    let available = ui.available_size();

                                    // Calculate display size to fit in central panel at zoom 1.0
                                    let scale = (available.x / image_size.x)
                                        .min(available.y / image_size.y);

                                    let display_size = image_size * scale * self.zoom;

                                    // Center the image
                                    let offset_x = (available.x - display_size.x) * 0.5;
                                    let offset_y = (available.y - display_size.y) * 0.5;

                                    // Use a child UI with clipping
                                    ui.scope(|ui| {
                                        ui.set_clip_rect(ui.max_rect());

                                        let image_pos =
                                            ui.min_rect().min + Vec2::new(offset_x, offset_y);

                                        let image_rect =
                                            egui::Rect::from_min_size(image_pos, display_size);
                                        // Create a sense area for the image
                                        // let response = ui.allocate_rect(image_rect, egui::Sense::click());
                                        //
                                        // if response.clicked() && self.question_done{
                                        //         self.show_question = false;
                                        //         self.overlay_texture = None;
                                        // }
                                        let mut child_ui =
                                            ui.child_ui(image_rect, egui::Layout::default());
                                        child_ui.image(texture_id, display_size);
                                    });
                                });
                            }
                        });
                    });
            }
        });
    }
}

fn button_display_color(
    clear_color: egui::Color32,
    dimmed_color: egui::Color32,
    button: Button,
    current_button: Button,
    button_order: &[Button; 5],
) -> egui::Color32 {
    if current_button == button
        || (current_button == Button::None && button_order.iter().all(|b| *b == Button::None))
    {
        clear_color
    } else {
        dimmed_color
    }
}

fn press_order_number(button_order: &[Button; 5], button: Button) -> Option<usize> {
    if button == Button::None {
        return None;
    }

    button_order
        .iter()
        .position(|ordered_button| *ordered_button == button)
        .map(|index| index + 1)
}

// REST API handlers
async fn get_pressed_button(
    state: Arc<Mutex<AppState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state = state.lock().unwrap();
    let response = serde_json::json!({
        "pressed_button": state.button
    });
    Ok(warp::reply::json(&response))
}

async fn reset(state: Arc<Mutex<AppState>>) -> Result<impl warp::Reply, warp::Rejection> {
    let mut state = state.lock().unwrap();
    state.button = Button::None;
    state.button_order = [Button::None; 5];
    state.answer_index = 0;
    state.play_button_sound = false;
    state.armed = true;
    state.do_reset = true;
    let response = serde_json::json!({
        "status": "reset",
        "message": "State has been reset"
    });
    Ok(warp::reply::json(&response))
}

// Start REST API server
async fn run_api_server(state: Arc<Mutex<AppState>>) {
    let state_filter = warp::any().map(move || state.clone());
    // CORS configuration
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "POST", "OPTIONS"])
        .allow_headers(vec!["Content-Type"]);

    // GET /getPressedButton
    let get_route = warp::path("getPressedButton")
        .and(warp::get())
        .and(state_filter.clone())
        .and_then(get_pressed_button);

    // POST /reset
    let reset_route = warp::path("reset")
        .and(warp::post())
        .and(state_filter)
        .and_then(reset);

    let routes = get_route.or(reset_route).with(cors);

    println!("Starting REST API server on http://localhost:3030");
    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

fn load_logo_texture(ctx: &egui::Context) -> egui::TextureHandle {
    // Load the logo image from file
    let img = image::open("logo.png").expect("Failed to load logo.png");

    let size = [img.width() as usize, img.height() as usize];
    let mut img_buffer = img.to_rgba8();

    // Make white pixels more transparent so circle color shows through
    for pixel in img_buffer.pixels_mut() {
        let [r, g, b, _] = pixel.0;
        // If pixel is whitish (high RGB values), reduce its alpha
        let brightness = (r as u32 + g as u32 + b as u32) / 3;
        if brightness > 200 {
            // Make white/light pixels semi-transparent
            pixel.0[3] = ((255 - brightness) as f32 * 0.5) as u8;
        }
    }

    let pixels = img_buffer.as_flat_samples();

    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());

    ctx.load_texture("knowit_logo", color_image, egui::TextureOptions::LINEAR)
}
fn load_texture(ctx: &egui::Context, file: &str) -> egui::TextureHandle {
    // Load the background image from file
    let img = image::open(file).expect("Failed to load texture file");

    let size = [img.width() as usize, img.height() as usize];
    let img_buffer = img.to_rgba8();
    let pixels = img_buffer.as_flat_samples();

    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());

    ctx.load_texture("background", color_image, egui::TextureOptions::LINEAR)
}
