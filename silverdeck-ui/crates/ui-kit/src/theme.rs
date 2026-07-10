//! SilverDeck's fixed dark console palette.

use gpui::{rgb, rgba, Rgba};

pub fn bg() -> Rgba {
    rgb(0x0b0f14)
}
pub fn panel() -> Rgba {
    rgb(0x141b24)
}
pub fn panel_hi() -> Rgba {
    rgb(0x1d2733)
}
pub fn accent() -> Rgba {
    rgb(0x38bdf8)
}
pub fn accent_dim() -> Rgba {
    rgb(0x0e7490)
}
pub fn text() -> Rgba {
    rgb(0xe5eaf0)
}
pub fn text_dim() -> Rgba {
    rgb(0x8b98a5)
}
pub fn ok() -> Rgba {
    rgb(0x4ade80)
}
pub fn err() -> Rgba {
    rgb(0xf87171)
}
pub fn scrim() -> Rgba {
    rgba(0x000000cc)
}
