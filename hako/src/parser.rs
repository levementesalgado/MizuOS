use crate::ast::*;

#[derive(Debug)]
pub struct ParseError {
    pub msg: String,
    pub line: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

impl std::error::Error for ParseError {}

pub struct Parser {
    input: String,
    pos: usize,
    line: usize,
}

impl Parser {
    pub fn new(input: &str) -> Self {
        Self { input: input.to_string(), pos: 0, line: 1 }
    }

    pub fn parse(&mut self) -> Result<Program, ParseError> {
        let mut program = Program { boxes: Vec::new(), flows: Vec::new() };
        loop {
            self.skip();
            if self.pos >= self.input.len() { break; }
            match self.peek() {
                "box" => program.boxes.push(self.parse_box()?),
                "flow" => program.flows.push(self.parse_flow()?),
                w => return Err(self.err(&format!("expected 'box' or 'flow', got '{}'", w))),
            }
        }
        Ok(program)
    }

    // --- Box ---

    fn parse_box(&mut self) -> Result<Box, ParseError> {
        self.expect_word("box")?;
        let name = self.expect_id()?;
        let extends = if self.peek() == "::" {
            self.pos += 2;
            Some(self.expect_id()?)
        } else {
            None
        };
        self.expect('{')?;
        let mut items = Vec::new();
        loop {
            self.skip();
            if self.peek_char() == Some('}') { self.pos += 1; break; }
            if self.pos >= self.input.len() { return Err(self.err("unterminated box")); }
            items.push(self.parse_item()?);
        }
        Ok(Box { name, extends, items })
    }

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        let first = self.expect_id()?;
        self.skip();

        // Check for => (auto-mode function)
        if self.peek() == "=>" {
            self.pos += 2;
            self.skip();
            self.expect_id()?; // mode name
            return Ok(Item::Fn { name: first, params: vec![], mode: FnMode::Auto, body: vec![] });
        }

        if self.peek_char() == Some('(') {
            self.parse_fn(first)
        } else if self.peek_char() == Some('{') {
            // Function with no params
            self.pos += 1;
            let body = self.block()?;
            Ok(Item::Fn { name: first, params: vec![], mode: FnMode::Block, body })
        } else if self.peek() == "raw" {
            self.pos += 3;
            self.skip();
            self.expect('{')?;
            let body = self.block()?;
            Ok(Item::Fn { name: first, params: vec![], mode: FnMode::Raw, body })
        } else if self.peek_char() == Some('=') {
            self.pos += 1;
            self.skip();
            let value = self.expr_eol()?;
            Ok(Item::Var { name: first, value: Some(value) })
        } else {
            let rest = self.expr_eol()?;
            let expr = if rest.is_empty() { first } else { format!("{}{}", first, rest) };
            Ok(Item::Expr(expr))
        }
    }

    fn parse_fn(&mut self, name: String) -> Result<Item, ParseError> {
        self.expect('(')?;
        let params = self.params()?;
        self.expect(')')?;
        self.skip();

        if self.peek() == "raw" {
            self.pos += 3;
            self.skip();
            self.expect('{')?;
            let body = self.block()?;
            Ok(Item::Fn { name, params, mode: FnMode::Raw, body })
        } else if self.peek() == "=>" {
            self.pos += 2;
            self.skip();
            self.expect_id()?; // "default" or whatever
            Ok(Item::Fn { name, params, mode: FnMode::Auto, body: vec![] })
        } else {
            self.expect('{')?;
            let body = self.block()?;
            Ok(Item::Fn { name, params, mode: FnMode::Block, body })
        }
    }

    // --- Flow ---

    fn parse_flow(&mut self) -> Result<Flow, ParseError> {
        self.expect_word("flow")?;
        let name = self.expect_id()?;
        self.expect('{')?;
        let mut steps = Vec::new();
        loop {
            self.skip();
            if self.peek_char() == Some('}') { self.pos += 1; break; }
            let step = self.expect_id()?;
            steps.push(step);
        // skip optional arrow → or ->
        self.skip();
        let ch = self.peek_char();
        if ch == Some('-') || ch == Some('→') {
            // advance past the arrow character(s)
            if let Some(c) = ch {
                self.pos += c.len_utf8(); // skip 1 byte for '-', 3 bytes for '→'
                if c == '-' && self.peek_char() == Some('>') {
                    self.pos += 1; // skip '>'
                }
            }
        }
        }
        Ok(Flow { name, steps })
    }

