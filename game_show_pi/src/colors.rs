use crate::button_states::Button;
use eframe::egui::Color32;

pub fn get_colors() -> [(Color32, Color32, Button); 5] {
    let colors = [
        (
            Color32::from_rgb(255, 255, 0),
            Color32::from_rgb(64, 64, 0),
            Button::Yellow,
        ), // Yellow
        (
            Color32::from_rgb(255, 0, 0),
            Color32::from_rgb(64, 0, 0),
            Button::Red,
        ), // Red
        (
            Color32::from_rgb(0, 255, 0),
            Color32::from_rgb(0, 64, 0),
            Button::Green,
        ), // Green
        (
            Color32::from_rgb(0, 0, 255),
            Color32::from_rgb(0, 0, 64),
            Button::Blue,
        ), // Blue
        (
            Color32::from_rgb(255, 255, 255),
            Color32::from_rgb(64, 64, 64),
            Button::White,
        ), // White
    ];
    colors
}

pub fn lighten_color(color: Color32, amount: f32) -> Color32 {
    let r = color.r() as f32;
    let g = color.g() as f32;
    let b = color.b() as f32;

    let r_light = (r + (255.0 - r) * amount).min(255.0) as u8;
    let g_light = (g + (255.0 - g) * amount).min(255.0) as u8;
    let b_light = (b + (255.0 - b) * amount).min(255.0) as u8;

    Color32::from_rgb(r_light, g_light, b_light)
}

pub fn blend_colors(color1: Color32, color2: Color32, t: f32) -> Color32 {
    let r = (color1.r() as f32 * (1.0 - t) + color2.r() as f32 * t) as u8;
    let g = (color1.g() as f32 * (1.0 - t) + color2.g() as f32 * t) as u8;
    let b = (color1.b() as f32 * (1.0 - t) + color2.b() as f32 * t) as u8;

    Color32::from_rgb(r, g, b)
}
