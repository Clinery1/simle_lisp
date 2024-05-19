use anyhow::Result;
use ropey::{
    Rope,
    RopeSlice,
};
use crossterm::{
    terminal::{
        BeginSynchronizedUpdate,
        EndSynchronizedUpdate,
        SetTitle,
        Clear,
        ClearType,
        ScrollUp,
        enable_raw_mode,
        disable_raw_mode,
        size as terminal_size,
    },
    event::{
        Event,
        KeyCode,
        KeyModifiers,
        read as read_event,
    },
    style::{
        Color,
        Stylize,
    },
    cursor::{
        MoveDown,
        MoveToColumn,
        MoveToRow,
        Show as ShowCursor,
        position as cursor_position,
    },
    execute,
    queue,
};
use tree_sitter::{
    QueryCursor,
    Node,
    Parser as TsParser,
    Query as TsQuery,
};
use std::{
    io::{
        Stdout,
        Write,
    },
    time::Instant,
    fs::read_to_string,
    collections::HashMap,
    sync::OnceLock,
    mem,
};
use crate::{
    interpreter::{
        ast::{
            ConvertState,
            InstructionId,
            repl_convert,
        },
        data::Data,
        Interpreter,
    },
    parser::{
        ReplContinue,
        repl_new_parser,
        new_parser,
    },
    ast::Expr,
    error_trace,
};


mod colors;


const HIGHLIGHT_QUERY: &str = include_str!("highlights.scm");


static COLOR_MAP: OnceLock<Vec<Color>> = OnceLock::new();


enum ReplDirective<'a> {
    Exit,
    Help,
    Include(&'a str),
}


#[derive(Copy, Clone)]
pub struct Cursor {
    pub line: usize,
    pub col: usize,
}

pub struct Repl {
    state: ConvertState,
    interpreter: Interpreter,
    history: Vec<String>,
    stdout: Stdout,
    ts_parser: TsParser,
    ts_query: TsQuery,
    rope: Rope,
    cursor_idx: usize,
    indent_level: usize,
    cursor: Cursor,
}
impl Repl {
    pub fn new()->Self {
        let mut ts_parser = TsParser::new();
        let lang = tree_sitter_simplelisp::language();
        ts_parser.set_language(&lang).expect("Error loading simplelisp grammar");
        let ts_query = TsQuery::new(&lang, HIGHLIGHT_QUERY)
            .expect("Could not load builtin simplelisp highlight query");
        let mut state = ConvertState::new();
        state.reserve_module();

        // Generate the color map at runtime. This is easier since I don't have to worry about the
        // thing desyncing if I change the `highlights.scm` file or `colors::COLORS` array.
        let raw_color_map: HashMap<&str, Color> = colors::COLORS.into_iter().copied().collect();
        let mut color_map = Vec::new();
        for cap in ts_query.capture_names() {
            let color = raw_color_map.get(cap).copied().unwrap();
            color_map.push(color);
        }
        COLOR_MAP.set(color_map).unwrap();

        Repl {
            interpreter: Interpreter::new(&mut state),
            state,
            history: Vec::new(),
            stdout: std::io::stdout(),
            ts_parser,
            ts_query,
            rope: Rope::new(),
            cursor_idx: 0,
            indent_level: 0,
            cursor: Cursor {
                line: 0,
                col: 0,
            },
        }
    }

    fn reset_cursor(&mut self) {
        self.cursor.line = 0;
        self.cursor.col = 0;
        self.cursor_idx = 0;
        self.indent_level = 0;
    }

    // ----- Adding chars or strings things

    fn add_char(&mut self, c: char) {
        if c == '\n' {return self.newline()}
        if self.cursor_idx == self.line().len_chars() && self.cursor.line > 0 {
            self.rope.insert_char(self.cursor_idx.saturating_sub(1), c);
        } else {
            self.rope.insert_char(self.cursor_idx, c);
        }
        self.cursor_right();
    }

    fn newline(&mut self) {
        self.rope.insert_char(self.cursor_idx, '\n');
        self.cursor_down();
        self.cursor_home();
        for _ in 0..(self.indent_level * 4) {
            self.add_char(' ');
        }
    }

    #[inline]
    fn add_str(&mut self, s: &str) {
        s.chars()
            .for_each(|c|self.add_char(c));
    }

    // ----- Cursor things

