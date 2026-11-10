use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

use rustyline::completion::{Completer, Pair};
use rustyline::config::Builder;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::{Hint, Hinter};
use rustyline::history::DefaultHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Context, Editor, Helper};

use mizu_core::{expand_path_env, parse_pipeline, tokenize, ParseError, Pipeline, SimpleCommand};

mod builtins;
use builtins::Builtins;

fn render_prompt() -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "?".into());
    let home = std::env::var("HOME").unwrap_or_default();
    let short = if cwd.starts_with(&home) {
        format!("~{}", &cwd[home.len()..])
    } else {
        cwd.clone()
    };
    format!("\x1b[38;5;80m{}\x1b[0m \x1b[38;5;213m☯\x1b[0m ", short)
}

/// ── Helper ──────────────────────────────────────────────────────
const COMMANDS: &[&str] = &[
    "help", "ls", "cat", "echo", "cd", "pwd", "mkdir", "rmdir", "rm", "touch",
    "cp", "mv", "clear", "whoami", "date", "uname", "mount", "umount", "hostname", "exit",
];

// Abreviações estilo Fish (expansão no primeiro token).
const ABBREVS: &[(&str, &str)] = &[
    ("ll", "ls -la"),
    ("la", "ls -a"),
    ("g", "git"),
    ("h", "help"),
    ("e", "echo"),
    ("c", "cat"),
];

fn expand_abbrev(line: &str) -> String {
    let t = line.trim_start();
    if t.is_empty() { return line.to_string(); }
    let first = t.split_whitespace().next().unwrap_or("");
    // Só expande se a linha toda é a abreviação ou ela + espaço + resto.
    for (short, long) in ABBREVS {
        if first == *short {
            let rest = t[first.len()..].trim_start();
            if rest.is_empty() {
                let indent = line.len() - t.len();
                return format!("{}{}", " ".repeat(indent), long);
            } else {
                let indent = line.len() - t.len();
                return format!("{}{} {}", " ".repeat(indent), long, rest);
            }
        }
    }
    line.to_string()
}

struct MizuHelper;

impl Helper for MizuHelper {}

impl Completer for MizuHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let line_before = &line[..pos];
        let first_token = line_before.split_whitespace().next().unwrap_or("");
        // Completando a primeira palavra? Oferece comandos + abreviações.
        if line_before.trim_end().len() == first_token.len() {
            let mut cmds: Vec<Pair> = COMMANDS.iter()
                .filter(|c| c.starts_with(first_token))
                .map(|c| Pair { display: c.to_string(), replacement: c.to_string() })
                .collect();
            for (short, long) in ABBREVS {
                if short.starts_with(first_token) {
                    cmds.push(Pair { display: format!("{} → {}", short, long), replacement: short.to_string() });
                }
            }
            if !cmds.is_empty() {
                return Ok((0, cmds));
            }
        }
        let token = line_before.split_whitespace().last().unwrap_or("");
        let prefix = expand_path_env(token);

        let parent = if prefix.ends_with('/') {
            Path::new(&prefix)
        } else {
            Path::new(&prefix).parent().unwrap_or(Path::new("."))
        };
        let partial: String = if prefix.ends_with('/') {
            String::new()
        } else {
            Path::new(&prefix)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        };

        let mut candidates = Vec::new();
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&partial) {
                    let mut display = name.clone();
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        display.push('/');
                    }
                    candidates.push(Pair { display, replacement: name });
                }
            }
        }

        let start = line_before.len() - token.len();
        Ok((start, candidates))
    }
}

