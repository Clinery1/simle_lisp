//! Colors and capture names copied from my fork of the Nebulous theme!
//! TODO: Make these customizable (if someone asks for this, then I will, but otherwise I won't. I
//! like these specific colors)


#![allow(dead_code)]


use crossterm::style::Color;


// Standard
const BACKGROUND: Color = Color::Rgb {r: 0x0B, g: 0x10, b: 0x15};
const RED: Color        = Color::Rgb {r: 0xFB, g: 0x46, b: 0x7B};
const BLUE: Color       = Color::Rgb {r: 0x0B, g: 0xA8, b: 0xE2};
const GREEN: Color      = Color::Rgb {r: 0x5E, g: 0xB9, b: 0x5D};
const PURPLE: Color     = Color::Rgb {r: 0x97, g: 0x5E, b: 0xEC};
const YELLOW: Color     = Color::Rgb {r: 0xFF, g: 0xCC, b: 0x00};
const ORANGE: Color     = Color::Rgb {r: 0xFF, g: 0x8D, b: 0x03};
const VIOLET: Color     = Color::Rgb {r: 0xF2, g: 0x81, b: 0xF2};
const MAGENTA: Color    = Color::Rgb {r: 0xF9, g: 0x5C, b: 0xE6};
const PINK: Color       = Color::Rgb {r: 0xDB, g: 0x73, b: 0xDA};
const WHITE: Color      = Color::Rgb {r: 0xCE, g: 0xD5, b: 0xE5};
const CYAN: Color       = Color::Rgb {r: 0x80, g: 0xA0, b: 0xFF};
const AQUA: Color       = Color::Rgb {r: 0x00, g: 0xD5, b: 0xA7};
const BLACK: Color      = Color::Rgb {r: 0x0B, g: 0x10, b: 0x15};
const GREY: Color       = Color::Rgb {r: 0x4B, g: 0x56, b: 0x66};
const LIGHTGREY: Color  = Color::Rgb {r: 0x0E, g: 0x16, b: 0x24};
const CUSTOM_1: Color   = Color::Rgb {r: 0x2D, g: 0x30, b: 0x36};
const CUSTOM_2: Color   = Color::Rgb {r: 0xAF, g: 0xFD, b: 0xF1};
const CUSTOM_3: Color   = Color::Rgb {r: 0xE2, g: 0xE7, b: 0xE6};

// Dark color
const DARKRED: Color    = Color::Rgb {r: 0xA8, g: 0x00, b: 0x46};
const DARKORANGE: Color = Color::Rgb {r: 0xA9, g: 0x5B, b: 0x00};
const DARKBLUE: Color   = Color::Rgb {r: 0x00, g: 0x69, b: 0x8F};
const DARKGREEN: Color  = Color::Rgb {r: 0x00, g: 0x5B, b: 0x09};
const DARKYELLOW: Color = Color::Rgb {r: 0x6E, g: 0x56, b: 0x00};
const DARKMAGENTA: Color= Color::Rgb {r: 0x85, g: 0x00, b: 0x7A};
const DARKCYAN: Color   = Color::Rgb {r: 0x3D, g: 0x56, b: 0xAE};
const DARKAQUA: Color   = Color::Rgb {r: 0x00, g: 0x69, b: 0x51};
const DARKGREY: Color   = Color::Rgb {r: 0x4A, g: 0x47, b: 0x47};
const DARKGREY_2: Color = Color::Rgb {r: 0x77, g: 0x73, b: 0x73};

pub const COLORS: &[(&str, Color)] = &[
    ("constant", ORANGE),
    ("constant.builtin", ORANGE),


    // Variables: cyan
    ("variable", CYAN),
    ("variable.builtin", CYAN),
    ("variable.definition", CYAN),
    ("variable.parameter", CYAN),


    // Functions: aqua
    ("function", AQUA),
    ("function.call", AQUA),
    ("function.builtin", AQUA),
    ("function.definition", AQUA),


    // OOP Things: dark aqua
    ("method", DARKAQUA),
    ("constructor", DARKAQUA),
    ("property", DARKAQUA),
    ("field", DARKAQUA),
    ("variable.member", DARKAQUA),
    ("variable.member.definition", DARKAQUA),


    // Macro things: violet and magenta
    ("constant.macro", VIOLET),
    ("function.macro", MAGENTA),


    // Comments: dark grey
    ("comment", DARKGREY),
    ("comment.documentation", GREY),
    ("todo", YELLOW),
    ("comment.todo", YELLOW),
    ("comment.note", YELLOW),


    // Numbers: orange
    ("float", ORANGE),
    ("number", ORANGE),


    // Boolean: dark yellow
    ("boolean", DARKYELLOW),


    // String and char: green
    ("character", GREEN),
    ("string", GREEN),
    ("string.escape", ORANGE),
    ("string.regex", RED),


    // Types: yellow
    ("type", YELLOW),
    ("type.builtin", YELLOW),
    ("type.definition", YELLOW),
    // this is the "mut" keyword in Rust
    ("type.qualifier", YELLOW),
    ("structure", YELLOW),


    // Statements
    ("conditional", DARKRED),


    // Unstyled: white
    ("operator", WHITE),


    // Errors: dark red
    ("exception", DARKRED),
    ("error", DARKRED),


    // Misc
    ("tag.delimiter", DARKGREEN),
    ("keyword", RED),
    ("keyword.function", RED),
    ("keyword.import", RED),
    ("keyword.conditional", RED),
    ("label", DARKGREEN),


    // Punctuation
    ("punctuation", ORANGE),
    ("punctuation.delimiter", ORANGE),
    ("punctuation.bracket", RED),
    ("punctuation.special", YELLOW),


    // Imports: purple
    ("include", PURPLE),
    ("module", PURPLE),


    // Markup / Markdown
    ("markup.heading", BLUE),
    ("markup.list", BLUE),
    ("markup.italic", WHITE),
    ("markup.strong", WHITE),
    ("markup.raw", GREY),
    ("markup.link", GREY),
    ("markup.link.label", CYAN),
    ("markup.link.url", DARKCYAN),
    ("punctuation.special.markdown", YELLOW),
];
