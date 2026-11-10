#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}

fn syscall3(nr: u32, a1: u32, a2: u32, a3: u32) -> u32 {
    let ret: u32;
    unsafe {
        let mut _a1 = a1;
        core::arch::asm!(
            "xchg ebx, {a1}",
            "int 0x80",
            "xchg ebx, {a1}",
            a1 = inout(reg) _a1,
            in("eax") nr,
            in("ecx") a2,
            in("edx") a3,
            lateout("eax") ret,
        );
    }
    ret
}

fn sys_write(fd: u32, s: &[u8]) {
    syscall3(4, fd, s.as_ptr() as u32, s.len() as u32);
}

fn sys_read(fd: u32, buf: &mut [u8]) -> usize {
    syscall3(3, fd, buf.as_mut_ptr() as u32, buf.len() as u32) as usize
}

fn sys_getdents(buf: &mut [u8]) -> i32 {
    syscall3(141, 0, buf.as_mut_ptr() as u32, buf.len() as u32) as i32
}

fn sys_exit(code: u32) -> ! {
    syscall3(1, code, 0, 0);
    loop {}
}

fn sys_fork() -> i32 {
    syscall3(2, 0, 0, 0) as i32
}

fn sys_execve(path: &str, argv: u32, envp: u32) -> i32 {
    let mut buf = [0u8; 128];
    let bytes = path.as_bytes();
    let len = bytes.len().min(127);
    buf[..len].copy_from_slice(&bytes[..len]);
    buf[len] = 0;
    syscall3(11, buf.as_ptr() as u32, argv, envp) as i32
}

fn sys_waitpid(pid: i32, status: &mut u32) -> i32 {
    syscall3(7, pid as u32, status as *mut u32 as u32, 0) as i32
}

fn sys_open(path: &str, flags: u32) -> i32 {
    let mut buf = [0u8; 128];
    let bytes = path.as_bytes();
    let len = bytes.len().min(127);
    buf[..len].copy_from_slice(&bytes[..len]);
    buf[len] = 0;
    syscall3(5, buf.as_ptr() as u32, flags, 0) as i32
}

fn sys_close(fd: u32) -> i32 {
    syscall3(6, fd, 0, 0) as i32
}

const STDIN: u32 = 0;
const STDOUT: u32 = 1;
const STDERR: u32 = 2;

fn print(s: &str) {
    sys_write(STDOUT, s.as_bytes());
}

fn println(s: &str) {
    print(s);
    sys_write(STDOUT, b"\n");
}

fn read_line(buf: &mut [u8]) -> Option<&str> {
    let n = sys_read(STDIN, buf);
    if n == 0 { return None; }
    let end = buf[..n].iter().position(|&b| b == b'\n' || b == b'\r').unwrap_or(n);
    core::str::from_utf8(&buf[..end]).ok()
}

struct Args<'a> {
    argv: [&'a str; 16],
    argc: usize,
}

fn parse_args(line: &str) -> Args {
    let mut argv = [""; 16];
    let mut argc = 0;
    let mut i = 0;
    let bytes = line.as_bytes();
    while i < bytes.len() && argc < 16 {
        while i < bytes.len() && bytes[i] == b' ' { i += 1; }
        if i >= bytes.len() { break; }
        let start = i;
        if bytes[i] == b'"' {
            i += 1;
            let inner = i;
            while i < bytes.len() && bytes[i] != b'"' { i += 1; }
            if let Ok(s) = core::str::from_utf8(&bytes[inner..i]) {
                argv[argc] = s;
                argc += 1;
            }
            i += 1;
        } else {
            while i < bytes.len() && bytes[i] != b' ' { i += 1; }
            if let Ok(s) = core::str::from_utf8(&bytes[start..i]) {
                argv[argc] = s;
                argc += 1;
            }
        }
    }
    Args { argv, argc }
}

fn cmd_ls() {
    let mut buf = [0u8; 512];
    let n = sys_getdents(&mut buf);
    if n <= 0 { return; }
    let mut off = 0usize;
    while (off as i32) < n {
        if off + 19 > buf.len() { break; }
        let reclen = u16::from_le_bytes([buf[off + 16], buf[off + 17]]);
        if reclen < 19 { break; }
        let name_end = off + 18 + 1 + (buf[off + 19..].iter().position(|&b| b == 0).unwrap_or(reclen as usize - 19));
        if let Ok(s) = core::str::from_utf8(&buf[off + 19..name_end]) {
            if !s.is_empty() && s != "." && s != ".." {
                println(s);
            }
        }
        off += reclen as usize;
    }
}

