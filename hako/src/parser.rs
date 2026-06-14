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
        Self {
            input: input.to_string(),
            pos: 0,
            line: 1,
        }
    }

    pub fn parse(&mut self) -> Result<HakoProgram, ParseError> {
        let mut program = HakoProgram {
            boxes: Vec::new(),
            impls: Vec::new(),
        };

        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.input.len() {
                break;
            }
            let word = self.peek_word();
            match word {
                "box" => {
                    program.boxes.push(self.parse_box()?);
                }
                "impl" => {
                    program.impls.push(self.parse_impl()?);
                }
                _ => {
                    return Err(self.error(&format!("expected 'box' or 'impl', got '{}'", word)));
                }
            }
        }

        Ok(program)
    }

    fn parse_box(&mut self) -> Result<HakoBox, ParseError> {
        self.expect_word("box")?;
        let name = self.expect_ident()?;
        self.expect_char('{')?;

        let mut instructions = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.peek_char() == Some('}') {
                self.pos += 1;
                break;
            }
            instructions.push(self.parse_inst_decl()?);
        }

        Ok(HakoBox { name, instructions })
    }

    fn parse_inst_decl(&mut self) -> Result<HakoInstDecl, ParseError> {
        self.expect_word("inst")?;
        let name = self.expect_ident()?;
        self.expect_char('(')?;
        let params = self.parse_params()?;
        self.expect_char(')')?;

        self.skip_whitespace_and_comments();
        let ret_type = if self.peek_word() == "->" {
            self.pos += 2; // skip "->"
            Some(self.parse_type_name()?)
        } else {
            None
        };

        Ok(HakoInstDecl { name, params, ret_type })
    }

    fn parse_impl(&mut self) -> Result<HakoImpl, ParseError> {
        self.expect_word("impl")?;
        let box_name = self.expect_ident()?;
        self.expect_word("::")?;
        let inst_name = self.expect_ident()?;
        self.expect_char('(')?;
        let params = self.parse_params()?;
        self.expect_char(')')?;

        self.skip_whitespace_and_comments();
        let ret_type = if self.peek_word() == "->" {
            self.pos += 2;
            Some(self.parse_type_name()?)
        } else {
            None
        };

        self.expect_char('{')?;
        let body = self.parse_stmt_block('}')?;

        Ok(HakoImpl { box_name, inst_name, params, ret_type, body })
    }

    fn parse_params(&mut self) -> Result<Vec<HakoParam>, ParseError> {
        let mut params = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.peek_char() == Some(')') {
                break;
            }
            if !params.is_empty() {
                self.expect_char(',')?;
                self.skip_whitespace_and_comments();
            }
            if self.peek_char() == Some(')') {
                break; // trailing comma allowed
            }
            let name = self.expect_ident()?;
            self.expect_char(':')?;
            let type_name = self.parse_type_name()?;
            params.push(HakoParam { name, type_name });
        }
        Ok(params)
    }

    fn parse_stmt_block(&mut self, end_char: char) -> Result<Vec<HakoStmt>, ParseError> {
        let mut stmts = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.input.len() || self.peek_char() == Some(end_char) {
                break;
            }
            stmts.push(self.parse_stmt()?);
            if self.peek_char() == Some(',') || self.peek_char() == Some(';') {
                self.pos += 1;
            }
        }
        if self.pos < self.input.len() && self.peek_char() == Some(end_char) {
            self.pos += 1;
        }
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<HakoStmt, ParseError> {
        self.skip_whitespace_and_comments();
        let word = self.peek_word();
        match word {
            "let" => self.parse_let(),
            "caso" => self.parse_caso(),
            "retorna" => self.parse_retorna(),
            "asm" => self.parse_asm(),
            "loop" => self.parse_loop(),
            "for" => self.parse_for(),
            "ref" => self.parse_ref_stmt(),
            "{" => {
                self.pos += 1;
                let body = self.parse_stmt_block('}')?;
                Ok(HakoStmt::Block(body))
            }
            "}" => {
                Ok(HakoStmt::Empty)
            }
            _ => {
                let expr = self.parse_expr_until_semicolon()?;
                Ok(HakoStmt::Expr(expr))
            }
        }
    }

    fn parse_let(&mut self) -> Result<HakoStmt, ParseError> {
        self.expect_word("let")?;
        self.skip_whitespace_and_comments();
        let is_mut = self.peek_word() == "mut";
        if is_mut { self.pos += 3; }
        let name = self.expect_ident()?;
        let type_name = if self.peek_char() == Some(':') {
            self.pos += 1;
            Some(self.parse_type_name()?)
        } else {
            None
        };
        self.skip_whitespace_and_comments();
        let value = if self.peek_char() == Some('=') {
            self.pos += 1;
            Some(self.parse_expr_until_semicolon()?)
        } else {
            None
        };
        Ok(HakoStmt::Let { name, type_name, is_mut, value })
    }

    fn parse_caso(&mut self) -> Result<HakoStmt, ParseError> {
        self.expect_word("caso")?;
        self.skip_whitespace_and_comments();
        // Check for "else" (caso contrário)
        if self.peek_word() == "else" {
            self.pos += 4;
            self.expect_word("=>")?;
            let body = self.parse_caso_body()?;
            return Ok(HakoStmt::CasoContrario(body));
        }
        let cond = self.parse_expr_until_arrow()?;
        self.expect_word("=>")?;
        let body = self.parse_caso_body()?;
        Ok(HakoStmt::Caso { cond, body })
    }

    fn parse_caso_body(&mut self) -> Result<Vec<HakoStmt>, ParseError> {
        self.skip_whitespace_and_comments();
        if self.peek_char() == Some('{') {
            self.pos += 1;
            let body = self.parse_stmt_block('}')?;
            Ok(body)
        } else if self.peek_word() == "caso" {
            // nested caso? or inline statement
            let stmt = self.parse_stmt()?;
            Ok(vec![stmt])
        } else {
            let expr = self.parse_expr_until_semicolon()?;
            Ok(vec![HakoStmt::Expr(expr)])
        }
    }

    fn parse_retorna(&mut self) -> Result<HakoStmt, ParseError> {
        self.expect_word("retorna")?;
        self.skip_whitespace_and_comments();
        if self.peek_char() == Some(';') || self.peek_char() == Some(',') || self.peek_char() == Some('}') {
            Ok(HakoStmt::Retorna(None))
        } else {
            let expr = self.parse_expr_until_semicolon()?;
            Ok(HakoStmt::Retorna(Some(expr)))
        }
    }

    fn parse_asm(&mut self) -> Result<HakoStmt, ParseError> {
        self.expect_word("asm")?;
        self.expect_char('!')?;
        self.expect_char('(')?;

        let template = self.parse_string_literal()?;

        let mut operands: Vec<String> = Vec::new();
        let mut clobbers: Vec<String> = Vec::new();

        loop {
            self.skip_whitespace_and_comments();
            if self.peek_char() == Some(')') { break; }
            self.expect_char(',')?;
            self.skip_whitespace_and_comments();
            if self.peek_char() == Some(')') { break; }

            // Read one full operand: constraint (expr) or clobber "string"
            let start = self.pos;
            let mut depth = 0;
            let mut in_string = false;
            loop {
                if self.pos >= self.input.len() { break; }
                let c = self.input.as_bytes()[self.pos] as char;
                if c == '"' { in_string = !in_string; }
                else if c == '(' && !in_string { depth += 1; }
                else if c == ')' && !in_string {
                    if depth == 0 { break; }
                    depth -= 1;
                }
                else if c == ',' && depth == 0 && !in_string { break; }
                self.pos += 1;
            }
            let operand = self.input[start..self.pos].trim().to_string();
            if operand.starts_with("options(") || operand.starts_with("clobber_abi") {
                clobbers.push(operand);
            } else {
                operands.push(operand);
            }
        }

        self.expect_char(')')?;
        Ok(HakoStmt::Asm { template, operands, clobbers })
    }

    fn parse_loop(&mut self) -> Result<HakoStmt, ParseError> {
        self.expect_word("loop")?;
        self.expect_char('{')?;
        let body = self.parse_stmt_block('}')?;
        Ok(HakoStmt::Loop(body))
    }

    fn parse_for(&mut self) -> Result<HakoStmt, ParseError> {
        self.expect_word("for")?;
        let var = self.expect_ident()?;
        self.expect_word("in")?;
        let iter = self.parse_expr_until_brace()?;
        self.expect_char('{')?;
        let body = self.parse_stmt_block('}')?;
        Ok(HakoStmt::For { var, iter, body })
    }

    fn parse_ref_stmt(&mut self) -> Result<HakoStmt, ParseError> {
        self.expect_word("ref")?;
        let path = self.parse_path()?;
        Ok(HakoStmt::Ref(path))
    }

    fn parse_expr_until_semicolon(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        let mut depth_paren = 0;
        let mut depth_brace = 0;
        loop {
            if self.pos >= self.input.len() {
                break;
            }
            let c = self.input.as_bytes()[self.pos] as char;
            if c == '(' { depth_paren += 1; }
            else if c == ')' { depth_paren -= 1; }
            else if c == '{' { depth_brace += 1; }
            else if c == '}' { depth_brace -= 1; }
            else if c == ';' && depth_paren == 0 && depth_brace == 0 {
                break;
            }
            else if c == ',' && depth_paren == 0 && depth_brace == 0 {
                break;
            }
            else if c == '\n' && depth_paren == 0 && depth_brace == 0 {
                // Check if the next non-space is the end
                break;
            }
            self.pos += 1;
        }
        let expr = self.input[start..self.pos].trim().to_string();
        Ok(expr)
    }

    fn parse_expr_until_arrow(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        let mut depth_paren = 0;
        loop {
            if self.pos >= self.input.len() {
                break;
            }
            let c = self.input.as_bytes()[self.pos] as char;
            if c == '(' { depth_paren += 1; }
            else if c == ')' { depth_paren -= 1; }
            else if c == '=' && depth_paren == 0 {
                if self.pos + 1 < self.input.len() && self.input.as_bytes()[self.pos + 1] as char == '>' {
                    break;
                }
            }
            self.pos += 1;
        }
        let expr = self.input[start..self.pos].trim().to_string();
        Ok(expr)
    }

    fn parse_expr_until_brace(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        loop {
            if self.pos >= self.input.len() {
                break;
            }
            let c = self.input.as_bytes()[self.pos] as char;
            if c == '{' {
                break;
            }
            self.pos += 1;
        }
        let expr = self.input[start..self.pos].trim().to_string();
        Ok(expr)
    }

    fn parse_type_name(&mut self) -> Result<String, ParseError> {
        self.skip_whitespace_and_comments();
        // Handle ! never type
        if self.peek_char() == Some('!') {
            self.pos += 1;
            return Ok("!".to_string());
        }
        // Handle & reference
        if self.peek_char() == Some('&') {
            self.pos += 1;
            let mut t = String::from("&");
            // Check for mut
            self.skip_whitespace_and_comments();
            if self.peek_word() == "mut" {
                self.pos += 3;
                t.push_str("mut ");
            }
            t.push_str(&self.parse_type_name()?);
            return Ok(t);
        }
        // Handle * pointer
        if self.peek_char() == Some('*') {
            self.pos += 1;
            let mut t = String::from("*");
            self.skip_whitespace_and_comments();
            if self.peek_word() == "const" {
                self.pos += 5;
                t.push_str("const ");
            } else if self.peek_word() == "mut" {
                self.pos += 3;
                t.push_str("mut ");
            }
            t.push_str(&self.parse_type_name()?);
            return Ok(t);
        }
        self.expect_ident()
    }

    fn parse_string_literal(&mut self) -> Result<String, ParseError> {
        self.skip_whitespace_and_comments();
        if self.peek_char() != Some('"') {
            return Err(self.error("expected string literal"));
        }
        self.pos += 1;
        let start = self.pos;
        loop {
            if self.pos >= self.input.len() {
                return Err(self.error("unterminated string literal"));
            }
            let c = self.input.as_bytes()[self.pos] as char;
            if c == '"' {
                let s = self.input[start..self.pos].to_string();
                self.pos += 1;
                return Ok(s);
            }
            if c == '\\' {
                self.pos += 1; // skip escaped char
            }
            self.pos += 1;
        }
    }

    fn parse_path(&mut self) -> Result<String, ParseError> {
        let mut path = self.expect_ident()?;
        loop {
            self.skip_whitespace_and_comments();
            if self.peek_word() == "::" {
                self.pos += 2;
                path.push_str("::");
                path.push_str(&self.expect_ident()?);
            } else {
                break;
            }
        }
        Ok(path)
    }

    // == Helpers ==

    fn peek_char(&self) -> Option<char> {
        self.input.as_bytes().get(self.pos).map(|&b| b as char)
    }

    fn peek_word(&self) -> &str {
        let start = self.pos;
        let rest = &self.input[start..];

        // Check multi-char operators first
        if rest.starts_with("::") { return "::"; }
        if rest.starts_with("->") { return "->"; }
        if rest.starts_with("=>") { return "=>"; }

        // Scan alphanumeric/underscore identifier
        let mut end = start;
        while end < self.input.len() {
            let c = self.input.as_bytes()[end] as char;
            if c.is_alphanumeric() || c == '_' {
                end += 1;
            } else {
                break;
            }
        }
        if end > start {
            &self.input[start..end]
        } else {
            ""
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        self.skip_whitespace_and_comments();
        let start = self.pos;
        while self.pos < self.input.len() {
            let c = self.input.as_bytes()[self.pos] as char;
            if c.is_alphanumeric() || c == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(self.error("expected identifier"));
        }
        Ok(self.input[start..self.pos].to_string())
    }

    fn expect_word(&mut self, word: &str) -> Result<(), ParseError> {
        self.skip_whitespace_and_comments();
        let w = self.peek_word();
        if w == word {
            // peek_word doesn't modify self.pos, so we advance by the matched word length
            self.pos += w.len();
            Ok(())
        } else {
            Err(self.error(&format!("expected '{}', got '{}'", word, w)))
        }
    }

    fn expect_char(&mut self, c: char) -> Result<(), ParseError> {
        self.skip_whitespace_and_comments();
        if self.peek_char() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected '{}', got '{:?}'", c, self.peek_char())))
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            if self.pos >= self.input.len() { break; }
            let c = self.input.as_bytes()[self.pos] as char;
            if c == '\n' { self.line += 1; self.pos += 1; }
            else if c.is_whitespace() { self.pos += 1; }
            else if c == '/' && self.pos + 1 < self.input.len() {
                let n = self.input.as_bytes()[self.pos + 1] as char;
                if n == '/' {
                    // line comment
                    self.pos += 2;
                    while self.pos < self.input.len() && self.input.as_bytes()[self.pos] as char != '\n' {
                        self.pos += 1;
                    }
                } else if n == '*' {
                    // block comment
                    self.pos += 2;
                    while self.pos + 1 < self.input.len() {
                        if self.input.as_bytes()[self.pos] as char == '\n' { self.line += 1; }
                        if self.input.as_bytes()[self.pos] as char == '*' && self.input.as_bytes()[self.pos + 1] as char == '/' {
                            self.pos += 2;
                            break;
                        }
                        self.pos += 1;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    fn error(&self, msg: &str) -> ParseError {
        ParseError {
            msg: msg.to_string(),
            line: self.line,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_program() {
        let mut p = Parser::new("");
        let prog = p.parse().unwrap();
        assert!(prog.boxes.is_empty());
        assert!(prog.impls.is_empty());
    }

    #[test]
    fn test_box_with_inst() {
        let input = r#"
box vga {
  inst put_char(c: u8, x: u16, y: u16)
  inst clear() -> !
}
"#;
        let mut p = Parser::new(input);
        let prog = p.parse().unwrap();
        assert_eq!(prog.boxes.len(), 1);
        assert_eq!(prog.boxes[0].name, "vga");
        assert_eq!(prog.boxes[0].instructions.len(), 2);
        assert_eq!(prog.boxes[0].instructions[0].name, "put_char");
        assert_eq!(prog.boxes[0].instructions[0].params.len(), 3);
    }

    #[test]
    fn test_simple_impl() {
        let input = r#"
impl vga::put_char(c: u8, x: u16, y: u16) {
  let pos = x + y * 80;
  asm!("mov byte [0xB8000], al");
  retorna;
}
"#;
        let mut p = Parser::new(input);
        let prog = p.parse().unwrap();
        assert_eq!(prog.impls.len(), 1);
        assert_eq!(prog.impls[0].box_name, "vga");
        assert_eq!(prog.impls[0].inst_name, "put_char");
        assert_eq!(prog.impls[0].body.len(), 4);
    }
}
