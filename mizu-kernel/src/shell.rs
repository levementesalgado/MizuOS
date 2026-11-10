use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use mizu_core::{tokenize, parse_pipeline, Pipeline, SimpleCommand, ParseError};

use crate::keyboard;
use crate::serial_driver;
use crate::vga_driver;
use crate::fs;

const LINE_MAX: usize = 256;
const HIST_SIZE: usize = 16;

static COMMANDS: &[&str] = &[
    "help", "echo", "clear", "uname", "whoami", "date", "yes",
    "ls", "cat", "head", "wc", "hexdump", "true", "false",
    "exit", "quit", "sh", "cp", "rm", "edit", "touch",
];

fn read_byte() -> u8 {
    loop {
        if let Some(c) = keyboard::get_key() {
            return c;
        }
        if let Some(c) = serial_driver::get_char() {
            return c;
        }
    }
}

struct LineEditor {
    buf: String,
    cursor: usize,
    history: VecDeque<String>,
    hist_idx: isize,
    saved_search: String,
}

impl LineEditor {
    fn new() -> Self {
        LineEditor {
            buf: String::new(),
            cursor: 0,
            history: VecDeque::new(),
            hist_idx: -1,
            saved_search: String::new(),
        }
    }

    fn insert_char(&mut self, c: u8) {
        if self.buf.len() >= LINE_MAX - 1 { return; }
        self.buf.insert(self.cursor, c as char);
        vga_driver::put_char(c);
        self.cursor += 1;
        self.redraw_line();
    }

    fn delete_before(&mut self) {
        if self.cursor == 0 || self.buf.is_empty() { return; }
        self.cursor -= 1;
        self.buf.remove(self.cursor);
        let slen = self.buf.len() - self.cursor;
        for &b in &self.buf.as_bytes()[self.cursor..] {
            vga_driver::put_char(b);
        }
        vga_driver::put_char(b' ');
        for _ in 0..slen + 1 { vga_driver::put_char(0x08); }
    }

    fn delete_after(&mut self) {
        if self.cursor >= self.buf.len() { return; }
        self.buf.remove(self.cursor);
        self.redraw_line();
    }

    fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            vga_driver::move_cursor_left();
        }
    }

    fn cursor_right(&mut self) {
        if self.cursor < self.buf.len() {
            let c = self.buf.as_bytes()[self.cursor];
            vga_driver::put_char(c);
            self.cursor += 1;
        }
    }

    fn cursor_home(&mut self) {
        while self.cursor > 0 {
            self.cursor_left();
        }
    }

    fn cursor_end(&mut self) {
        while self.cursor < self.buf.len() {
            self.cursor_right();
        }
    }

    fn clear_line(&mut self) {
        let len = self.buf.len();
        self.cursor_home();
        for _ in 0..len {
            vga_driver::put_char(b' ');
        }
        for _ in 0..len {
            vga_driver::put_char(0x08);
        }
        self.buf.clear();
        self.cursor = 0;
    }

    fn redraw_line(&self) {
        let tlen = self.buf.len() - self.cursor;
        for &b in self.buf.as_bytes()[self.cursor..].iter() {
            vga_driver::put_char(b);
        }
        if !self.buf.is_empty() && self.cursor < self.buf.len() {
            for _ in 0..tlen {
                vga_driver::put_char(0x08);
            }
        }
    }

    fn history_up(&mut self) {
        if self.history.is_empty() { return; }
        if self.hist_idx == -1 {
            self.saved_search = self.buf.clone();
        }
        let next = self.hist_idx + 1;
        if next as usize >= self.history.len() { return; }
        self.hist_idx = next;
        self.load_history();
    }

    fn history_down(&mut self) {
        if self.hist_idx == -1 { return; }
        if self.hist_idx == 0 {
            self.hist_idx = -1;
            self.load_saved();
            return;
        }
        self.hist_idx -= 1;
        self.load_history();
    }

    fn load_history(&mut self) {
        let len = self.buf.len();
        self.cursor_home();
        for _ in 0..len {
            vga_driver::put_char(b' ');
        }
        for _ in 0..len {
            vga_driver::put_char(0x08);
        }
        let idx = self.hist_idx as usize;
        self.buf = self.history[self.history.len() - 1 - idx].clone();
        self.cursor = self.buf.len();
        vga_driver::write_str(&self.buf);
    }

    fn load_saved(&mut self) {
        let len = self.buf.len();
        self.cursor_home();
        for _ in 0..len {
            vga_driver::put_char(b' ');
        }
        for _ in 0..len {
            vga_driver::put_char(0x08);
        }
        self.buf = core::mem::take(&mut self.saved_search);
        self.cursor = self.buf.len();
        vga_driver::write_str(&self.buf);
    }

    fn push_history(&mut self) {
        let s = self.buf.trim().to_string();
        if s.is_empty() { return; }
        if self.history.back().map(|x| x == &s).unwrap_or(false) { return; }
        self.history.push_back(s);
        if self.history.len() > HIST_SIZE {
            self.history.pop_front();
        }
        self.hist_idx = -1;
        self.saved_search.clear();
    }

    fn tab_complete(&mut self) {
        if self.buf.is_empty() { return; }

        // Find current word
        let mut start = self.cursor;
        while start > 0 {
            let c = self.buf.as_bytes()[start - 1];
            if c == b' ' || c == b'\t' { break; }
            start -= 1;
        }
        let prefix = &self.buf[start..self.cursor];

        // Collect matches
        let mut matches: Vec<&str> = COMMANDS.iter().filter(|c| c.starts_with(prefix)).map(|c| *c).collect();

        // Add filenames
        let files = fs::file_list();
        for f in &files {
            if f.starts_with(prefix) {
                matches.push(f.as_str());
            }
        }

        matches.sort();
        matches.dedup();

        if matches.is_empty() { return; }
        if matches.len() == 1 {
            let rest = &matches[0][prefix.len()..];
            for &b in rest.as_bytes().iter() {
                self.buf.insert(self.cursor, b as char);
                self.cursor += 1;
                vga_driver::put_char(b);
            }
            // Add space after completion
            self.buf.insert(self.cursor, ' ');
            self.cursor += 1;
            vga_driver::put_char(b' ');
            return;
        }

        // Multiple matches: find common prefix
        let common = longest_common_prefix(&matches);
        if common.len() > prefix.len() {
            let rest = &common[prefix.len()..];
            for &b in rest.as_bytes().iter() {
                self.buf.insert(self.cursor, b as char);
                self.cursor += 1;
                vga_driver::put_char(b);
            }
            return;
        }

        // Show all matches
        vga_driver::put_char(b'\n');
        for m in &matches {
            vga_driver::write_str("  ");
            vga_driver::write_str(m);
        }
        vga_driver::put_char(b'\n');
        prompt();
        vga_driver::write_str(&self.buf);
        // Fix cursor: move back to current position
        let remaining = self.buf.len() - self.cursor;
        for _ in 0..remaining {
            vga_driver::put_char(0x08);
        }
    }

    fn handle_escape(&mut self) -> bool {
        let bracket = read_byte();
        if bracket != b'[' { return false; }
        let cmd = read_byte();
        match cmd {
            b'A' => { self.history_up(); true }
            b'B' => { self.history_down(); true }
            b'C' => { self.cursor_right(); true }
            b'D' => { self.cursor_left(); true }
            b'H' => { self.cursor_home(); true }
            b'F' => { self.cursor_end(); true }
            b'3' => {
                // Could be DEL (^[[3~) or other
                let tilde = read_byte();
                if tilde == b'~' { self.delete_after(); }
                true
            }
            b'1' => {
                let tilde = read_byte();
                if tilde == b'~' { self.cursor_home(); }
                true
            }
            b'4' => {
                let tilde = read_byte();
                if tilde == b'~' { self.cursor_end(); }
                true
            }
            _ => true
        }
    }

    fn get_line(&mut self) -> Option<String> {
        prompt();
        self.buf.clear();
        self.cursor = 0;

        loop {
            let c = read_byte();
            match c {
                b'\n' | b'\r' => {
                    vga_driver::put_char(b'\n');
                    let line = self.buf.trim().to_string();
                    self.push_history();
                    if line.is_empty() { return None; }
                    if line == "exit" || line == "quit" { return None; }
                    return Some(line);
                }
                0x03 => { // Ctrl+C
                    self.clear_line();
                    vga_driver::write_str("^C\n");
                    return None;
                }
                0x04 => { // Ctrl+D
                    if self.buf.is_empty() {
                        vga_driver::put_char(b'\n');
                        return None;
                    }
                }
                0x08 | 0x7F => self.delete_before(),
                0x09 => self.tab_complete(),
                0x0C => { // Ctrl+L
                    vga_driver::clear();
                    prompt();
                    vga_driver::write_str(&self.buf);
                    let remaining = self.buf.len() - self.cursor;
                    for _ in 0..remaining {
                        vga_driver::put_char(0x08);
                    }
                }
                0x0E => { // Ctrl+N (down)
                    self.history_down();
                }
                0x10 => { // Ctrl+P (up)
                    self.history_up();
                }
                0x15 => { // Ctrl+U
                    self.clear_line();
                }
                 keyboard::KEY_LEFT => self.cursor_left(),
                 keyboard::KEY_RIGHT => self.cursor_right(),
                 keyboard::KEY_UP => self.history_up(),
                 keyboard::KEY_DOWN => self.history_down(),
                 keyboard::KEY_HOME => self.cursor_home(),
                 keyboard::KEY_END => self.cursor_end(),
                 keyboard::KEY_DEL => self.delete_after(),
                 0x1B => { // Escape sequence (serial)
                    self.handle_escape();
                }
                c if self.buf.len() < LINE_MAX - 1 && c >= 0x20 && c <= 0x7E => {
                    self.insert_char(c);
                }
                _ => {}
            }
        }
    }
}

