//! Minimal layout types for native chrome (page content is WebView2).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayMode {
    Block,
    Inline,
    Flex,
    None,
}

#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BG: Self = Self { r: 0.102, g: 0.102, b: 0.102, a: 1.0 };
    pub const CARD_BG: Self = Self { r: 0.106, g: 0.114, b: 0.122, a: 1.0 };
    pub const ACCENT: Self = Self { r: 0.0, g: 0.851, b: 0.949, a: 1.0 };
    pub const ONION: Self = Self { r: 0.55, g: 0.35, b: 0.85, a: 1.0 };
    pub const INPUT: Self = Self { r: 0.05, g: 0.05, b: 0.05, a: 1.0 };
}

#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: Color,
    pub text: Option<String>,
    pub link: Option<String>,
    pub display: DisplayMode,
}

/// Fixed chrome height above the WebView content area.
pub const CHROME_HEIGHT: f32 = 80.0;