    fn compute_cursor_idx(&mut self) {
        let line = self.rope.line(self.cursor.line.min(self.rope.len_lines().saturating_sub(1)));
        let line_char_idx = self.rope.line_to_char(self.cursor.line);

        let char_idx = self.cursor.col.min(line.len_chars());

        self.cursor_idx = line_char_idx + char_idx;
    }

    fn cursor_up(&mut self) {
        if self.cursor.line != 0 {
            self.cursor.line -= 1;
            self.compute_cursor_idx();
        }
    }

    fn cursor_down(&mut self) {
        if self.cursor.line + 1 < self.rope.len_lines() {
            self.cursor.line += 1;
            self.compute_cursor_idx();
        } else if self.cursor.line >= self.rope.len_lines() {
            self.cursor.line = self.rope.len_lines() - 1;
            self.compute_cursor_idx();
        }
    }

    fn cursor_left(&mut self) {
        if self.cursor.col > 0 {
            let line = self.line();
            let mut line_end = line.len_chars().saturating_sub(1);
            if self.line_ends_with_nl() {
                line_end = line_end.saturating_sub(1);
            }
            if self.cursor.col > line_end {
                self.cursor.col = line_end;
                self.compute_cursor_idx();
            } else {
                self.cursor.col -= 1;
            }
            self.compute_cursor_idx();
        }
    }

    fn cursor_right(&mut self) {
        if self.rope.len_chars() > 0 {
            if self.char() != '\n' {
                let line = self.rope.line(self.cursor.line);
                if self.cursor.col < line.len_chars() {
                    self.cursor.col += 1;
                }
                self.compute_cursor_idx();
            }
        }
    }

    fn cursor_home(&mut self) {
        self.cursor.col = 0;
        self.compute_cursor_idx();
    }

    fn cursor_end(&mut self) {
        // let line = self.line();
        self.cursor.col = usize::MAX;

        self.compute_cursor_idx();
    }

    // ----- Removal things

    fn backspace(&mut self) {
        if self.cursor_idx == 0 {
            return;
        }
        self.cursor_idx -= 1;

        self.delete();
        if self.cursor.col == 0 {
            self.cursor.line -= 1;
            self.cursor_end();
        } else {
            self.cursor.col -= 1;
        }
    }

    #[inline]
    fn delete(&mut self) {
        if self.cursor_idx >= self.rope.len_chars() {return}
        self.rope.remove(self.cursor_idx..=self.cursor_idx);
    }

    // ----- Indexing helpers

    fn line_ends_with_nl(&self)->bool {
        let line = self.line();
        if line.len_chars() == 0 {return false}
        line.char(line.len_chars() - 1) == '\n'
    }
    
    // #[inline]
    // fn prev_char(&self)->char {
    //     self.rope.char(self.cursor_idx.saturating_sub(1))
    // }

    #[inline]
    fn char(&self)->char {
        self.rope.char(self.cursor_idx.min(self.rope.len_chars() - 1))
    }

    fn line(&self)->RopeSlice {
        let line = self.cursor.line.min(self.rope.len_lines().saturating_sub(1));
        self.rope.line(line)
    }

    // ----- Display/Input handling things