fn longest_common_prefix(strings: &[&str]) -> String {
    if strings.is_empty() { return String::new(); }
    if strings.len() == 1 { return strings[0].to_string(); }
    let first = strings[0].as_bytes();
    let mut len = 0;
    'outer: for i in 0..first.len() {
        for s in &strings[1..] {
            if s.as_bytes().get(i) != Some(&first[i]) { break 'outer; }
        }
        len = i + 1;
    }
    strings[0][..len].to_string()
}

fn prompt() {
    vga_driver::write_str("\x1b[38;5;80m~ \x1b[38;5;213m☯\x1b[0m ");
    serial_driver::write_str("~ ☯ ");
}

pub fn shell_loop() -> ! {
    vga_driver::write_str("\x1b[38;5;80m☯ Mizu Kernel v0.1.0\x1b[0m\n");
    vga_driver::write_str("\x1b[2mcomandos: help | atalhos: Ctrl+C/D/L/U\x1b[0m\n\n");
    serial_driver::write_str("☯ Mizu Kernel v0.1.0\n");
    serial_driver::write_str("comandos: help | atalhos: Ctrl+C/D/L/U\n\n");

    let mut ed = LineEditor::new();

    loop {
        let line = match ed.get_line() {
            Some(l) => l,
            None => continue,
        };
        if line == "exit" || line == "quit" {
            break;
        }
        execute_line(&line);
    }

    vga_driver::write_str("\x1b[38;5;80m sayonara~\x1b[0m\n");
    loop { unsafe { core::arch::asm!("cli; hlt"); } }
}

fn execute_line(line: &str) {
    let tokens = match tokenize(line) {
        Ok(t) => t,
        Err(ParseError::UnterminatedQuote) => {
            vga_driver::write_str("mizu: aspas não fechadas\n");
            return;
        }
        Err(ParseError::Empty) => return,
    };

    let pipeline = match parse_pipeline(&tokens) {
        Ok(p) => p,
        Err(_) => {
            vga_driver::write_str("mizu: erro de parsing\n");
            return;
        }
    };

    match pipeline {
        Pipeline::Single(cmd) => execute_simple(&cmd),
        Pipeline::Pipe(_, _) => {
            // Pipes not yet supported with proper stdin/stdout
            vga_driver::write_str("mizu: pipes não suportados no kernel (ainda)\n");
        }
    }
}

