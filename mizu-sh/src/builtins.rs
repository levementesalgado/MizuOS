use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use mizu_core::expand_path_env as expand_path;

pub struct Builtins {
    handlers: HashMap<&'static str, fn(&[&str])>,
}

impl Builtins {
    pub fn new() -> Self {
        let mut handlers: HashMap<&'static str, fn(&[&str])> = HashMap::new();
        handlers.insert("help", cmd_help);
        handlers.insert("ls", cmd_ls);
        handlers.insert("cat", cmd_cat);
        handlers.insert("echo", cmd_echo);
        handlers.insert("rm", cmd_rm);
        handlers.insert("rmdir", cmd_rmdir);
        handlers.insert("mkdir", cmd_mkdir);
        handlers.insert("touch", cmd_touch);
        handlers.insert("cp", cmd_cp);
        handlers.insert("mv", cmd_mv);
        handlers.insert("clear", cmd_clear);
        handlers.insert("pwd", cmd_pwd);
        handlers.insert("whoami", cmd_whoami);
        handlers.insert("date", cmd_date);
        handlers.insert("uname", cmd_uname);
        handlers.insert("cd", cmd_cd);
        handlers.insert("mount", cmd_mount);
        handlers.insert("umount", cmd_umount);
        handlers.insert("hostname", cmd_hostname);

        Self { handlers }
    }

    pub fn is_builtin(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    /// Executa um builtin. Retorna true se achou e executou.
    pub fn exec(&self, name: &str, args: &[String]) -> bool {
        if let Some(&handler) = self.handlers.get(name) {
            let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            handler(&refs);
            true
        } else {
            false
        }
    }
}

// ── Builtins ─────────────────────────────────────────────────────

fn cmd_help(_args: &[&str]) {
    println!("\x1b[38;5;80m☯ Mizu Shell — Comandos\x1b[0m");
    println!();
    let cmds = [
        ("help", "mostra esta ajuda"),
        ("ls", "lista diretório"),
        ("cat", "mostra conteúdo de arquivo"),
        ("echo", "imprime argumentos"),
        ("cd", "muda diretório"),
        ("pwd", "diretório atual"),
        ("mkdir", "cria diretório"),
        ("rmdir", "remove diretório vazio"),
        ("rm", "remove arquivo"),
        ("touch", "cria arquivo vazio"),
        ("cp", "copia arquivo"),
        ("mv", "move/renomeia arquivo"),
        ("clear", "limpa terminal"),
        ("whoami", "nome do usuário"),
        ("date", "data/hora atual"),
        ("uname", "informações do sistema"),
        ("exit", "sai do shell"),
        ("mount", "monta sistema de arquivos"),
        ("umount", "desmonta sistema de arquivos"),
        ("hostname", "mostra/define hostname"),
    ];
    for (name, desc) in &cmds {
        println!("  \x1b[38;5;213m{:8}\x1b[0m  {}", name, desc);
    }
    println!();
    println!("\x1b[2mPipes: cmd1 | cmd2  |  Redir: > >> <\x1b[0m");
}

fn cmd_ls(args: &[&str]) {
    let show_all = args.contains(&"-a");
    let long = args.contains(&"-l");
    let path = args.iter().find(|a| !a.starts_with('-')).unwrap_or(&".");
    let dir = expand_path(path);

    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("ls: {}: {}", dir, e);
            return;
        }
    };

    let mut items: Vec<(String, fs::FileType)> = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            show_all || !name.starts_with('.')
        })
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let ft = e.file_type().ok()?;
            Some((name, ft))
        })
        .collect();

    items.sort_by(|a, b| a.0.cmp(&b.0));

    if long {
        for (name, ft) in &items {
            let kind = if ft.is_dir() { "d" } else { "-" };
            let color = if ft.is_dir() {
                "\x1b[38;5;80m"
            } else if ft.is_symlink() {
                "\x1b[38;5;213m"
            } else {
                "\x1b[0m"
            };
            // Tamanho legível estilo Nushell (B/KB/MB).
            let size_str = fs::metadata(expand_path(name).as_str())
                .map(|m| human_size(m.len()))
                .unwrap_or_else(|_| "-".into());
            println!("{} {} {}{}\x1b[0m", kind, size_str, color, name);
        }
    } else {
        let parts: Vec<String> = items
            .iter()
            .map(|(name, ft)| {
                let color = if ft.is_dir() {
                    "\x1b[38;5;80m"
                } else if ft.is_symlink() {
                    "\x1b[38;5;213m"
                } else {
                    ""
                };
                let suffix = if ft.is_dir() { "/" } else { "" };
                format!("{}{}{}\x1b[0m{}", color, name, "\x1b[0m", suffix)
            })
            .collect();
        // Colunas adaptativas
        let term_w = terminal_width().unwrap_or(80);
        let max_len = parts.iter().map(|p| visible_len(p)).max().unwrap_or(10) + 2;
        let cols = (term_w / max_len).max(1);
        for chunk in parts.chunks(cols) {
            for p in chunk {
                let visible = visible_len(p);
                print!("{}", p);
                if visible < max_len {
                    print!("{}", " ".repeat(max_len - visible));
                }
            }
            println!();
        }
    }
}