    fn prompt_line(&mut self)->Result<()> {
        let mut prev_row = cursor_position()?.1;
        let mut prev_lines = self.rope.len_lines();

        let mut prev_rope: Option<(Cursor, Rope)> = None;
        let mut history_item = self.history.len();

        prev_row = self.render_buffer(prev_row, 0)?;

        enable_raw_mode().unwrap();

        loop {
            match read_event()? {
                Event::Key(key_event)=>{
                    let shift = key_event.modifiers == KeyModifiers::SHIFT;
                    if key_event.modifiers.is_empty() || shift {
                        match key_event.code {
                            KeyCode::Backspace=>self.backspace(),
                            KeyCode::Delete=>self.delete(),
                            KeyCode::Enter=>{
                                self.newline();
                                if self.check_code() {
                                    break;
                                }
                            },

                            // Ensure Shift works as expected, regardless of what char we get
                            KeyCode::Char(c)=>if shift {
                                self.add_char(c.to_ascii_uppercase());
                            } else {
                                self.add_char(c);
                            },

                            // Indent things
                            KeyCode::BackTab=>{ // Shift + Tab
                                if self.rope.len_chars() != 0 {
                                let mut prev_cursor = self.cursor;
                                    self.cursor_home();
                                    for _ in 0..4 {
                                        if self.rope.len_chars() == 0 {break}
                                        if self.char() != ' ' {break}
                                        self.delete();
                                        prev_cursor.col = prev_cursor.col.saturating_sub(1);
                                    }
                                    self.indent_level = self.indent_level.saturating_sub(1);
                                    self.cursor = prev_cursor;
                                    self.compute_cursor_idx();
                                }
                            },
                            KeyCode::Tab=>{
                                let mut prev_cursor = self.cursor;
                                prev_cursor.col += 4;
                                self.indent_level += 1;
                                self.cursor_home();
                                self.add_str("    ");
                                self.cursor = prev_cursor;
                                self.compute_cursor_idx();
                            },

                            KeyCode::Left=>self.cursor_left(),
                            KeyCode::Right=>self.cursor_right(),
                            KeyCode::Up=>self.cursor_up(),
                            KeyCode::Down=>self.cursor_down(),
                            KeyCode::Home=>self.cursor_home(),
                            KeyCode::End=>self.cursor_end(),
                            KeyCode::PageUp=>{
                                if history_item > 0 {
                                    history_item = (history_item - 1).min(self.history.len().saturating_sub(1));
                                    if history_item < self.history.len() {
                                        let new_rope = Rope::from(self.history[history_item].as_str());
                                        prev_rope = Some((self.cursor, mem::replace(&mut self.rope, new_rope)));
                                        self.reset_cursor();
                                    }
                                }
                            },
                            KeyCode::PageDown=>{
                                history_item = (history_item + 1).min(self.history.len().max(1));
                                if history_item < self.history.len() {
                                    self.rope = Rope::from(self.history[history_item].as_str());
                                    self.reset_cursor();
                                } else if prev_rope.is_some() {
                                    let (cursor, rope) = prev_rope.take().unwrap();
                                    self.rope = rope;
                                    self.cursor = cursor;
                                }
                            },

                            _=>{},
                        }
                    } else if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                        match key_event.code {
                            KeyCode::Char('d'|'D')=>break,
                            KeyCode::Char('w'|'W')=>{
                                if self.rope.len_chars() > 0 {
                                    match self.char() {
                                        '/'|':'|';'|'\\'|'.'|'\t'|'\r'|'\n'|'('|')'|'['|']'|'{'|'}'|'"'|'\''|'#'=>{
                                            self.backspace();
                                        },
                                        _=>{
                                            while self.rope.len_chars() > 0 {
                                                match self.char() {
                                                    '/'|':'|';'|'\t'|'\r'|'\n'|'('|')'|'['|']'|'{'|'}'|'"'|'\''|'#'=>{
                                                        break;
                                                    },
                                                    _=>self.backspace(),
                                                }
                                            }
                                        },
                                    }
                                }
                            },
                            _=>{},
                        }
                    }
                },
                _=>{},
            }

            prev_row = self.render_buffer(prev_row, prev_lines)?;
            prev_lines = self.rope.len_lines();
        }

        disable_raw_mode().unwrap();