fn execute_simple(cmd: &SimpleCommand) {
    if cmd.args.is_empty() {
        return;
    }

    let name = &cmd.args[0];
    let args: Vec<&str> = cmd.args[1..].iter().map(|s| s.as_str()).collect();

    match name.as_str() {
        "help"   => cmd_help(),
        "echo"   => cmd_echo(&args),
        "clear"  => cmd_clear(),
        "uname"  => cmd_uname(),
        "whoami" => cmd_whoami(),
        "date"   => cmd_date(),
        "yes"    => cmd_yes(),
        "ls"     => cmd_ls(&args),
        "cat"    => cmd_cat(&args),
        "head"   => cmd_head(&args),
        "wc"     => cmd_wc(&args),
        "hexdump" => cmd_hexdump(&args),
        "sh"     => cmd_sh(&args),
        "true"   => {},
        "false"  => {},
        "cp"     => cmd_cp(&args),
        "rm"     => cmd_rm(&args),
        "edit"   => cmd_edit(&args),
        "touch"  => cmd_touch(&args),
        "sleep"  => cmd_sleep(&args),
        _        => {
            // Try to run as a script file
            if let Some(data) = fs::file_read(name) {
                let s = core::str::from_utf8(data).unwrap_or("");
                for l in s.lines() {
                    let trimmed = l.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        execute_line(trimmed);
                    }
                }
            } else {
                vga_driver::write_str("mizu: comando não encontrado: ");
                vga_driver::write_str(name);
                vga_driver::put_char(b'\n');
            }
        }
    }
}

fn cmd_help() {
    vga_driver::write_str("comandos:\n");
    for cmd in COMMANDS {
        vga_driver::write_str("  ");
        vga_driver::write_str(cmd);
    }
    vga_driver::put_char(b'\n');
    vga_driver::write_str("\natalhos:\n");
    vga_driver::write_str("  Ctrl+C  — cancela linha\n");
    vga_driver::write_str("  Ctrl+D  — EOF\n");
    vga_driver::write_str("  Ctrl+L  — limpa tela\n");
    vga_driver::write_str("  Ctrl+U  — apaga linha\n");
    vga_driver::write_str("  Tab     — completar\n");
    vga_driver::write_str("  ▲/▼     — histórico\n");
    vga_driver::write_str("  ◀/▶     — mover cursor\n");
    vga_driver::write_str("  Home    — início da linha\n");
    vga_driver::write_str("  End     — fim da linha\n");
    vga_driver::write_str("  Del     — apagar após cursor\n");
}

fn cmd_echo(args: &[&str]) {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 { vga_driver::put_char(b' '); }
        vga_driver::write_str(arg);
    }
    vga_driver::put_char(b'\n');
}

fn cmd_clear() { vga_driver::clear(); }

fn cmd_uname() {
    vga_driver::write_str("Mizu OS\nKernel: mizu\nArch: i686\n");
}

fn cmd_whoami() { vga_driver::write_str("mizu\n"); }

fn cmd_date() {
    let ticks = crate::pit::get_ticks();
    vga_driver::write_str("ticks: ");
    print_u32(ticks);
    vga_driver::put_char(b'\n');
}

fn cmd_yes() {
    for _ in 0..20 {
        vga_driver::write_str("y\n");
    }
}

fn cmd_ls(_args: &[&str]) {
    let files = fs::file_list();
    if files.is_empty() {
        vga_driver::write_str("(vazio)\n");
        return;
    }
    for f in &files {
        vga_driver::write_str(f);
        vga_driver::put_char(b'\n');
    }
}

fn cmd_cat(args: &[&str]) {
    if args.is_empty() {
        vga_driver::write_str("uso: cat <arquivo>\n");
        return;
    }
    let name = args[0];
    match fs::file_read(name) {
        Some(data) => {
            vga_driver::write_str(core::str::from_utf8(data).unwrap_or("(binário)"));
            if data.len() > 0 && data[data.len() - 1] != b'\n' {
                vga_driver::put_char(b'\n');
            }
        }
        None => {
            vga_driver::write_str("mizu: arquivo não encontrado: ");
            vga_driver::write_str(name);
            vga_driver::put_char(b'\n');
        }
    }
}

fn cmd_head(args: &[&str]) {
    let name = if args.is_empty() { "help" } else { args[0] };
    match fs::file_read(name) {
        Some(data) => {
            let s = core::str::from_utf8(data).unwrap_or("");
            for (i, line) in s.lines().enumerate() {
                if i >= 10 { break; }
                vga_driver::write_str(line);
                vga_driver::put_char(b'\n');
            }
        }
        None => {
            vga_driver::write_str("mizu: arquivo não encontrado: ");
            vga_driver::write_str(name);
            vga_driver::put_char(b'\n');
        }
    }
}

