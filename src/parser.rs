use parser_helper::{
    LogosTokenStream,
    LookaheadLexer,
    Span,
    SimpleError,
    new_parser,
};
use anyhow::{
    Context,
    Result,
    bail,
};
use crate::{
    lexer::*,
    ast::*,
};


macro_rules! todo {
    ($data:literal)=>{
        bail!(concat!("(", file!(), ")", "[", line!(), ":", column!(), "]", " TODO: ", $data));
    };

    ()=>{
        bail!(concat!("(", file!(), ")", "[", line!(), ":", column!(), "]", " TODO"));
    };
}


pub type ParseResult<T> = std::result::Result<T, SimpleError<String>>;


new_parser!(pub struct MyParser<'a, 2, Token<'a>, LogosTokenStream<'a, Token<'a>>>);
impl<'a> MyParser<'a> {
    #[inline]
    fn next(&mut self)->Token<'a> {
        self.take_token()
    }

    #[inline]
    fn peek(&mut self)->&Token<'a> {
        self.lookahead(0)
    }

    #[inline]
    fn peek1(&mut self)->&Token<'a> {
        self.lookahead(1)
    }

    fn match_ident(&mut self, i: &str)->ParseResult<()> {
        match self.next() {
            Token::Ident(ti)=>if ti == i {
                Ok(())
            } else {
                Err(self.error(format!("Expected keyword `{i}`")))
            },
            _=>Err(self.error("Expected identifier")),
        }
    }

    fn is_next_token(&mut self, t: Token)->bool {
        self.peek() == &t
    }

    pub fn parse_all(&mut self)->Result<Vec<Expr<'a>>> {
        let mut ret = Vec::new();

        while !self.is_next_token(Token::EOF) {
            ret.push(self.parse_expr().context("Parsing an Expr")?);
        }

        return Ok(ret);
    }

    fn parse_expr(&mut self)->Result<Expr<'a>> {
        todo!();
    }

    fn parse_list(&mut self)->Result<Expr<'a>> {
    }
}


pub fn new_parser<'a>(source: &'a str)->MyParser<'a> {
    use logos::Logos;
    MyParser::new(Token::lexer(source), ())
}
