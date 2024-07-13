use parser_helper::Token as TokenTrait;
use logos::{
    Logos,
    Lexer,
};
pub use StartOrEnd::*;


#[derive(Debug, Logos, PartialEq)]
#[logos(skip "[ \t\r\n]")]
pub enum Token<'a> {
    /// all but whitespace, (), [], `#`, `'`, and `"`
    #[regex("[^/:;\\\\ .\t\r\n()\\[\\]{}\"'#0-9][^/ .\t\r\n()\\[\\]{}\"]*")]
    Ident(&'a str),

    #[regex("[^/:;\\\\ .\t\r\n()\\[\\]{}\"'#0-9][^/ .\t\r\n()\\[\\]{}\"]*/", parse_path)]
    Path(Vec<&'a str>),

    #[regex("\\.[^ .\t\r\n()\\[\\]{}\"]+", strip_first)]
    DotIdent(&'a str),

    #[regex("[0-9][0-9_]*", number)]
    Number(i64),

    #[regex("[0-9][0-9_]*\\.[0-9][0-9_]*", float)]
    Float(f64),

    #[token("\"", string)]
    String(String),

    #[regex("#x[0-9A-Fa-f]{2}", parse_byte_hex)]
    #[regex("#b[01]{1,8}", parse_byte_bin)]
    Byte(u8),

    #[token("\\space", |_|' ')]
    #[token("\\newline", |_|'\n')]
    #[token("\\tab", |_|'\n')]
    #[token("\\", parse_char)]
    Char(char),

    #[token("'")]
    Quote,

    #[token("...", priority = 10)]
    Splat,  // ...(and the list went SPLAT!)

    #[regex("#[a-zA-Z_]+", strip_first)]
    HashLiteral(&'a str),

    #[regex(":[^ .\t\r\n()\\[\\]{}\"]*", strip_first)]
    ReplDirective(&'a str),

    #[token("(", |_|Start)]
    #[token(")", |_|End)]
    List(StartOrEnd),

    #[token("[", |_|Start)]
    #[token("]", |_|End)]
    Vector(StartOrEnd),

    #[token("{", |_|Start)]
    #[token("}", |_|End)]
    Squiggle(StartOrEnd),

    #[regex(";[^\n]*", strip_first, priority=10)]
    Comment(&'a str),

    EOF,
}
impl<'a> TokenTrait for Token<'a> {
    fn eof()->Self {Self::EOF}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum StartOrEnd {
    Start,
    End,
}


fn number<'a>(l: &mut Lexer<'a, Token<'a>>)->Option<i64> {
    l.slice()
        .chars()
        .filter(|c|match c {
            '0'..='9'=>true,
            _=>false,
        })
        .collect::<String>()
        .parse::<i64>()
        .ok()
}

fn float<'a>(l: &mut Lexer<'a, Token<'a>>)->Option<f64> {
    l.slice()
        .chars()
        .filter(|c|match c {
            '0'..='9'|'.'=>true,
            _=>false,
        })
        .collect::<String>()
        .parse::<f64>()
        .ok()
}

fn string<'a>(l: &mut Lexer<'a, Token<'a>>)->Option<String> {
    let mut s = String::new();
    let mut escape = false;
    let mut valid = false;

    for c in l.remainder().chars() {
        l.bump(c.len_utf8());

        if escape {
            escape = false;
            match c {
                '"'=>s.push('"'),
                '\\'=>s.push('\\'),
                't'=>s.push('\t'),
                'r'=>s.push('\r'),
                'n'=>s.push('\n'),
                '0'=>s.push('\0'),
                _=>valid = false,
            }
        } else {
            match c {
                '"'=>break,
                '\\'=>escape = true,
                _=>s.push(c),
            }
        }
    }

    if valid {return None}

    return Some(s);
}

fn strip_first<'a>(l: &mut Lexer<'a, Token<'a>>)->&'a str {
    &l.slice()[1..]
}

fn parse_char<'a>(l: &mut Lexer<'a, Token<'a>>)->Option<char> {
    let c = l.remainder().chars().next()?;
    l.bump(c.len_utf8());
    return Some(c);
}

fn parse_path<'a>(l: &mut Lexer<'a, Token<'a>>)->Vec<&'a str> {
    let mut out = Vec::new();
    let len = l.slice().len();
    out.push(&l.slice()[..(len - 1)]);

    let mut slice_start = 0;
    let mut count = 0;
    for c in l.remainder().chars() {
        if slice_start == count {
            match c {
                '/'=>panic!("Cannot have a path section of zero length!"),
                ':'|
                    ';'|
                    '\\'|
                    ' '|
                    '.'|
                    '\t'|
                    '\r'|
                    '\n'|
                    '('|
                    ')'|
                    '['|
                    ']'|
                    '{'|
                    '}'|
                    '"'|
                    '\''|
                    '#'|
                    '0'..='9'=>break,
                _=>{},
            }
        } else {
            match c {
                '/'=>{
                    out.push(&l.remainder()[slice_start..count]);
                    slice_start = count + 1;
                },
                ':'|
                    ';'|
                    '\\'|
                    ' '|
                    '.'|
                    '\t'|
                    '\r'|
                    '\n'|
                    '('|
                    ')'|
                    '['|
                    ']'|
                    '{'|
                    '}'|
                    '"'|
                    '\''=>break,
                _=>{},
            }
        }
        count += c.len_utf8()
    }
    out.push(&l.remainder()[slice_start..count]);

    l.bump(count);

    return out;
}

fn parse_byte_hex<'a>(lex: &mut Lexer<'a, Token<'a>>)->Option<u8> {
    let hex = &lex.slice()[2..];
    u8::from_str_radix(hex, 16).ok()
}

fn parse_byte_bin<'a>(lex: &mut Lexer<'a, Token<'a>>)->Option<u8> {
    let bin = &lex.slice()[2..];
    u8::from_str_radix(bin, 2).ok()
}