fn cmd_wc(args: &[&str]) {
    let name = if args.is_empty() { "help" } else { args[0] };
    match fs::file_read(name) {
        Some(data) => {
            let s = core::str::from_utf8(data).unwrap_or("");
            let bytes = data.len();
            let lines = s.lines().count();
            let words = s.split_whitespace().count();
            vga_driver::write_str("  linhas: ");
            print_u32(lines as u32);
            vga_driver::write_str("  palavras: ");
            print_u32(words as u32);
            vga_driver::write_str("  bytes: ");
            print_u32(bytes as u32);
            vga_driver::put_char(b'\n');
        }
        None => {
            vga_driver::write_str("mizu: arquivo não encontrado: ");
            vga_driver::write_str(name);
            vga_driver::put_char(b'\n');
        }
    }
}

fn cmd_hexdump(args: &[&str]) {
    let name = if args.is_empty() { "help" } else { args[0] };
    match fs::file_read(name) {
        Some(data) => {
            let len = core::cmp::min(data.len(), 256);
            for i in (0..len).step_by(16) {
                print_u32(i as u32);
                vga_driver::write_str(": ");
                for j in 0..16 {
                    if i + j < len {
                        print_hex_byte(data[i + j]);
                        vga_driver::put_char(b' ');
                    }
                }
                vga_driver::write_str(" |");
                for j in 0..16 {
                    if i + j < len {
                        if data[i + j] >= 0x20 && data[i + j] <= 0x7E {
                            vga_driver::put_char(data[i + j]);
                        } else {
                            vga_driver::put_char(b'.');
                        }
                    }
                }
                vga_driver::write_str("|\n");
            }
        }
        None => {
            vga_driver::write_str("mizu: arquivo não encontrado: ");
            vga_driver::write_str(name);
            vga_driver::put_char(b'\n');
        }
    }
}

fn cmd_sh(args: &[&str]) {
    if args.is_empty() {
        vga_driver::write_str("uso: sh <arquivo>\n");
        return;
    }
    let name = args[0];
    match fs::file_read(name) {
        Some(data) => {
            let s = core::str::from_utf8(data).unwrap_or("");
            for l in s.lines() {
                let trimmed = l.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    execute_line(trimmed);
                }
            }
        }
        None => {
            vga_driver::write_str("mizu: arquivo não encontrado: ");
            vga_driver::write_str(name);
            vga_driver::put_char(b'\n');
        }
    }
}

fn cmd_cp(args: &[&str]) {
    if args.len() < 2 {
        vga_driver::write_str("uso: cp <origem> <destino>\n");
        return;
    }
    let src = args[0];
    let dst = args[1];
    match fs::file_read(src) {
        Some(data) => {
            if fs::file_write(dst, data) {
                vga_driver::write_str("ok\n");
            } else {
                vga_driver::write_str("erro ao escrever\n");
            }
        }
        None => {
            vga_driver::write_str("mizu: origem não encontrada: ");
            vga_driver::write_str(src);
            vga_driver::put_char(b'\n');
        }
    }
}

fn cmd_rm(args: &[&str]) {
    if args.is_empty() {
        vga_driver::write_str("uso: rm <arquivo>\n");
        return;
    }
    for name in args {
        if fs::file_remove(name) {
            vga_driver::write_str("removido: ");
            vga_driver::write_str(name);
            vga_driver::put_char(b'\n');
        } else {
            vga_driver::write_str("mizu: não encontrado: ");
            vga_driver::write_str(name);
            vga_driver::put_char(b'\n');
        }
    }
}

fn cmd_touch(args: &[&str]) {
    if args.is_empty() {
        vga_driver::write_str("uso: touch <arquivo>\n");
        return;
    }
    for name in args {
        if fs::file_exists(name) {
            continue;
        }
        fs::file_write(name, &[]);
        vga_driver::write_str("criado: ");
        vga_driver::write_str(name);
        vga_driver::put_char(b'\n');
    }
}

fn editor_read_key() -> u8 {
    loop {
        let c = read_byte();
        if c == 0x1B {
            let bracket = read_byte();
            if bracket != b'[' { continue; }
            let cmd = read_byte();
            match cmd {
                b'A' => return keyboard::KEY_UP,
                b'B' => return keyboard::KEY_DOWN,
                b'C' => return keyboard::KEY_RIGHT,
                b'D' => return keyboard::KEY_LEFT,
                b'H' => return keyboard::KEY_HOME,
                b'F' => return keyboard::KEY_END,
                b'3' => {
                    let tilde = read_byte();
                    if tilde == b'~' { return keyboard::KEY_DEL; }
                    continue;
                }
                _ => continue,
            }
        }
        return c;
    }
}

