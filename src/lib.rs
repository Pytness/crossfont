//! Compatibility layer for different font engines.
//!
//! CoreText is used on macOS.
//! DirectWrite is used on Windows.
//! FreeType is used everywhere else.

#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use)]

use std::fmt::{self, Display, Formatter};
use std::ops::{Add, Mul};
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(not(any(target_os = "macos", windows)))]
extern crate harfbuzz_rs;

// If target isn't macos or windows, reexport everything from ft.
#[cfg(not(any(target_os = "macos", windows)))]
pub mod ft;
#[cfg(not(any(target_os = "macos", windows)))]
pub use ft::FreeTypeRasterizer as Rasterizer;

#[cfg(windows)]
pub mod directwrite;
#[cfg(windows)]
pub use directwrite::DirectWriteRasterizer as Rasterizer;

#[cfg(target_os = "macos")]
pub mod darwin;
#[cfg(target_os = "macos")]
pub use darwin::CoreTextRasterizer as Rasterizer;

/// Placeholder glyph key that represents a blank glyph
pub const PLACEHOLDER_GLYPH: KeyType = KeyType::Placeholder;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontDesc {
    name: String,
    style: Style,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slant {
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weight {
    Normal,
    Bold,
}

/// Style of font.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Style {
    Specific(String),
    Description { slant: Slant, weight: Weight },
}

impl fmt::Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Style::Specific(ref s) => f.write_str(s),
            Style::Description { slant, weight } => {
                write!(f, "slant={:?}, weight={:?}", slant, weight)
            },
        }
    }
}

impl FontDesc {
    pub fn new<S>(name: S, style: Style) -> FontDesc
    where
        S: Into<String>,
    {
        FontDesc { name: name.into(), style }
    }
}

impl fmt::Display for FontDesc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} - {}", self.name, self.style)
    }
}

/// Identifier for a Font for use in maps/etc.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct FontKey {
    token: u32,
}

impl FontKey {
    /// Get next font key for given size.
    ///
    /// The generated key will be globally unique.
    pub fn next() -> FontKey {
        static TOKEN: AtomicUsize = AtomicUsize::new(0);

        FontKey { token: TOKEN.fetch_add(1, Ordering::SeqCst) as _ }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct GlyphKey {
    pub character: char,
    pub font_key: FontKey,
    pub size: Size,
}

/// Captures possible outcomes of shaping, if shaping succeeded it will return a `GlyphIndex`.
/// If shaping failed or did not occur, `Fallback` will be returned.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum KeyType {
    /// A valid glyph index from Font face to be rasterized to a glyph.
    GlyphIndex(u32),
    /// A character that has not been converted to an index before rasterizing.
    Char(char),
    /// Plaeholder glyph useful when we need a glyph but it shouldn't ever render as anything.
    /// (cursors, wide_char_spacers, etc.)
    Placeholder,
}

impl Default for KeyType {
    fn default() -> Self {
        PLACEHOLDER_GLYPH
    }
}

impl From<u32> for KeyType {
    fn from(val: u32) -> Self {
        KeyType::GlyphIndex(val)
    }
}

impl From<char> for KeyType {
    fn from(val: char) -> Self {
        KeyType::Char(val)
    }
}

/// Font size stored as integer.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Size(i16);

impl Size {
    /// Create a new `Size` from a f32 size in points.
    pub fn new(size: f32) -> Size {
        Size((size * Size::factor()) as i16)
    }

    /// Scale factor between font "Size" type and point size.
    #[inline]
    pub fn factor() -> f32 {
        2.0
    }

    /// Get the f32 size in points.
    pub fn as_f32_pts(self) -> f32 {
        f32::from(self.0) / Size::factor()
    }
}

impl<T: Into<Size>> Add<T> for Size {
    type Output = Size;

    fn add(self, other: T) -> Size {
        Size(self.0.saturating_add(other.into().0))
    }
}

impl<T: Into<Size>> Mul<T> for Size {
    type Output = Size;

    fn mul(self, other: T) -> Size {
        Size(self.0 * other.into().0)
    }
}

impl From<f32> for Size {
    fn from(float: f32) -> Size {
        Size::new(float)
    }
}

#[derive(Debug, Clone)]
pub struct RasterizedGlyph {
    pub character: char,
    pub width: i32,
    pub height: i32,
    pub top: i32,
    pub left: i32,
    pub advance: (i32, i32),
    pub buffer: BitmapBuffer,
}

#[derive(Clone, Debug)]
pub enum BitmapBuffer {
    /// RGB alphamask.
    Rgb(Vec<u8>),

    /// RGBA pixels with premultiplied alpha.
    Rgba(Vec<u8>),
}

impl Default for RasterizedGlyph {
    fn default() -> RasterizedGlyph {
        RasterizedGlyph {
            character: ' ',
            width: 0,
            height: 0,
            top: 0,
            left: 0,
            advance: (0, 0),
            buffer: BitmapBuffer::Rgb(Vec::new()),
        }
    }
}

struct BufDebugger<'a>(&'a [u8]);

impl<'a> fmt::Debug for BufDebugger<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("GlyphBuffer").field("len", &self.0.len()).field("bytes", &self.0).finish()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Metrics {
    pub average_advance: f64,
    pub line_height: f64,
    pub descent: f32,
    pub underline_position: f32,
    pub underline_thickness: f32,
    pub strikeout_position: f32,
    pub strikeout_thickness: f32,
}

/// Errors occuring when using the rasterizer.
#[derive(Debug)]
pub enum Error {
    /// Unable to find a font matching the description.
    FontNotFound(FontDesc),

    /// Unable to find metrics for a font face.
    MetricsNotFound,

    /// The glyph could not be found in any font.
    MissingGlyph(RasterizedGlyph),

    /// Requested an operation with a FontKey that isn't known to the rasterizer.
    UnknownFontKey,

    /// Error from platfrom's font system.
    PlatformError(String),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::FontNotFound(font) => write!(f, "font {:?} not found", font),
            Error::MissingGlyph(glyph) => {
                write!(f, "glyph for character {:?} not found", glyph.character)
            },
            Error::UnknownFontKey => f.write_str("invalid font key"),
            Error::MetricsNotFound => f.write_str("metrics not found"),
            Error::PlatformError(err) => write!(f, "{}", err),
        }
    }
}

pub trait Rasterize {
    /// Create a new Rasterizer.
    fn new(device_pixel_ratio: f32) -> Result<Self, Error>
    where
        Self: Sized;

    /// Get `Metrics` for the given `FontKey`.
    fn metrics(&self, _: FontKey, _: Size) -> Result<Metrics, Error>;

    /// Load the font described by `FontDesc` and `Size`.
    fn load_font(&mut self, _: &FontDesc, _: Size) -> Result<FontKey, Error>;

    /// Rasterize the glyph described by `GlyphKey`..
    fn get_glyph(&mut self, _: GlyphKey) -> Result<RasterizedGlyph, Error>;

    /// Update the Rasterizer's DPI factor.
    fn update_dpr(&mut self, device_pixel_ratio: f32);

    /// Kerning between two characters.
    fn kerning(&mut self, left: GlyphKey, right: GlyphKey) -> (f32, f32);
}

#[derive(Clone, Debug)]
pub struct Info {
    pub codepoint: u32,
    pub cluster: u32,
}

/// Extends the Rasterizer with Harfbuzz specific functionality.
pub trait RasterizeExt {
    /// Shape the provided text into a set of glyphs.
    fn shape(&mut self, text: &str, font_key: FontKey) -> Vec<Info>;
}