        return Ok(());
    }

    fn render_buffer(&mut self, mut prev_row: u16, prev_lines: usize)->Result<u16> {
        queue!(&mut self.stdout, BeginSynchronizedUpdate)?;
        // TODO: treesitter highlighting!
        let last_line = self.rope.len_lines().saturating_sub(1);
        let lines = self.rope.len_lines();
        let line_num_cols = match lines {
            0..=9=>1,
            10..=99=>2,
            100..=999=>4,
            _=>panic!("Too many lines!"),
        };

        let size = terminal_size()?.1;
        let mut position = prev_row;

        if prev_lines > 0 {
            queue!(&mut self.stdout, MoveToRow(prev_row), ShowCursor)?;
            for _ in 0..prev_lines {
                queue!(&mut self.stdout, Clear(ClearType::CurrentLine), MoveDown(1))?;
            }
            queue!(&mut self.stdout, MoveToRow(prev_row))?;
        }

        let tree = self.ts_parser.parse_with(&mut |offset, _|{
            let rope = &self.rope;
            if offset >= rope.len_bytes() {
                ""
            } else {
                let (s, chunk_start_idx, ..) = rope.chunk_at_byte(offset);
                let offset_into_chunk = offset - chunk_start_idx;
                &s[offset_into_chunk..]
            }
        }, None).expect("Could not generate TS tree");

        let mut query_cursor = QueryCursor::new();

        let text_provider = |node: Node|{
            let rope = &self.rope;
            rope.get_byte_slice(node.byte_range())
                .map(|s|s.to_string())
                .into_iter()
        };

        let matches = query_cursor.captures(&self.ts_query, tree.root_node(), text_provider);

        let mut captures_iter = matches
            .map(|(each_match, _)|each_match.captures.iter())
            .flatten()
            .map(|cap|{
                let color = COLOR_MAP.get().unwrap()[cap.index as usize];
                let range = cap.node.byte_range();
                (range.end, color)
            });

        let mut byte_idx = 0;

        // (capture_end_byte_idx, color)
        let mut current_capture: Option<(usize, Color)> = captures_iter.next();
        let default_cap = (usize::MAX, Color::White);
        for (i, line) in self.rope.lines().enumerate() {
            queue!(&mut self.stdout, MoveToColumn(0))?;
            if lines == 1 {
                write!(&mut self.stdout, "> ")?;
            } else {
                write!(&mut self.stdout, "{:<line_num_cols$} ", i + 1)?;
            }
            let (mut cap_end, mut color) = current_capture.unwrap_or(default_cap);

            for c in line.chars() {
                byte_idx += c.len_utf8();
                while cap_end < byte_idx {
                    current_capture = captures_iter.next();
                    (cap_end, color) = current_capture.unwrap_or(default_cap);
                }

                if c == '\n' {break}
                write!(&mut self.stdout, "{}", c.with(color))?;
            }

            if i != last_line {
                if position == (size - 1) {
                    prev_row -= 1;
                    queue!(&mut self.stdout, ScrollUp(1))?;
                } else {
                    position += 1;
                }
                queue!(&mut self.stdout, MoveDown(1))?;
            }
        }

        let move_up = prev_row + self.cursor.line as u16;
        if move_up > 0 {
            queue!(&mut self.stdout, MoveToRow(move_up))?;
        }

        let line = self.line();
        let char_offset = self.cursor.col.min(line.len_chars());
        let move_right = 1 + (line_num_cols as u16) + char_offset as u16;
        if move_right > 0 {
            let mut offset = 0;
            if line.len_chars() != 0 {
                if self.line_ends_with_nl() && char_offset == line.len_chars() {
                    offset = 1;
                }
            }
            queue!(&mut self.stdout, MoveToColumn(move_right.saturating_sub(offset)))?;
        }

        queue!(&mut self.stdout, EndSynchronizedUpdate)?;
        self.stdout.flush()?;

        return Ok(prev_row);
    }

    /// A simple check to see if something *may* be successful
    fn check_code(&self)->bool {
        let mut depth = 0;

        let mut iter = self.rope.chars();

        while let Some(c) = iter.next() {
            match c {
                '{'=>loop {
                    match iter.next() {
                        Some('}')=>break,
                        Some(_)=>{},
                        None=>return false,
                    }
                },
                '['=>loop {
                    match iter.next() {
                        Some(']')=>break,
                        Some(_)=>{},
                        None=>return false,
                    }
                },
                '('=>depth += 1,
                ')'=>if depth == 0 {
                    return false;
                } else {
                    depth -= 1;
                },
                '"'=>{
                    let mut escape = false;
                    loop {
                        match iter.next() {
                            Some('\\')=>escape = true,
                            Some('"')=>if !escape {break},
                            Some(_)=>{},
                            None=>return false,
                        }
                        if escape {escape = false}
                    }
                },
                '\\'=>{
                    iter.next();
                },
                _=>{},
            }
        }

        return depth == 0;
    }

    pub fn run(&mut self, debug: u8, stats_for_nerds: bool) {     // TODO: Debug and stats for nerds
        println!("Welcome to the slp REPL!");
        println!("To exit the repl, press <Ctrl+d> or execute `:exit`");
        println!("For help, execute `:help`");

        execute!(&mut self.stdout, SetTitle("Simplelisp REPL")).unwrap();

        // Read
        'repl:loop {
            self.reset_cursor();

            self.prompt_line().unwrap();
            if self.rope.len_chars() == 0 {break 'repl}

            let mut source = self.rope.to_string();
            while source.ends_with(['\n', ' ']) {
                source.pop();
            }
            println!();

            // Eval(parse)
            let mut parser = repl_new_parser(source.as_str());
            let parse_start = Instant::now();
            let start_id = match parser.parse_all() {
                Ok(exprs)=>{
                    if stats_for_nerds {
                        let time = parse_start.elapsed();
                        println!("Parse time: {time:?}");
                    }
                    drop(parser);


                    if debug >= 1 {
                        println!("{} root AST nodes", exprs.len());
                    }

                    if debug >= 2 {
                        for expr in exprs.iter() {
                            println!("{expr:#?}");
                        }
                    }

                    let ret = if exprs.len() == 1 {
                        match match_repl_directive(&exprs) {
                            Ok(Some(dir))=>match dir {
                                ReplDirective::Help=>{
                                    print_repl_help();
                                    self.history.push(source);
                                    self.rope = Rope::new();
                                    continue 'repl;
                                },
                                ReplDirective::Exit=>break 'repl,
                                ReplDirective::Include(name)=>Some(include_file(&mut self.state, name).unwrap()),
                            },
                            Ok(None)=>None,
                            Err(_)=>{
                                self.history.push(source);
                                self.rope = Rope::new();
                                continue 'repl;
                            },
                        }
                    } else {None};

                    if let Some(out) = ret {
                        out
                    } else {
                        match repl_convert(&mut self.state, exprs) {
                            Ok(start_id)=>start_id,
                            Err(e)=>{
                                error_trace(e, &source, "<REPL>");
                                self.history.push(source);
                                self.rope = Rope::new();
                                continue 'repl;
                            },
                        }
                    }
                },
                Err(e)=>{
                    drop(parser);

                    // if the line looks unfinished, then dont clear it, and don't throw an
                    // error
                    if e.root_cause().downcast_ref::<ReplContinue>().is_some() {
                        continue 'repl;
                    }

                    error_trace(e, source.as_str(), "<REPL>");
                    self.history.push(source);
                    self.rope = Rope::new();
                    continue 'repl;
                },
            };

            // Eval(execute)
            let start_ins_count = self.interpreter.metrics.instructions_executed;
            match self.interpreter.run(&mut self.state, Some(start_id)) {
                // Print
                Ok(Some(dr))=>{
                    if stats_for_nerds {
                        println!("Run time: {:?}", self.interpreter.metrics.last_run_time);
                        println!("{} instructions executed", self.interpreter.metrics.instructions_executed - start_ins_count);
                        println!("{} total allocations with {} still alive",
                            self.interpreter.metrics.allocations,
                            self.interpreter.get_data_store().get_alloc_rem(),
                        );
                    }
                    let data_ref = dr.get_data();
                    match &*data_ref {
                        Data::None=>{},
                        d=>println!(">> {d:?}"),
                    }
                },
                Ok(None)=>{},
                Err(e)=>error_trace(e, source.as_str(), "<REPL>"),
            }

            let freed = self.interpreter.gc_collect();
            if stats_for_nerds {
                if freed > 0 {
                    println!("{freed} allocations collected this cycle");
                }
            }

            let position = cursor_position().unwrap();
            if position.0 != 0 {
                println!(" ⏎");
            }

            self.rope = Rope::new();
            self.history.push(source);

            // Loop
        }

    }
}