fn cmd_cat(args: &[&str]) {
    if args.is_empty() {
        // Lê de stdin
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).ok();
        print!("{}", buf);
        return;
    }
    for path in args {
        let expanded = expand_path(path);
        match fs::read_to_string(&expanded) {
            Ok(content) => print!("{}", content),
            Err(e) => eprintln!("cat: {}: {}", path, e),
        }
    }
}

fn cmd_echo(args: &[&str]) {
    let line = args.join(" ");
    println!("{}", line);
}

fn cmd_cd(args: &[&str]) {
    let target = args.first().unwrap_or(&"~");
    let expanded = expand_path(target);
    if let Err(e) = std::env::set_current_dir(&expanded) {
        eprintln!("cd: {}: {}", target, e);
    }
}

fn cmd_pwd(_args: &[&str]) {
    let cwd = std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    println!("{}", cwd);
}

fn cmd_mkdir(args: &[&str]) {
    for path in args {
        let expanded = expand_path(path);
        if let Err(e) = fs::create_dir_all(&expanded) {
            eprintln!("mkdir: {}: {}", path, e);
        }
    }
}

fn cmd_rmdir(args: &[&str]) {
    for path in args {
        let expanded = expand_path(path);
        if let Err(e) = fs::remove_dir(&expanded) {
            eprintln!("rmdir: {}: {}", path, e);
        }
    }
}

fn cmd_rm(args: &[&str]) {
    let recursive = args.contains(&"-r") || args.contains(&"-rf");
    let force = args.contains(&"-f") || args.contains(&"-rf");
    for path in args {
        if path.starts_with('-') {
            continue;
        }
        let expanded = expand_path(path);
        let result = if recursive {
            fs::remove_dir_all(&expanded)
        } else {
            fs::remove_file(&expanded)
        };
        if let Err(e) = result {
            if !force {
                eprintln!("rm: {}: {}", path, e);
            }
        }
    }
}

fn cmd_touch(args: &[&str]) {
    for path in args {
        let expanded = expand_path(path);
        if Path::new(&expanded).exists() {
            // Atualiza timestamp
            let _ = filetime(&expanded);
        } else {
            if let Err(e) = fs::File::create(&expanded) {
                eprintln!("touch: {}: {}", path, e);
            }
        }
    }
}

fn cmd_cp(args: &[&str]) {
    if args.len() < 2 {
        eprintln!("cp: faltam argumentos (origem destino)");
        return;
    }
    let src = expand_path(args[args.len() - 2]);
    let dst = expand_path(args[args.len() - 1]);
    if let Err(e) = fs::copy(&src, &dst) {
        eprintln!("cp: {} -> {}: {}", src, dst, e);
    }
}

