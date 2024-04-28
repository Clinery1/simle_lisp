use parser_helper::Token as TokenTrait;
use logos::{
    Logos,
    Lexer,
};
pub use StartOrEnd::*;


#[derive(Debug, Logos, PartialEq)]
#[logos(skip "[ \t\r\n]")]
pub enum Token<'a> {
    #[regex("[^ \t\r\n()\"'#0-9][^ \t\r\n()\"]*")]
    Ident(&'a str),

    #[regex("[0-9][0-9_]*", number)]
    Number(i64),

    #[regex("[0-9][0-9_]*\\.[0-9][0-9_]*", float)]
    Float(f64),

    #[token("\"", string)]
    String(String),

    #[token("'")]
    Quote,

    #[regex("#[^ \t\r\n()\"]+", strip_first)]
    HashLiteral(&'a str),

    #[token("(", |_|Start)]
    #[token(")", |_|End)]
    Paren(StartOrEnd),

    #[regex(";[^\n]", strip_first, priority=10)]
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