fn print_repl_help() {
    println!(r#"Help:"#);
    println!(r#"    :help               Display this message"#);
    println!(r#"    :exit               Exits the REPL"#);
    println!(r#"    (:include "NAME")   Reads and executes the file, then keeps the functions"#);
}

fn match_repl_directive<'a>(exprs: &'a [Expr<'a>])->Result<Option<ReplDirective<'a>>, ()> {
    if exprs.len() == 0 {return Ok(None)}

    match &exprs[0] {
        Expr::List(items)=>{
            match items.first() {
                Some(Expr::ReplDirective(s))=>match *s {
                    "include"=>{
                        if items.len() != 2 {
                            println!(":include takes 1 argument");
                            return Err(());
                        }
                        match &items[1] {
                            Expr::String(s)=>return Ok(Some(ReplDirective::Include(s))),
                            _=>{
                                println!(":include only accepts strings");
                                return Err(());
                            },
                        }
                    }
                    _=>{
                        println!("Unknown directive: `{s}`");
                        return Err(());
                    },
                },
                _=>return Ok(None),
            }
        },
        Expr::ReplDirective(s)=>match *s {
            "exit"=>return Ok(Some(ReplDirective::Exit)),
            "help"=>{
                return Ok(Some(ReplDirective::Help));
            },
            _=>{
                println!("Unknown directive: `{s}`");
                return Err(());
            },
        },
        _=>return Ok(None),
    }
}

fn include_file(state: &mut ConvertState, name: &str)->Result<InstructionId> {
    let source = read_to_string(name)?;
    let mut parser = new_parser(source.as_str());
    let exprs = parser.parse_all()?;

    return repl_convert(state, exprs);
}