fn cmd_mv(args: &[&str]) {
    if args.len() < 2 {
        eprintln!("mv: faltam argumentos (origem destino)");
        return;
    }
    let src = expand_path(args[args.len() - 2]);
    let dst = expand_path(args[args.len() - 1]);
    if let Err(e) = fs::rename(&src, &dst) {
        eprintln!("mv: {} -> {}: {}", src, dst, e);
    }
}

fn cmd_clear(_args: &[&str]) {
    print!("\x1b[2J\x1b[H");
    io::stdout().flush().ok();
}

fn cmd_whoami(_args: &[&str]) {
    let user = std::env::var("USER").unwrap_or_else(|_| "mizu".into());
    println!("{}", user);
}

fn cmd_date(_args: &[&str]) {
    let now = chrono::Local::now();
    println!("{}", now.format("%a %b %d %H:%M:%S %Y"));
}

fn cmd_uname(_args: &[&str]) {
    println!("Mizu OS");
    println!("Kernel: Linux (userspace)");
    println!("Arch: i686");
    #[cfg(target_arch = "x86")]
    println!("CPU: i686");
    #[cfg(target_arch = "x86_64")]
    println!("CPU: x86_64");
}

fn cmd_mount(args: &[&str]) {
    if args.len() < 2 {
        eprintln!("mount: uso: mount -t tipo dispositivo diretorio");
        return;
    }
    let mut fs_type = "";
    let mut source = "";
    let mut target = "";
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-t" => { i += 1; if i < args.len() { fs_type = args[i]; } }
            s if source.is_empty() => source = s,
            s => target = s,
        }
        i += 1;
    }
    if target.is_empty() {
        target = source;
        source = "";
    }
    let src_c = std::ffi::CString::new(source).unwrap_or_default();
    let target_c = std::ffi::CString::new(target).unwrap_or_default();
    let fs_c = std::ffi::CString::new(fs_type).unwrap_or_default();
    let ret = unsafe { libc::mount(src_c.as_ptr(), target_c.as_ptr(), fs_c.as_ptr(), 0, std::ptr::null()) };
    if ret != 0 {
        eprintln!("mount: erro ao montar {} em {}: {}", source, target, std::io::Error::last_os_error());
    }
}

fn cmd_umount(args: &[&str]) {
    for target in args {
        let target_c = std::ffi::CString::new(*target).unwrap_or_default();
        let ret = unsafe { libc::umount(target_c.as_ptr()) };
        if ret != 0 {
            eprintln!("umount: erro ao desmontar {}: {}", target, std::io::Error::last_os_error());
        }
    }
}

fn cmd_hostname(args: &[&str]) {
    if args.is_empty() {
        let mut buf = [0i8; 256];
        let ret = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut _, buf.len()) };
        if ret == 0 {
            let name = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_string_lossy();
            println!("{}", name);
        }
    } else {
        let name_c = std::ffi::CString::new(args[0]).unwrap_or_default();
        unsafe { libc::sethostname(name_c.as_ptr(), args[0].len()) };
    }
}

// ── Helpers ──────────────────────────────────────────────────────

fn human_size(n: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 { v /= 1024.0; i += 1; }
    if i == 0 { format!("{:>6} {}", n, UNITS[i]) } else { format!("{:>6.1} {}", v, UNITS[i]) }
}

fn visible_len(s: &str) -> usize {
    // Remove ANSI codes
    let re = regex_lite::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").len()
}

fn terminal_width() -> Option<usize> {
    let (w, _) = termion::terminal_size().ok()?;
    Some(w as usize)
}

fn filetime(path: &str) -> std::io::Result<()> {
    let now = std::time::SystemTime::now();
    let ft: filetime::FileTime = now.into();
    filetime::set_file_times(path, ft, ft)
}