impl Hinter for MizuHelper {
    type Hint = MizuHint;
    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<MizuHint> {
        // Sugestão estilo Fish: só no primeiro token, completa comando conhecido.
        if pos == 0 { return None; }
        let before = &line[..pos.min(line.len())];
        if before.contains(char::is_whitespace) { return None; }
        let cur = before.trim();
        if cur.is_empty() || cur.len() < 2 { return None; }
        for c in COMMANDS {
            if c.starts_with(cur) && *c != cur {
                return Some(MizuHint { suffix: c[cur.len()..].to_string() });
            }
        }
        for (short, long) in ABBREVS {
            if long.starts_with(cur) && cur.len() >= 2 {
                let _ = short;
                return Some(MizuHint { suffix: long[cur.len()..].to_string() });
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
struct MizuHint {
    suffix: String,
}
impl Hint for MizuHint {
    fn completion(&self) -> Option<&str> { Some(&self.suffix) }
    fn display(&self) -> &str { &self.suffix }
}

impl Highlighter for MizuHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&self, prompt: &'b str, _default: bool) -> std::borrow::Cow<'b, str> {
        std::borrow::Cow::Owned(prompt.to_string())
    }
    fn highlight_hint<'h>(&self, hint: &'h str) -> std::borrow::Cow<'h, str> {
        std::borrow::Cow::Owned(format!("\x1b[2m{}\x1b[0m", hint))
    }
    fn highlight<'l>(&self, line: &'l str, _cursor: usize) -> std::borrow::Cow<'l, str> {
        // Highlight simples: comando (azul), string (rosa), operador (negrito).
        let mut out = String::new();
        let mut first = true;
        for tok in split_args_keep(line) {
            if tok.is_empty() {
                continue;
            }
            let delim = &tok.trim();
            if delim.is_empty() {
                out.push_str(&tok);
                continue;
            }
            let word: String = tok.chars().take_while(|c| !c.is_whitespace()).collect();
            let rest: String = tok.chars().skip_while(|c| !c.is_whitespace()).collect();
            if ["|", ">", ">>", "<"].contains(&word.as_str()) {
                out.push_str(&format!("\x1b[1m{}\x1b[0m{}", word, rest));
                first = false;
                continue;
            }
            if (word.starts_with('"') && word.ends_with('"')) || (word.starts_with('\'') && word.ends_with('\'')) {
                out.push_str(&format!("\x1b[38;5;213m{}\x1b[0m{}", word, rest));
            } else if first {
                let known = COMMANDS.contains(&word.as_str()) || std::fs::metadata(&word).is_ok();
                let color = if known { "38;5;80" } else { "31" };
                out.push_str(&format!("\x1b[{}m{}\x1b[0m{}", color, word, rest));
                first = false;
                continue;
            } else if word.starts_with('-') {
                out.push_str(&format!("\x1b[2m{}\x1b[0m{}", word, rest));
            } else {
                out.push_str(&format!("{}{}", word, rest));
            }
            if !["|", ">", ">>", "<"].contains(&word.as_str()) {
                first = false;
            }
        }
        std::borrow::Cow::Owned(out)
    }
    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool { false }
}