fn cmd_cat(args: &Args) {
    if args.argc < 2 { println("usage: cat <file>"); return; }
    let fd = sys_open(args.argv[1], 0);
    if fd < 0 { println("cat: file not found"); return; }
    let mut buf = [0u8; 512];
    loop {
        let n = syscall3(3, fd as u32, buf.as_mut_ptr() as u32, buf.len() as u32) as usize;
        if n == 0 { break; }
        sys_write(STDOUT, &buf[..n]);
    }
    sys_close(fd as u32);
}

fn cmd_cp(args: &Args) {
    if args.argc < 3 { println("usage: cp <src> <dst>"); return; }
    let src = sys_open(args.argv[1], 0);
    if src < 0 { println("cp: source not found"); return; }
    let mut data = [0u8; 4096];
    let mut total = 0usize;
    loop {
        let n = syscall3(3, src as u32, data.as_mut_ptr() as u32, data.len() as u32) as usize;
        if n == 0 { break; }
        if total + n > data.len() { break; }
        total += n;
    }
    sys_close(src as u32);
    // Write to destination (O_WRONLY | O_CREAT | O_TRUNC)
    let dst = sys_open(args.argv[2], 0o1 | 0o100 | 0o1000);
    if dst < 0 { println("cp: cannot create destination"); return; }
    syscall3(4, dst as u32, data.as_ptr() as u32, total as u32);
    sys_close(dst as u32);
}

fn cmd_rm(args: &Args) {
    if args.argc < 2 { println("usage: rm <file>"); return; }
    // Use unlink syscall (SYS_UNLINK = 10)
    let mut buf = [0u8; 128];
    let bytes = args.argv[1].as_bytes();
    buf[..bytes.len()].copy_from_slice(bytes);
    buf[bytes.len()] = 0;
    syscall3(10, buf.as_ptr() as u32, 0, 0);
}

fn cmd_echo(args: &Args) {
    for i in 1..args.argc {
        if i > 1 { print(" "); }
        print(args.argv[i]);
    }
    println("");
}

fn cmd_exec(args: &Args) {
    // Try to exec an ELF from the filesystem
    let pid = sys_fork();
    if pid == 0 {
        // Child: exec
        let mut argv_buf = [0u32; 32];
        let mut str_buf = [0u8; 256];
        let mut off = 0usize;
        let mut ai = 0usize;
        for i in 0..args.argc {
            let s = args.argv[i].as_bytes();
            if off + s.len() + 1 > str_buf.len() { break; }
            str_buf[off..off + s.len()].copy_from_slice(s);
            str_buf[off + s.len()] = 0;
            argv_buf[ai] = str_buf.as_ptr() as u32 + off as u32;
            off += s.len() + 1;
            ai += 1;
        }
        argv_buf[ai] = 0;
        sys_execve(args.argv[0], argv_buf.as_ptr() as u32, 0);
        // If exec fails
        println("exec: command not found");
        sys_exit(1);
    } else if pid > 0 {
        // Parent: wait
        let mut status = 0u32;
        sys_waitpid(pid, &mut status);
    } else {
        println("fork failed");
    }
}

fn cmd_ps() {
    println("PID 1: kernel shell");
    println("PID 2: init.elf (this shell)");
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut buf = [0u8; 256];

    loop {
        print("~ ☯ ");
        let line = match read_line(&mut buf) {
            Some(l) => l,
            None => continue,
        };

        let args = parse_args(line.trim());
        if args.argc == 0 { continue; }

        match args.argv[0] {
            "echo" => cmd_echo(&args),
            "help" => {
                println("Mizu init shell");
                println("  echo, ls, cat, cp, rm, ps, help, clear, exit");
                println("  Other words: exec ELF from filesystem");
            }
            "clear" => { print("\x1b[2J\x1b[H"); }
            "exit" | "halt" => {
                println("exiting init...");
                sys_exit(0);
            }
            "ls" => cmd_ls(),
            "cat" => cmd_cat(&args),
            "cp" => cmd_cp(&args),
            "rm" => cmd_rm(&args),
            "ps" => cmd_ps(),
            _ => cmd_exec(&args),
        }
    }
}