fn cmd_edit(args: &[&str]) {
    if args.is_empty() {
        vga_driver::write_str("uso: edit <arquivo>\n");
        return;
    }
    let filename = args[0];

    let mut lines: Vec<String> = Vec::new();
    if let Some(data) = fs::file_read(filename) {
        let s = core::str::from_utf8(data).unwrap_or("");
        for line in s.lines() {
            lines.push(String::from(line));
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }

    let fg = vga_driver::Color::LightGray;
    let bg = vga_driver::Color::Black;
    let rev_bg = vga_driver::Color::LightGray;
    let rev_fg = vga_driver::Color::Black;
    let default_attr = (bg as u8) << 4 | (fg as u8);
    let rev_attr = (rev_bg as u8) << 4 | (rev_fg as u8);

    let mut line_idx = 0usize;
    let mut col_idx = 0usize;
    let mut scroll = 0usize;
    let mut modified = false;
    let mut status_msg = String::new();
    let w = vga_driver::vga_width();
    let h = vga_driver::vga_height();
    let content_rows = h - 2;

    vga_driver::set_cursor_visible(false);

    'editor: loop {
        // Render title bar
        {
            let title = alloc::format!(" edit: {} {}", filename, if modified { "[+]" } else { "" });
            let truncated: String = title.chars().take(w - 2).collect();
            for c in 0..w {
                let ch = if c == 0 { b'\xDA' }
                    else if c == w - 1 { b'\xBF' }
                    else if c - 1 < truncated.len() { truncated.as_bytes()[c - 1] }
                    else { b'\xC4' };
                vga_driver::write_char_rc(0, c, ch, rev_attr);
            }
        }

        // Render content area
        for r in 0..content_rows {
            let buf_idx = scroll + r;
            if buf_idx < lines.len() {
                let line = &lines[buf_idx];
                // write spaces first
                for c in 0..w {
                    vga_driver::write_char_rc(1 + r, c, b' ', default_attr);
                }
                // write content
                for (c, &b) in line.as_bytes().iter().enumerate() {
                    if c >= w { break; }
                    vga_driver::write_char_rc(1 + r, c, b, default_attr);
                }
            } else {
                for c in 0..w {
                    vga_driver::write_char_rc(1 + r, c, b' ', default_attr);
                }
            }
        }

        // Render status bar
        {
            let status = if status_msg.is_empty() {
                alloc::format!(" ^S save  ^X quit   lin {}/{} col {}", line_idx + 1, lines.len(), col_idx + 1)
            } else {
                status_msg.clone()
            };
            for c in 0..w {
                let ch = if c == 0 { b'\xC0' }
                    else if c == w - 1 { b'\xD9' }
                    else if c - 1 < status.len() { status.as_bytes()[c - 1] }
                    else { b'\xC4' };
                vga_driver::write_char_rc(1 + content_rows, c, ch, rev_attr);
            }
        }

        // Place cursor
        let screen_row = 1 + line_idx.saturating_sub(scroll);
        let screen_col = core::cmp::min(col_idx, w - 1);
        if screen_row < h {
            vga_driver::set_cursor(screen_row, screen_col);
        }

        // Read key
        let key = editor_read_key();
        status_msg.clear();

        match key {
            b'\n' | b'\r' => {
                // Split line at cursor
                let rest: String = lines[line_idx][col_idx..].to_string();
                lines[line_idx].truncate(col_idx);
                line_idx += 1;
                lines.insert(line_idx, rest);
                col_idx = 0;
                modified = true;
                // Auto-scroll
                if line_idx >= scroll + content_rows {
                    scroll = line_idx + 1 - content_rows;
                }
            }
            0x08 | 0x7F => {
                if col_idx > 0 {
                    col_idx -= 1;
                    lines[line_idx].remove(col_idx);
                    modified = true;
                } else if line_idx > 0 {
                    // Join with previous line
                    let prev_len = lines[line_idx - 1].len();
                    let rest = lines.remove(line_idx);
                    col_idx = prev_len;
                    line_idx -= 1;
                    lines[line_idx].push_str(&rest);
                    modified = true;
                    if scroll > 0 && line_idx < scroll {
                        scroll = line_idx;
                    }
                }
            }
            keyboard::KEY_DEL => {
                if col_idx < lines[line_idx].len() {
                    lines[line_idx].remove(col_idx);
                    modified = true;
                } else if line_idx + 1 < lines.len() {
                    let rest = lines.remove(line_idx + 1);
                    lines[line_idx].push_str(&rest);
                    modified = true;
                }
            }
            keyboard::KEY_LEFT => {
                if col_idx > 0 {
                    col_idx -= 1;
                } else if line_idx > 0 {
                    line_idx -= 1;
                    col_idx = lines[line_idx].len();
                    if scroll > 0 && line_idx < scroll {
                        scroll = line_idx;
                    }
                }
            }
            keyboard::KEY_RIGHT => {
                if col_idx < lines[line_idx].len() {
                    col_idx += 1;
                } else if line_idx + 1 < lines.len() {
                    line_idx += 1;
                    col_idx = 0;
                    if line_idx >= scroll + content_rows {
                        scroll = line_idx + 1 - content_rows;
                    }
                }
            }
            keyboard::KEY_UP => {
                if line_idx > 0 {
                    line_idx -= 1;
                    if col_idx > lines[line_idx].len() {
                        col_idx = lines[line_idx].len();
                    }
                    if scroll > 0 && line_idx < scroll {
                        scroll = line_idx;
                    }
                }
            }
            keyboard::KEY_DOWN => {
                if line_idx + 1 < lines.len() {
                    line_idx += 1;
                    if col_idx > lines[line_idx].len() {
                        col_idx = lines[line_idx].len();
                    }
                    if line_idx >= scroll + content_rows {
                        scroll = line_idx + 1 - content_rows;
                    }
                }
            }
            keyboard::KEY_HOME => { col_idx = 0; }
            keyboard::KEY_END => { col_idx = lines[line_idx].len(); }
            // Ctrl+Q or Ctrl+X: quit
            0x11 | 0x18 => {
                if modified {
                    status_msg = " sem salvar! ^S para salvar, ^Q de novo para sair".to_string();
                    // second press quits anyway
                    let key2 = editor_read_key();
                    if key2 == 0x11 || key2 == 0x18 {
                        break 'editor;
                    }
                    // Re-push unhandled key... hmm, this is tricky.
                    // For now, just continue and the status will show.
                    // On next loop iteration, we'll get the actual key
                    continue;
                }
                break 'editor;
            }
            // Ctrl+S: save
            0x13 => {
                let mut data = Vec::new();
                for (i, line) in lines.iter().enumerate() {
                    data.extend_from_slice(line.as_bytes());
                    if i + 1 < lines.len() {
                        data.push(b'\n');
                    }
                }
                if fs::file_write(filename, &data) {
                    status_msg = " salvo!".to_string();
                    modified = false;
                } else {
                    status_msg = " erro ao salvar!".to_string();
                }
            }
            c if c >= 0x20 && c <= 0x7E => {
                lines[line_idx].insert(col_idx, c as char);
                col_idx += 1;
                modified = true;
                if col_idx >= w {
                    // wrap insertion to next line
                    let rest: String = lines[line_idx][col_idx..].to_string();
                    lines[line_idx].truncate(col_idx);
                    line_idx += 1;
                    lines.insert(line_idx, rest);
                    col_idx = 0;
                    if line_idx >= scroll + content_rows {
                        scroll = line_idx + 1 - content_rows;
                    }
                }
            }
            _ => {}
        }
    }

    vga_driver::set_cursor_visible(true);
    vga_driver::clear();
}

fn cmd_sleep(args: &[&str]) {
    let ticks: u32 = args.get(0).and_then(|s| s.parse().ok()).unwrap_or(10);
    let start = crate::pit::get_ticks();
    loop {
        let now = crate::pit::get_ticks();
        if now.wrapping_sub(start) >= ticks {
            break;
        }
    }
}

fn print_u32(mut n: u32) {
    if n == 0 {
        vga_driver::put_char(b'0');
        return;
    }
    let mut buf = [0u8; 12];
    let mut i = 12;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    vga_driver::write_str(core::str::from_utf8(&buf[i..]).unwrap_or("?"));
}

fn print_hex_byte(b: u8) {
    let hex = b"0123456789abcdef";
    vga_driver::put_char(hex[(b >> 4) as usize]);
    vga_driver::put_char(hex[(b & 0xF) as usize]);
}