    // --- Statements ---

    fn block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        loop {
            self.skip();
            if self.pos >= self.input.len() || self.peek_char() == Some('}') {
                if self.peek_char() == Some('}') { self.pos += 1; }
                break;
            }
            stmts.push(self.stmt()?);
        }
        Ok(stmts)
    }

    fn stmt(&mut self) -> Result<Stmt, ParseError> {
        self.skip();
        // Check for string literal (assembly line in raw block)
        if self.peek_char() == Some('"') {
            self.pos += 1;
            let start = self.pos;
            loop {
                if self.pos >= self.input.len() { break; }
                let c = self.input.as_bytes()[self.pos] as char;
                if c == '"' { break; }
                if c == '\n' { self.line += 1; }
                self.pos += 1;
            }
            let content = self.input[start..self.pos].to_string();
            if self.peek_char() == Some('"') { self.pos += 1; }
            return Ok(Stmt::AsmLine(content));
        }
        let w = self.peek();
        match w {
            "if" => self.parse_if(),
            "loop" => self.parse_loop(),
            "for" => self.parse_for(),
            "break" => { self.pos += 5; Ok(Stmt::Break) }
            "{" => { self.pos += 1; let body = self.block()?; Ok(Stmt::Block(body)) }
            "}" => { self.pos += 1; Ok(Stmt::Expr(String::new())) }
            "in" | "out" => {
                let keyword = w.to_string();
                self.pos += keyword.len();
                self.skip();
                if self.peek_char() == Some('(') {
                    self.expect('(')?;
                    let reg = self.expect_id()?;
                    self.expect(',')?;
                    self.skip();
                    let var = self.expect_id()?;
                    self.expect(')')?;
                    if keyword == "out" {
                        Ok(Stmt::AsmOut { reg, var })
                    } else {
                        Ok(Stmt::AsmIn { reg, var })
                    }
                } else {
                    let rest = self.expr_eol()?;
                    let expr = if rest.is_empty() { keyword } else { format!("{}{}", keyword, rest) };
                    Ok(Stmt::Expr(expr))
                }
            }
            _ => self.parse_assign_or_expr(),
        }
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        self.expect_word("if")?;
        let cond = self.expr_arrow()?;
        self.skip();
        // Skip optional "=>" after condition
        if self.peek() == "=>" { self.pos += 2; self.skip(); }
        let then = if self.peek_char() == Some('{') {
            self.pos += 1;
            self.block()?
        } else {
            vec![self.stmt()?]
        };
        self.skip();
        let r#else = if self.peek() == "else" {
            self.pos += 4;
            self.skip();
            if self.peek_char() == Some('{') {
                self.pos += 1;
                self.block()?
            } else {
                vec![self.stmt()?]
            }
        } else {
            vec![]
        };
        Ok(Stmt::If { cond, then, r#else })
    }

    fn parse_loop(&mut self) -> Result<Stmt, ParseError> {
        self.expect_word("loop")?;
        self.expect('{')?;
        let body = self.block()?;
        Ok(Stmt::Loop(body))
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        self.expect_word("for")?;
        let var = self.expect_id()?;
        self.expect_word("in")?;
        let iter = self.expr_brace()?;
        self.expect('{')?;
        let body = self.block()?;
        Ok(Stmt::For { var, iter, body })
    }

    fn parse_assign_or_expr(&mut self) -> Result<Stmt, ParseError> {
        let first = self.expect_id()?;
        self.skip();
        // Read the full left-hand side including array indexing etc.
        let mut lvalue = first;
        loop {
            match self.peek_char() {
                Some('[') | Some('(') => {
                    let ch = self.peek_char().unwrap();
                    self.pos += 1;
                    let mut depth = 1i32;
                    let start = self.pos;
                    loop {
                        if self.pos >= self.input.len() { break; }
                        let c = self.input.as_bytes()[self.pos] as char;
                        if c == '(' || c == '[' || c == '{' { depth += 1; }
                        else if c == ')' || c == ']' || c == '}' { depth -= 1; if depth <= 0 { break; } }
                        self.pos += 1;
                    }
                    let inner = self.input[start..self.pos].to_string();
                    self.pos += 1; // skip closing bracket
                    lvalue = if ch == '[' {
                        format!("{}[{}]", lvalue, inner)
                    } else {
                        format!("{}({})", lvalue, inner)
                    };
                    // skip spaces/tabs only, not newlines
                    loop {
                        let c = self.peek_char();
                        if c == Some(' ') || c == Some('\t') { self.pos += 1; }
                        else { break; }
                    }
                }
                _ => break,
            }
        }
        if self.peek_char() == Some('=') {
            self.pos += 1;
            self.skip();
            let value = self.expr_eol()?;
            Ok(Stmt::Assign { var: lvalue, value })
        } else {
            // Check for remaining expression on the same line (before newline)
            let saved = self.pos;
            let mut has_continuation = false;
            loop {
                let c = self.peek_char();
                if c == None || c == Some('\n') || c == Some(';') || c == Some('}') { break; }
                if c != Some(' ') && c != Some('\t') { has_continuation = true; break; }
                self.pos += 1;
            }
            self.pos = saved;
            if has_continuation {
                let rest = self.expr_eol()?;
                Ok(Stmt::Expr(format!("{}{}", lvalue, rest)))
            } else {
                Ok(Stmt::Expr(lvalue))
            }
        }
    }

    // --- Expression helpers ---

    fn expr_eol(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        let mut depth = 0i32;
        let mut in_str = false;
        loop {
            if self.pos >= self.input.len() { break; }
            let c = self.input.as_bytes()[self.pos] as char;
            if c == '"' { in_str = !in_str; }
            if !in_str {
                if c == '(' || c == '[' || c == '{' { depth += 1; }
                else if c == ')' || c == ']' || c == '}' {
                    if depth <= 0 {
                        break; // unmatched bracket — not part of this expression
                    }
                    depth -= 1;
                }
                else if (c == ';' || c == '\n') && depth == 0 { break; }
                else if c == '/' && depth == 0 {
                    if self.pos + 1 < self.input.len()
                        && self.input.as_bytes()[self.pos + 1] as char == '/' {
                        break; // stop at inline // comment
                    }
                }
                else if c == '=' && depth == 0 {
                    // Check it's not ==
                    if self.pos + 1 < self.input.len() && self.input.as_bytes()[self.pos + 1] as char == '=' {
                        // == is fine, continue
                    } else if self.pos > start {
                        // = at depth 0, outside fn call — break
                        break;
                    }
                }
            }
            self.pos += 1;
        }
        Ok(self.input[start..self.pos].trim().to_string())
    }

    fn expr_arrow(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        let mut depth = 0i32;
        loop {
            if self.pos >= self.input.len() { break; }
            let c = self.input.as_bytes()[self.pos] as char;
            if c == '(' { depth += 1; }
            else if c == ')' { depth -= 1; }
            else if c == '/' && depth == 0 {
                if self.pos + 1 < self.input.len()
                    && self.input.as_bytes()[self.pos + 1] as char == '/' {
                    break;
                }
            }
            else if c == '=' && depth == 0 {
                if self.pos + 1 < self.input.len()
                    && self.input.as_bytes()[self.pos + 1] as char == '>' {
                    break;
                }
            }
            self.pos += 1;
        }
        Ok(self.input[start..self.pos].trim().to_string())
    }

    fn expr_brace(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        loop {
            if self.pos >= self.input.len() { break; }
            let c = self.input.as_bytes()[self.pos] as char;
            if c == '{' { break; }
            if c == '/' && self.pos + 1 < self.input.len()
                && self.input.as_bytes()[self.pos + 1] as char == '/' {
                break;
            }
            self.pos += 1;
        }
        Ok(self.input[start..self.pos].trim().to_string())
    }

    // --- Parameter list ---

    fn params(&mut self) -> Result<Vec<(String, String)>, ParseError> {
        let mut v = Vec::new();
        loop {
            self.skip();
            if self.peek_char() == Some(')') { break; }
            if !v.is_empty() { self.expect(',')?; self.skip(); }
            if self.peek_char() == Some(')') { break; }
            let name = self.expect_id()?;
            self.skip();
            let typ = if self.peek_char() == Some(':') {
                self.pos += 1;
                self.skip();
                self.parse_type()?
            } else {
                self.infer_type(&name)
            };
            v.push((name, typ));
        }
        Ok(v)
    }

    fn parse_type(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        // Handle &str, &[u32], &[u8], etc.
        if self.peek_char() == Some('&') {
            self.pos += 1;
            if self.peek_char() == Some('[') {
                self.pos += 1;
                let inner = self.expect_id()?;
                self.expect(']')?;
                Ok(format!("&[{}]", inner))
            } else {
                self.expect_id()?;
                Ok(self.input[start..self.pos].to_string())
            }
        } else {
            let id = self.expect_id()?;
            if self.peek_char() == Some('[') {
                self.pos += 1;
                let inner = self.expect_id()?;
                self.expect(']')?;
                Ok(format!("{}[{}]", id, inner))
            } else {
                Ok(id)
            }
        }
    }

    fn infer_type(&self, name: &str) -> String {
        match name {
            "s" | "msg" | "str" => "&str".into(),
            "data" | "buf" | "buffer" | "arr" => "&[u32]".into(),
            _ => "u32".into(),
        }
    }

    // --- Lexer helpers ---

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn peek(&self) -> &str {
        let rest = &self.input[self.pos..];
        if rest.starts_with("::") { return "::"; }
        if rest.starts_with("=>") { return "=>"; }
        if rest.starts_with("->") { return "->"; }
        let ch = rest.chars().next();
        match ch {
            Some(c) if c.is_alphanumeric() || c == '_' => {
                let mut end = 0;
                for (i, c) in rest.char_indices() {
                    if c.is_alphanumeric() || c == '_' { end = i + c.len_utf8(); } else { break; }
                }
                &rest[..end]
            }
            _ => "",
        }
    }

    fn expect_id(&mut self) -> Result<String, ParseError> {
        self.skip();
        let start = self.pos;
        let rest = &self.input[self.pos..];
        let mut end = 0;
        for (i, c) in rest.char_indices() {
            if c.is_alphanumeric() || c == '_' {
                end = i + c.len_utf8();
            } else {
                break;
            }
        }
        if end == 0 { return Err(self.err("expected identifier")); }
        self.pos += end;
        Ok(self.input[start..self.pos].to_string())
    }

    fn expect_word(&mut self, word: &str) -> Result<(), ParseError> {
        self.skip();
        let p = self.peek();
        if p == word { self.pos += word.len(); Ok(()) }
        else { Err(self.err(&format!("expected '{}', got '{}'", word, p))) }
    }

    fn expect(&mut self, c: char) -> Result<(), ParseError> {
        self.skip();
        if self.peek_char() == Some(c) { self.pos += 1; Ok(()) }
        else { Err(self.err(&format!("expected '{}'", c))) }
    }

    fn skip(&mut self) {
        loop {
            if self.pos >= self.input.len() { break; }
            let rest = &self.input[self.pos..];
            let c = match rest.chars().next() {
                Some(ch) => ch,
                None => break,
            };
            if c == '\n' { self.line += 1; self.pos += 1; }
            else if c == '\r' { self.pos += 1; }
            else if c == ' ' || c == '\t' { self.pos += 1; }
            else if c == '/' && rest.len() >= 2 {
                let next = rest.as_bytes()[1] as char;
                if next == '/' {
                    self.pos += 2;
                    while self.pos < self.input.len() && self.input.as_bytes()[self.pos] as char != '\n' { self.pos += 1; }
                } else if next == '*' {
                    self.pos += 2;
                    while self.pos + 1 < self.input.len() {
                        if self.input.as_bytes()[self.pos] as char == '\n' { self.line += 1; }
                        if self.input.as_bytes()[self.pos] as char == '*' && self.input.as_bytes()[self.pos + 1] as char == '/' { self.pos += 2; break; }
                        self.pos += 1;
                    }
                } else { break; }
            } else { break; }
        }
    }

    fn err(&self, msg: &str) -> ParseError {
        ParseError { msg: msg.to_string(), line: self.line }
    }
}
