//! TODO: Partial error recovery when parsing lists of exprs


use parser_helper::{
    LogosTokenStream,
    LookaheadLexer,
    SimpleError,
    new_parser,
};
use anyhow::{
    Context,
    Result,
    bail,
};
use std::ops::Fn as FnTrait;
use crate::{
    lexer::*,
    ast::*,
};


#[allow(unused)]
macro_rules! todo {
    ($data:literal)=>{
        bail!(concat!("(", file!(), ")", "[", line!(), ":", column!(), "]", " TODO: ", $data))
    };

    ()=>{
        bail!(concat!("(", file!(), ")", "[", line!(), ":", column!(), "]", " TODO"))
    };
}


pub type ParseResult<T> = std::result::Result<T, SimpleError<String>>;


new_parser!(pub struct MyParser<'a, 1, Token<'a>, LogosTokenStream<'a, Token<'a>>>);
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
    fn is_next_token(&mut self, t: Token)->bool {
        self.peek() == &t
    }

    fn start_list(&mut self)->ParseResult<()> {
        match self.next() {
            Token::List(Start)=>Ok(()),
            _=>Err(self.error("Expected `(`")),
        }
    }

    fn end_list(&mut self)->ParseResult<()> {
        match self.next() {
            Token::List(End)=>Ok(()),
            _=>Err(self.error("Expected `)`")),
        }
    }

    fn try_end_list(&mut self)->bool {
        if self.peek() == &Token::List(End) {
            self.next();
            return true;
        }
        return false;
    }

    fn start_vector(&mut self)->ParseResult<()> {
        match self.next() {
            Token::Vector(Start)=>Ok(()),
            _=>Err(self.error("Expected `[`")),
        }
    }

    fn end_vector(&mut self)->ParseResult<()> {
        match self.next() {
            Token::Vector(End)=>Ok(()),
            _=>Err(self.error("Expected `]`")),
        }
    }

    fn start_squiggle(&mut self)->ParseResult<()> {
        match self.next() {
            Token::Squiggle(Start)=>Ok(()),
            _=>Err(self.error("Expected `{`")),
        }
    }

    fn end_squiggle(&mut self)->ParseResult<()> {
        match self.next() {
            Token::Squiggle(End)=>Ok(()),
            _=>Err(self.error("Expected `}`")),
        }
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

    #[inline]
    fn error<M: Into<String>>(&self, msg: M)->SimpleError<String> {
        self.0.error(msg)
    }

    fn ident(&mut self)->ParseResult<&'a str> {
        match self.next() {
            Token::Ident(i)=>Ok(i),
            _=>Err(self.error("Unexpected token. Expected identifier")),
        }
    }

    fn dot_ident(&mut self)->ParseResult<&'a str> {
        match self.next() {
            Token::DotIdent(i)=>Ok(i),
            _=>Err(self.error("Unexpected token. Expected dot identifier")),
        }
    }

    pub fn parse_all(&mut self)->Result<Vec<Expr<'a>>> {
        let mut ret = Vec::new();

        while !self.is_next_token(Token::EOF) {
            ret.push(self.parse_expr().context("Parsing an Expr")?);
        }

        return Ok(ret);
    }

    fn parse_expr(&mut self)->Result<Expr<'a>> {
        if self.is_next_token(Token::List(Start)) {
            return self.parse_list();
        }

        match self.next() {
            Token::Number(n)=>Ok(Expr::Number(n)),
            Token::Float(f)=>Ok(Expr::Float(f)),
            Token::String(s)=>Ok(Expr::String(s)),
            Token::Char(c)=>Ok(Expr::Char(c)),
            Token::Ident(i)=>if i == "None" {
                Ok(Expr::None)
            } else {
                Ok(Expr::Ident(i))
            },
            Token::DotIdent(s)=>Ok(Expr::DotIdent(s)),
            Token::HashLiteral(lit)=>self.match_hash_literal(lit),
            Token::Comment(c)=>Ok(Expr::Comment(c)),
            Token::Quote=>self.parse_expr_quoted()
                .map(Box::new)
                .map(Expr::Quote),
            Token::Splat=>self.parse_expr()
                .map(Box::new)
                .map(Expr::Splat),

            Token::List(Start)=>bail!(format!("[{}] Unreachable code!", line!())),
            Token::List(End)=>bail!(self.error("Unexpected `)`")),
            // NOTE: Maybe change this?
            Token::Vector(_)=>bail!(self.error("Vectors are not allowed here")),
            Token::Squiggle(_)=>bail!(self.error("Squiggles are not allowed here")),
            Token::EOF=>bail!(self.error("Unexpected EOF")),
        }
    }

    fn match_hash_literal(&self, lit: &str)->Result<Expr<'static>> {
        match lit {
            "t"=>Ok(Expr::True),
            "f"=>Ok(Expr::False),
            _=>bail!(self.error(format!("Invalid #literal `{lit}`"))),
        }
    }

    fn parse_list(&mut self)->Result<Expr<'a>> {
        self.start_list()?;

        // filter out the keywords, and route them to the correct methods for parsing
        match self.peek() {
            Token::Ident(i)=>match *i {
                "fn"=>return self.parse_fn(),
                "cond"=>return self.parse_cond(),
                "def"=>return self.parse_def(),
                "set"=>return self.parse_set(),
                "defn"=>return self.parse_defn(),
                "quote"=>return self.parse_quote(),
                "begin"=>return self.parse_begin(),
                "object"=>return self.parse_object(),
                "module"=>return self.parse_module(),
                _=>{},
            },
            _=>{},
        }

        if self.try_end_list() {
            return Ok(Expr::List(Vec::new()));
        }

        let called = self.parse_expr()
            .context("List callable item")?;

        let mut items = self.parse_end_listed_items(Self::parse_expr)
            .context("List items")?;
        items.insert(0, called);

        return Ok(Expr::List(items));
    }

    fn parse_module(&mut self)->Result<Expr<'a>> {
        self.match_ident("module")?;
        let name = self.ident()?;
        self.end_list()?;
        return Ok(Expr::Module(name));
    }

    fn parse_object(&mut self)->Result<Expr<'a>> {
        self.match_ident("object")?;

        return self.parse_end_listed_items(Self::parse_object_inner)
            .map(Expr::Object)
            .context("Object fields");
    }

    fn parse_object_inner(&mut self)->Result<Field<'a>> {
        match self.peek() {
            Token::List(Start)=>{
                self.start_list()?;

                let name = self.dot_ident()?;
                let e = self.parse_expr()?;
                
                self.end_list()?;

                return Ok(Field::Full(name, e));
            },
            Token::DotIdent(_)=>Ok(Field::Shorthand(self.dot_ident()?)),
            _=>bail!("Unexpected token. Expected `(` or DotIdent"),
        }
    }

    fn parse_begin(&mut self)->Result<Expr<'a>> {
        self.match_ident("begin")?;

        return self.parse_end_listed_items(Self::parse_expr)
            .map(Expr::Begin)
            .context("Begin items");
    }

    fn parse_defn(&mut self)->Result<Expr<'a>> {
        self.match_ident("defn")?;

        let name = self.ident()
            .context("Defn name")?;

        let data = self.parse_fn_inner()
            .map(|(captures, signature)|Expr::Fn(Fn {
                name: Some(name),
                captures,
                signature,
            }))
            .map(Box::new)
            .context("Defn inner")?;

        return Ok(Expr::Def {
            name,
            data,
        });
    }

    fn parse_fn(&mut self)->Result<Expr<'a>> {
        self.match_ident("fn")?;

        let (captures, signature) = self.parse_fn_inner()?;
        return Ok(Expr::Fn(Fn {
            name: None,
            captures,
            signature,
        }));
    }

    fn parse_fn_inner(&mut self)->Result<(Option<Squiggle<'a>>, FnSignature<'a>)> {
        let captures = match self.peek() {
            Token::Squiggle(Start)=>Some(self.parse_squiggle()?),
            Token::Squiggle(End)=>bail!(self.error("Unexpected closing squiggle")),
            _=>None,
        };

        match self.peek() {
            Token::List(Start)=>{},    // we are an overloaded function, so continue.
            Token::Vector(Start)=>return self.parse_fn_param_body()
                .map(|(param, body)|(captures, FnSignature::Single(param, body))),
            _=>{
                self.next();
                bail!(self.error("Unexpected token. Expected `(` or `[`"));
            },
        }

        let variants = self.parse_end_listed_items(Self::parse_fn_overload_variant)
            .context("Fn overload variants")?;

        return Ok((captures, FnSignature::Multi(variants)));
    }

    fn parse_fn_overload_variant(&mut self)->Result<(Vector<'a>, Vec<Expr<'a>>)> {
        self.start_list()?;

        return self.parse_fn_param_body();
    }

    fn parse_fn_param_body(&mut self)->Result<(Vector<'a>, Vec<Expr<'a>>)> {
        let params = self.parse_vector()
            .context("Fn params")?;

        let body = self.parse_end_listed_items(Self::parse_expr)
            .context("Fn body")?;

        return Ok((params, body));
    }

    fn parse_cond(&mut self)->Result<Expr<'a>> {
        self.match_ident("cond")?;

        let mut conditions = self.parse_end_listed_items(Self::parse_cond_inner)
            .context("Cond conditions")?;

        let mut default = None;

        if let Some(elem) = conditions.iter().enumerate().find(|(_,(c,_))|c==&Expr::Ident("else")) {
            let index = elem.0;

            default = Some(Box::new(conditions.remove(index).1));
        }

        return Ok(Expr::Cond {
            conditions,
            default,
        });
    }

    fn parse_cond_inner(&mut self)->Result<(Expr<'a>, Expr<'a>)> {
        self.start_list()
            .context("Cond branch")?;

        let condition = self.parse_expr()
            .context("Cond branch condition")?;

        let body = self.parse_expr()
            .context("Cond branch body")?;

        self.end_list()
            .context("End cond branch")?;

        return Ok((condition, body));
    }

    fn parse_def(&mut self)->Result<Expr<'a>> {
        self.match_ident("def")?;

        let name = self.ident()
            .context("Def name")?;

        let mut data = self.parse_expr()
            .map(Box::new)
            .context("Def data")?;

        // set the name if data is a function
        match &mut *data {
            Expr::Fn(f)=>f.name = Some(name),
            _=>{},
        }

        self.end_list()
            .context("End def")?;

        return Ok(Expr::Def {
            name,
            data,
        });
    }

    fn parse_set(&mut self)->Result<Expr<'a>> {
        self.match_ident("set")?;

        let name = self.ident()
            .context("Set name")?;

        let data = self.parse_expr()
            .map(Box::new)
            .context("Set data")?;

        self.end_list()
            .context("End set")?;

        return Ok(Expr::Set {
            name,
            data,
        });
    }

    fn parse_quote(&mut self)->Result<Expr<'a>> {
        self.match_ident("quote")?;

        let quoted = self.parse_expr_quoted()
            .map(Box::new)
            .map(Expr::Quote)
            .context("Quote builtin")?;

        self.end_list().context("End quote builtin")?;

        return Ok(quoted);
    }

    fn parse_squiggle(&mut self)->Result<Squiggle<'a>> {
        self.start_squiggle()?;

        let mut items = Vec::new();

        while !self.is_next_token(Token::Squiggle(End)) {
            match self.next() {
                Token::Ident(i)=>items.push(i),
                _=>bail!(self.error("Squiggles can only have identifiers")),
            }
        }

        self.end_squiggle()?;

        return Ok(Squiggle {items});
    }

    fn parse_vector(&mut self)->Result<Vector<'a>> {
        self.start_vector()?;

        let mut items = Vec::new();
        let mut remainder = None;

        while !self.is_next_token(Token::Vector(End)) {
            match self.next() {
                Token::Ident("&")=>{
                    remainder = Some(self.ident()
                        .context("Vector remainder can only be an identifier")?
                    );

                    break;
                },
                Token::Ident(i)=>items.push(i),
                _=>bail!(self.error("Vectors can only have identifiers")),
            }
        }

        self.end_vector()?;

        return Ok(Vector {
            items,
            remainder,
        });
    }

    fn parse_expr_quoted(&mut self)->Result<Expr<'a>> {
        match self.next() {
            Token::Number(n)=>Ok(Expr::Number(n)),
            Token::Float(f)=>Ok(Expr::Float(f)),
            Token::String(s)=>Ok(Expr::String(s)),
            Token::Char(c)=>Ok(Expr::Char(c)),
            Token::DotIdent(s)=>Ok(Expr::DotIdent(s)),
            Token::Ident(i)=>if i == "None" {
                Ok(Expr::None)
            } else {
                Ok(Expr::Ident(i))
            },
            Token::HashLiteral(lit)=>self.match_hash_literal(lit),
            Token::Comment(c)=>Ok(Expr::Comment(c)),
            Token::Quote=>self.parse_expr_quoted()
                .map(Box::new)
                .map(Expr::Quote),
            Token::Splat=>self.parse_expr_quoted()
                .map(Box::new)
                .map(Expr::Splat),

            Token::List(Start)=>self.parse_end_listed_items(Self::parse_expr_quoted)
                .map(Expr::List)
                .context("Quoted list"),
            Token::Vector(Start)=>self.parse_vector()
                .map(Expr::Vector)
                .context("Quoted vector"),
            Token::Squiggle(Start)=>self.parse_squiggle()
                .map(Expr::Squiggle)
                .context("Quoted squiggle"),

            Token::Vector(End)=>bail!(self.error("Unexpected `]`")),
            Token::Squiggle(End)=>bail!(self.error("Unexpected `}`")),
            Token::List(End)=>bail!(self.error("Unexpected `)`")),
            Token::EOF=>bail!(self.error("Unexpected EOF")),
        }
    }

    fn parse_end_listed_items<T, F: FnTrait(&mut Self)->Result<T>>(&mut self, f: F)->Result<Vec<T>> {
        let mut ret = Vec::new();

        while !self.try_end_list() {
            ret.push(f(self)
                .context("Listed item")?
            );
        }

        return Ok(ret);
    }

    /// A tail recursive list parser. We won't use this version because debug builds don't optimize
    /// and we would probably stack overflow VERY FAST, but it is here as an exercise.
    #[allow(dead_code)]
    fn parse_list_end_or_expr(&mut self, mut items: Vec<Expr<'a>>)->Result<Expr<'a>> {
        if self.try_end_list() {return Ok(Expr::List(items))}

        items.push(self
            .parse_expr_quoted()
            .context("Parsing a quoted list item")?
        );

        return self.parse_list_end_or_expr(items);
    }
}


pub fn new_parser<'a>(source: &'a str)->MyParser<'a> {
    use logos::Logos;
    MyParser::new(Token::lexer(source), ())
}
