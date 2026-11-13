use serde::Serialize;

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize)]
pub enum Button {
    Yellow,
    Red,
    Blue,
    Green,
    White,
    None,
}