fn split_args_keep(line: &str) -> Vec<String> {
    // Mantém espaços como tokens próprios para highlight não comer a linha.
    let mut v = Vec::new();
    let mut cur = String::new();
    for ch in line.chars() {
        if ch.is_whitespace() {
            if !cur.is_empty() { v.push(std::mem::take(&mut cur)); }
            if cur.is_empty() { v.push(ch.to_string()); }
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() { v.push(cur); }
    v
}

impl Validator for MizuHelper {
    fn validate(&self, _ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        Ok(ValidationResult::Valid(None))
    }
}

/// ── Execução ────────────────────────────────────────────────────
fn execute_command(cmd: &SimpleCommand, builtins: &Builtins) -> ExitStatus {
    if cmd.args.is_empty() {
        return ExitStatus::default();
    }

    let cmd_name = &cmd.args[0];
    let args = &cmd.args[1..];

    if builtins.exec(cmd_name, args) {
        return ExitStatus::default();
    }

    let stdin: Stdio = match &cmd.stdin_file {
        Some(file) => {
            let path = expand_path_env(file);
            Stdio::from(std::fs::File::open(&path).unwrap_or_else(|_| {
                eprintln!("mizu-sh: {}: arquivo não encontrado", path);
                std::process::exit(1);
            }))
        }
        None => Stdio::inherit(),
    };

    let stdout: Stdio = match &cmd.stdout_file {
        Some(file) => {
            let path = expand_path_env(file);
            if cmd.stdout_append {
                Stdio::from(
                    std::fs::OpenOptions::new()
                        .create(true).append(true).open(&path)
                        .unwrap_or_else(|_| { eprintln!("mizu-sh: erro ao abrir {}", path); std::process::exit(1); }),
                )
            } else {
                Stdio::from(
                    std::fs::File::create(&path).unwrap_or_else(|_| { eprintln!("mizu-sh: erro ao criar {}", path); std::process::exit(1); }),
                )
            }
        }
        None => Stdio::inherit(),
    };

    let mut child = match Command::new(cmd_name).args(args).stdin(stdin).stdout(stdout).spawn() {
        Ok(c) => c,
        Err(_) => {
            eprintln!("mizu-sh: {}: comando não encontrado", cmd_name);
            return ExitStatus::default();
        }
    };
    child.wait().unwrap_or_default()
}

fn execute_pipeline(pipeline: &Pipeline, builtins: &Builtins) {
    match pipeline {
        Pipeline::Single(cmd) => { execute_command(cmd, builtins); }
        Pipeline::Pipe(left, right) => {
            let left_out = execute_piped(left, builtins);
            let mut right_cmd = prepare_external(right, builtins);
            right_cmd.stdin(left_out);
            if let Some(mut child) = right_cmd.spawn().ok() {
                let _ = child.wait();
            }
        }
    }
}

fn execute_piped(pipeline: &Pipeline, builtins: &Builtins) -> std::process::ChildStdout {
    match pipeline {
        Pipeline::Single(cmd) => {
            let mut child = prepare_external(cmd, builtins)
                .stdout(Stdio::piped())
                .spawn()
                .expect("mizu-sh: falha ao spawnar");
            child.stdout.take().expect("mizu-sh: sem stdout")
        }
        Pipeline::Pipe(left, right) => {
            let left_out = execute_piped(left, builtins);
            let mut right_cmd = prepare_external(right, builtins);
            right_cmd.stdin(left_out).stdout(Stdio::piped());
            let mut child = right_cmd.spawn().expect("mizu-sh: falha ao spawnar");
            child.stdout.take().expect("mizu-sh: sem stdout")
        }
    }
}

fn prepare_external(cmd: &SimpleCommand, builtins: &Builtins) -> Command {
    let cmd_name = &cmd.args[0];
    let args = &cmd.args[1..];

    if builtins.is_builtin(cmd_name) {
        let full_cmd = format!("{} {}", cmd_name, args.join(" "));
        let mut c = Command::new(std::env::current_exe().unwrap_or_else(|_| "mizu-sh".into()));
        c.arg("-c").arg(&full_cmd);
        c
    } else {
        let mut c = Command::new(cmd_name);
        c.args(args);
        c
    }
}

fn run_line(line: &str, builtins: &Builtins) {
    let expanded = expand_abbrev(line);
    let tokens = match tokenize(&expanded) {
        Ok(t) => t,
        Err(ParseError::UnterminatedQuote) => { eprintln!("mizu-sh: aspas não fechadas"); return; }
        Err(ParseError::Empty) => return,
    };
    let pipeline = match parse_pipeline(&tokens) {
        Ok(p) => p,
        Err(_) => { eprintln!("mizu-sh: erro de parsing"); return; }
    };
    execute_pipeline(&pipeline, builtins);
}

/// ── Main ────────────────────────────────────────────────────────
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let builtins = Builtins::new();

    if args.len() > 2 && args[1] == "-c" {
        let input = args[2..].join(" ");
        run_line(&input, &builtins);
        return;
    }

    let config = Builder::new()
        .history_ignore_space(true)
        .edit_mode(rustyline::config::EditMode::Emacs)
        .build();

    let mut rl: Editor<MizuHelper, DefaultHistory> = Editor::with_config(config)
        .expect("mizu-sh: falha ao criar editor");

    rl.set_helper(Some(MizuHelper));
    rl.load_history(".mizu_history").ok();

    println!("\x1b[38;5;80m☯ Mizu Shell v0.1.0\x1b[0m");
    println!("\x1b[2mdigite help para comandos\x1b[0m");

    loop {
        let prompt = render_prompt();
        match rl.readline(&prompt) {
            Ok(input) => {
                let trimmed = input.trim().to_string();
                if trimmed.is_empty() { continue; }
                if trimmed == "exit" { break; }
                let _ = rl.add_history_entry(&trimmed);
                run_line(&trimmed, &builtins);
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(e) => { eprintln!("mizu-sh: erro: {:?}", e); break; }
        }
    }

    rl.save_history(".mizu_history").ok();
    println!("\x1b[38;5;80m☯ sayonara~\x1b[0m");
}
