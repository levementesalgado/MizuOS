use alloc::string::String;
use alloc::vec::Vec;
use crate::arch::i686::gdt::TSS;
use crate::arch::i686::paging::PageDir;
use crate::interrupts::SYSCALL_ESP;
use crate::memory;
use crate::serial_driver;
use crate::syscall;

const MAX_PROCS: usize = 256;
const KSTACK_PAGES: usize = 2;
const KSTACK_SIZE: u32 = KSTACK_PAGES as u32 * 4096;

// ─── File descriptors + pipes ──────────────────────────
const MAX_FDS: usize = 32;
const MAX_PIPES: usize = 64;
const PIPE_SIZE: usize = 4096;
const MAX_OPENED: usize = 64;

#[derive(Clone, Copy, PartialEq)]
enum FdType {
    None,
    Stdin,
    Stdout,
    Stderr,
    File,
    PipeReader(u16),
    PipeWriter(u16),
}

#[derive(Clone, Copy)]
struct FdEntry {
    ftype: FdType,
    cloexec: bool,
    file_offset: u32,
    opened_idx: u16,
}

#[derive(Clone)]
struct OpenedFile {
    name: String,
}

static mut OPENED_FILES: Option<Vec<Option<OpenedFile>>> = None;
const OPENED_NONE: u16 = u16::MAX;


#[derive(Clone, Copy)]
struct PipeBufCtl {
    in_use: bool,
    start: u32,
    len: u32,
    readers: u32,
    writers: u32,
}

fn opened_init() {
    unsafe {
        if OPENED_FILES.is_none() {
            let mut v = Vec::with_capacity(MAX_OPENED);
            for _ in 0..MAX_OPENED { v.push(None); }
            OPENED_FILES = Some(v);
        }
    }
}

fn opened_alloc(name: &str) -> u16 {
    opened_init();
    unsafe {
        let table = OPENED_FILES.as_mut().unwrap();
        for i in 0..table.len() {
            if table[i].is_none() {
                table[i] = Some(OpenedFile { name: String::from(name) });
                return i as u16;
            }
        }
    }
    OPENED_NONE
}

fn opened_free(idx: u16) {
    if idx == OPENED_NONE { return; }
    unsafe {
        if let Some(ref mut table) = OPENED_FILES {
            if (idx as usize) < table.len() {
                table[idx as usize] = None;
            }
        }
    }
}

fn opened_get_name(idx: u16) -> Option<String> {
    if idx == OPENED_NONE { return None; }
    unsafe {
        if let Some(ref table) = OPENED_FILES {
            if (idx as usize) < table.len() {
                if let Some(ref entry) = table[idx as usize] {
                    return Some(entry.name.clone());
                }
            }
        }
    }
    None
}

fn opened_set_name(idx: u16, name: &str) {
    if idx == OPENED_NONE { return; }
    unsafe {
        if let Some(ref mut table) = OPENED_FILES {
            if (idx as usize) < table.len() {
                if let Some(ref mut entry) = table[idx as usize] {
                    entry.name = String::from(name);
                }
            }
        }
    }
}

const FD_ZERO: FdEntry = FdEntry { ftype: FdType::None, cloexec: false, file_offset: 0, opened_idx: OPENED_NONE };
static mut FDTABLE: [[FdEntry; MAX_FDS]; MAX_PROCS] = [[FD_ZERO; MAX_FDS]; MAX_PROCS];
static mut PIPE_CTL: [PipeBufCtl; MAX_PIPES] = [PipeBufCtl { in_use: false, start: 0, len: 0, readers: 0, writers: 0 }; MAX_PIPES];
static mut PIPE_DATA: [u8; MAX_PIPES * PIPE_SIZE] = [0; MAX_PIPES * PIPE_SIZE];

unsafe fn set_fd(pid: u32, fd: u32, ftype: FdType, cloexec: bool) {
    if (fd as usize) >= MAX_FDS { return; }
    let old = FDTABLE[pid as usize][fd as usize];
    if old.ftype == FdType::File { opened_free(old.opened_idx); }
    FDTABLE[pid as usize][fd as usize] = FdEntry { ftype, cloexec, file_offset: 0, opened_idx: OPENED_NONE };
}

unsafe fn get_fd(pid: u32, fd: u32) -> FdEntry {
    if (fd as usize) >= MAX_FDS { return FD_ZERO; }
    FDTABLE[pid as usize][fd as usize]
}

// Find lowest free FD for current process
unsafe fn alloc_fd_for(pid: u32, start: u32) -> Option<u32> {
    for fd in start..MAX_FDS as u32 {
        if get_fd(pid, fd).ftype == FdType::None {
            return Some(fd);
        }
    }
    None
}

fn alloc_fd() -> Option<u32> {
    unsafe { alloc_fd_for(CURRENT_PID, 0) }
}

unsafe fn close_fd_for(pid: u32, fd: u32) {
    if (fd as usize) >= MAX_FDS { return; }
    let entry = FDTABLE[pid as usize][fd as usize];
    match entry.ftype {
        FdType::File => { opened_free(entry.opened_idx); }
        FdType::PipeReader(id) | FdType::PipeWriter(id) => {
            let ctl = &mut PIPE_CTL[id as usize];
            if matches!(entry.ftype, FdType::PipeReader(_)) {
                ctl.readers = ctl.readers.saturating_sub(1);
            } else {
                ctl.writers = ctl.writers.saturating_sub(1);
            }
            if ctl.readers == 0 && ctl.writers == 0 {
                ctl.in_use = false;
            }
        }
        _ => {}
    }
    FDTABLE[pid as usize][fd as usize] = FD_ZERO;
}

unsafe fn alloc_pipe() -> Option<u16> {
    for id in 0..MAX_PIPES as u16 {
        if !PIPE_CTL[id as usize].in_use {
            let ctl = &mut PIPE_CTL[id as usize];
            ctl.in_use = true;
            ctl.start = 0;
            ctl.len = 0;
            ctl.readers = 1;
            ctl.writers = 1;
            let base = (id as usize) * PIPE_SIZE;
            for i in 0..PIPE_SIZE { PIPE_DATA[base + i] = 0; }
            return Some(id);
        }
    }
    None
}

unsafe fn pipe_read(id: u16, buf: u32, len: u32) -> u32 {
    let ctl = &mut PIPE_CTL[id as usize];
    let max_read = core::cmp::min(len, ctl.len);
    if max_read == 0 { return 0; }
    let base = (id as usize) * PIPE_SIZE;
    let start = ctl.start as usize;
    let p = buf as *mut u8;
    for i in 0..max_read as usize {
        let src = (start + i) % PIPE_SIZE;
        p.add(i).write(PIPE_DATA[base + src]);
    }
    ctl.start = (ctl.start + max_read) % PIPE_SIZE as u32;
    ctl.len -= max_read;
    max_read
}

unsafe fn pipe_write(id: u16, buf: u32, len: u32) -> i32 {
    let ctl = &mut PIPE_CTL[id as usize];
    if ctl.readers == 0 { return -syscall::EPIPE; }
    let space = PIPE_SIZE as u32 - ctl.len;
    if space == 0 { return 0; } // buffer full, no blocking
    let n = core::cmp::min(len, space) as usize;
    let base = (id as usize) * PIPE_SIZE;
    let start = (ctl.start + ctl.len) as usize % PIPE_SIZE;
    let p = buf as *const u8;
    for i in 0..n {
        let dst = (start + i) % PIPE_SIZE;
        PIPE_DATA[base + dst] = p.add(i).read();
    }
    ctl.len += n as u32;
    n as i32
}

// ─── End FD definitions ────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum State {
    Free,
    Running,
    Ready,
    Blocked,
    Zombie,
}

#[derive(Clone, Copy)]
struct SignalAction {
    handler: u32,
    mask: u64,
    flags: u32,
    restorer: u32,
}

#[derive(Clone, Copy)]
pub struct Process {
    state: State,
    pid: u32,
    ppid: u32,
    exit_code: u32,
    uid: u32,
    gid: u32,
    euid: u32,
    egid: u32,
    pd: u32,
    kstack_base: u32,
    saved_esp: u32,
    sigactions: [SignalAction; 32],
    sigpending: u32,
    sigmask: u64,
    pgid: u32,
    sid: u32,
    ctty: u32,
    brk: u32,
    mmap_base: u32,
    cwd: [u8; 256],
    cwd_len: u16,
}

const EMPTY_SIGACTION: SignalAction = SignalAction {
    handler: 0,
    mask: 0,
    flags: 0,
    restorer: 0,
};

static mut PPROCS: [Process; MAX_PROCS] = [Process {
    state: State::Free,
    pid: 0,
    ppid: 0,
    exit_code: 0,
    uid: 0,
    gid: 0,
    euid: 0,
    egid: 0,
    pd: 0,
    kstack_base: 0,
    saved_esp: 0,
    sigactions: [EMPTY_SIGACTION; 32],
    sigpending: 0,
    sigmask: 0,
    pgid: 0,
    sid: 0,
    ctty: 0,
    brk: 0,
    mmap_base: 0x40000000,
    cwd: [0; 256],
    cwd_len: 0,
}; MAX_PROCS];

pub static mut CURRENT_PID: u32 = 0;
static mut NEXT_PID: u32 = 1;

pub fn init(kpd: u32) {
    unsafe {
        for i in 0..MAX_PROCS {
            PPROCS[i] = Process {
                state: State::Free,
                pid: 0,
                ppid: 0,
                exit_code: 0,
                uid: 0,
                gid: 0,
                euid: 0,
                egid: 0,
                pd: 0,
                kstack_base: 0,
                saved_esp: 0,
                sigactions: [EMPTY_SIGACTION; 32],
                sigpending: 0,
                sigmask: 0,
                pgid: 0,
                sid: 0,
                ctty: 0,
                brk: 0,
                mmap_base: 0x40000000,
                cwd: [0; 256],
                cwd_len: 0,
            };
        }
        CURRENT_PID = 1;
        let p = &mut PPROCS[1];
        p.state = State::Running;
        p.pid = 1;
        p.ppid = 0;
        p.pd = kpd;
        p.uid = 1000;
        p.gid = 1000;
        p.euid = 1000;
        p.egid = 1000;
        p.pgid = 1;
        p.sid = 1;
        p.ctty = 0;
        p.brk = 0x40000000;
        p.cwd[0] = b'/';
        p.cwd_len = 1;
        NEXT_PID = 2;

        // Set up stdin/stdout/stderr for PID 1
        set_fd(1, 0, FdType::Stdin, false);
        set_fd(1, 1, FdType::Stdout, false);
        set_fd(1, 2, FdType::Stderr, false);
    }
    serial_driver::sprintln!("  process: init (PID 1) pd={:#x}", kpd);
}

fn alloc_pid() -> Option<u32> {
    unsafe {
        for i in 1..MAX_PROCS {
            if PPROCS[i].state as u8 == State::Free as u8 {
                return Some(i as u32);
            }
        }
    }
    None
}

fn alloc_kstack() -> Option<u32> {
    // Scan from end of kernel up through the identity-mapped range.
    // We map 12MB (0 - 0xBFFFFF), so the last allocatable frame is 3070
    // (frame 3071 would make TOP past 12MB).
    // This returns the TOP address of an 8KB two-page stack region.
    let end_frame = 3071usize.min(memory::total_frames()); // within 12MB range
    let start_frame = (memory::kernel_end_frame() + 1).max(257) as usize;
    for frame in (start_frame..end_frame).rev() {
        let addr = frame * 4096;
        if !memory::alloc_frame_at(addr) { continue; }
        if !memory::alloc_frame_at(addr + 4096) {
            memory::free_frame(addr);
            continue;
        }
        let top = (addr + KSTACK_SIZE as usize) as u32;
        return Some(top);
    }
    None
}

unsafe fn read_user_string(ptr: u32) -> String {
    let mut s = String::new();
    let mut p = ptr as *const u8;
    loop {
        let c = p.read();
        if c == 0 { break; }
        s.push(c as char);
        p = p.add(1);
    }
    s
}

// Called from interrupts.rs - reads a string from user space
pub unsafe fn raw_read_string(ptr: u32) -> String {
    read_user_string(ptr)
}

// ─── Syscall implementations ────────────────────────────

pub fn sys_getpid() -> u32 {
    unsafe { CURRENT_PID }
}

pub fn sys_getppid() -> u32 {
    unsafe { PPROCS[CURRENT_PID as usize].ppid }
}

pub fn sys_getuid() -> u32 {
    unsafe { PPROCS[CURRENT_PID as usize].uid }
}

pub fn sys_geteuid() -> u32 {
    unsafe { PPROCS[CURRENT_PID as usize].euid }
}

pub fn sys_getgid() -> u32 {
    unsafe { PPROCS[CURRENT_PID as usize].gid }
}

pub fn sys_getegid() -> u32 {
    unsafe { PPROCS[CURRENT_PID as usize].egid }
}

pub fn sys_getpgrp() -> u32 {
    unsafe { PPROCS[CURRENT_PID as usize].pgid }
}

pub fn sys_getsid(pid: u32) -> u32 {
    unsafe {
        if pid == 0 {
            PPROCS[CURRENT_PID as usize].sid
        } else if (pid as usize) < MAX_PROCS && PPROCS[pid as usize].state as u8 != State::Free as u8 {
            PPROCS[pid as usize].sid
        } else {
            0
        }
    }
}

pub fn sys_setsid() -> u32 {
    unsafe {
        let p = CURRENT_PID as usize;
        PPROCS[p].sid = PPROCS[p].pid;
        PPROCS[p].pgid = PPROCS[p].pid;
        PPROCS[p].pid
    }
}

pub fn sys_setpgid(pid: u32, pgid: u32) -> i32 {
    unsafe {
        let p = if pid == 0 { CURRENT_PID as usize } else { pid as usize };
        if p >= MAX_PROCS || PPROCS[p].state as u8 == State::Free as u8 {
            return -syscall::ESRCH;
        }
        PPROCS[p].pgid = if pgid == 0 { PPROCS[p].pid } else { pgid };
        0
    }
}

pub fn sys_brk(addr: u32) -> u32 {
    unsafe {
        let p = &mut PPROCS[CURRENT_PID as usize];
        if addr == 0 { return p.brk; }
        if addr <= p.brk { p.brk = addr; return addr; }
        let old_end = (p.brk + 0xFFF) & !0xFFF;
        let new_end = (addr + 0xFFF) & !0xFFF;
        if new_end > old_end {
            let kpd = p.pd;
            let saved_cr3: u32;
            core::arch::asm!("mov {}, cr3", out(reg) saved_cr3);
            core::arch::asm!("mov cr3, {pd}", pd = in(reg) kpd);
            let mut page = old_end;
            while page < new_end {
                let frame = match memory::alloc_frame() {
                    Some(f) => f,
                    None => { core::arch::asm!("mov cr3, {pd}", pd = in(reg) saved_cr3); return p.brk; }
                };
                let mut pd_obj = PageDir::from_phys(kpd);
                pd_obj.map(page as usize, frame, true);
                page += 4096;
            }
            core::arch::asm!("mov cr3, {pd}", pd = in(reg) saved_cr3);
        }
        p.brk = addr;
        addr
    }
}

pub fn sys_uname(buf: u32) -> i32 {
    let sysname = b"MizuOS";
    let nodename = b"mizu";
    let release = b"0.1.0";
    let version = b"#1 Tue Jun 17 2026";
    let machine = b"i686";
    let domain = b"(none)";
    unsafe {
        let p = buf as *mut u8;
        let lens = [sysname.len(), nodename.len(), release.len(), version.len(), machine.len(), domain.len()];
        let strs: [&[u8]; 6] = [sysname, nodename, release, version, machine, domain];
        for field in 0..6 {
            let offset = field * 65;
            for i in 0..65 {
                p.add(offset + i).write(if i < lens[field] { strs[field][i] } else { 0 });
            }
        }
    }
    0
}

pub fn sys_clock_gettime(_clk_id: u32, tp: u32) -> i32 {
    if tp == 0 { return -syscall::EFAULT; }
    unsafe {
        let ptr = tp as *mut u32;
        ptr.write(1740000000);
        ptr.add(1).write(0);
    }
    0
}

pub fn sys_nanosleep(_req: u32, _rem: u32) -> i32 {
    0
}

pub fn sys_alarm(_seconds: u32) -> u32 {
    0
}

pub fn sys_sched_yield() -> i32 {
    0
}

pub fn sys_exit(code: u32) -> ! {
    unsafe {
        let p = CURRENT_PID as usize;
        let ppid = PPROCS[p].ppid as usize;
        PPROCS[p].state = State::Zombie;
        PPROCS[p].exit_code = code;
        // Send SIGCHLD to parent
        if ppid < MAX_PROCS && ppid != p {
            PPROCS[ppid].sigpending |= 1 << syscall::SIGCHLD;
            if PPROCS[ppid].state as u8 == State::Blocked as u8 {
                PPROCS[ppid].state = State::Ready;
            }
        }
        serial_driver::sprintln!("  exit: PID {} exit({})", CURRENT_PID, code);
    }
    // Find next process and switch
    switch_to_next()
}

pub fn sys_exit_group(code: u32) -> ! {
    sys_exit(code)
}

pub fn sys_kill(pid: u32, sig: u32) -> i32 {
    unsafe {
        if (pid as usize) >= MAX_PROCS { return -syscall::ESRCH; }
        if PPROCS[pid as usize].state as u8 == State::Free as u8 { return -syscall::ESRCH; }
        if sig == 0 { return 0; }
        if sig >= 32 { return -syscall::EINVAL; }
        PPROCS[pid as usize].sigpending |= 1 << sig;
        0
    }
}

pub fn sys_tgkill(_tgid: u32, pid: u32, sig: u32) -> i32 {
    sys_kill(pid, sig)
}

pub fn sys_getpgid(pid: u32) -> i32 {
    unsafe {
        if pid == 0 { return PPROCS[CURRENT_PID as usize].pgid as i32; }
        if (pid as usize) >= MAX_PROCS || PPROCS[pid as usize].state as u8 == State::Free as u8 {
            return -syscall::ESRCH;
        }
        PPROCS[pid as usize].pgid as i32
    }
}

pub fn sys_sigaction(signum: u32, act: u32, oldact: u32) -> i32 {
    unsafe {
        let p = CURRENT_PID as usize;
        if signum >= 32 || signum == syscall::SIGKILL || signum == syscall::SIGSTOP {
            return -syscall::EINVAL;
        }
        if oldact != 0 {
            let oa = oldact as *mut u32;
            oa.write(PPROCS[p].sigactions[signum as usize].handler);
            oa.add(1).write((PPROCS[p].sigactions[signum as usize].mask & 0xFFFFFFFF) as u32);
            oa.add(2).write((PPROCS[p].sigactions[signum as usize].mask >> 32) as u32);
            oa.add(3).write(PPROCS[p].sigactions[signum as usize].flags);
            oa.add(4).write(PPROCS[p].sigactions[signum as usize].restorer);
        }
        if act != 0 {
            let a = act as *const u32;
            PPROCS[p].sigactions[signum as usize].handler = a.read();
            let mask_lo = a.add(1).read();
            let mask_hi = a.add(2).read();
            PPROCS[p].sigactions[signum as usize].mask = (mask_hi as u64) << 32 | mask_lo as u64;
            PPROCS[p].sigactions[signum as usize].flags = a.add(3).read();
            PPROCS[p].sigactions[signum as usize].restorer = a.add(4).read();
        }
    }
    0
}

pub fn sys_sigprocmask(how: u32, set: u32, oldset: u32) -> i32 {
    unsafe {
        let p = CURRENT_PID as usize;
        if oldset != 0 {
            let os = oldset as *mut u32;
            os.write((PPROCS[p].sigmask & 0xFFFFFFFF) as u32);
            os.add(1).write((PPROCS[p].sigmask >> 32) as u32);
        }
        if set != 0 {
            let s = set as *const u32;
            let mask_lo = s.read();
            let mask_hi = s.add(1).read();
            let mask = (mask_hi as u64) << 32 | mask_lo as u64;
            match how {
                0 => PPROCS[p].sigmask |= mask,
                1 => PPROCS[p].sigmask &= !mask,
                2 => PPROCS[p].sigmask = mask,
                _ => return -syscall::EINVAL,
            }
        }
    }
    0
}

pub fn sys_sigpending(set: u32) -> i32 {
    unsafe {
        let p = CURRENT_PID as usize;
        if set != 0 {
            let s = set as *mut u32;
            s.write(PPROCS[p].sigpending);
            s.add(1).write(0);
        }
    }
    0
}

pub fn sys_sigsuspend(_mask: u32) -> i32 {
    -syscall::EINTR
}

pub fn sys_sigaltstack(_ss: u32, _old_ss: u32) -> i32 {
    0
}

// ─── Signal delivery ────────────────────────────────

fn is_sig_ignored(sig: u32) -> bool {
    unsafe {
        let p = CURRENT_PID as usize;
        let act = &PPROCS[p].sigactions[sig as usize];
        if act.handler == 1 { return true; }  // SIG_IGN
        if act.handler == 0 {                 // SIG_DFL
            // Signals whose default action is ignore
            return matches!(sig,
                syscall::SIGCHLD | syscall::SIGURG | syscall::SIGWINCH |
                syscall::SIGCONT | syscall::SIGSTOP | syscall::SIGTSTP |
                syscall::SIGTTIN | syscall::SIGTTOU
            );
        }
        // Has a user handler — skip delivery for now
        true
    }
}

fn sig_default_terminate(sig: u32) -> bool {
    unsafe {
        let p = CURRENT_PID as usize;
        let act = &PPROCS[p].sigactions[sig as usize];
        if act.handler != 0 { return false; }
        !matches!(sig,
            syscall::SIGCHLD | syscall::SIGURG | syscall::SIGWINCH |
            syscall::SIGCONT | syscall::SIGSTOP | syscall::SIGTSTP |
            syscall::SIGTTIN | syscall::SIGTTOU
        )
    }
}

pub fn check_signals() {
    unsafe {
        let p = CURRENT_PID as usize;
        let pending = PPROCS[p].sigpending;
        let blocked = PPROCS[p].sigmask as u32;
        let deliverable = pending & !blocked;

        if deliverable == 0 { return; }

        for sig in 1..32u32 {
            if deliverable & (1 << sig) == 0 { continue; }
            if is_sig_ignored(sig) {
                PPROCS[p].sigpending &= !(1 << sig);
                continue;
            }
            if sig_default_terminate(sig) {
                serial_driver::sprintln!("  signal {}: terminating PID {}", sig, CURRENT_PID);
                PPROCS[p].sigpending = 0;
                PPROCS[p].state = State::Zombie;
                PPROCS[p].exit_code = 128 + sig;
                let ppid = PPROCS[p].ppid as usize;
                if ppid < MAX_PROCS && ppid != p {
                    PPROCS[ppid].sigpending |= 1 << syscall::SIGCHLD;
                    if PPROCS[ppid].state as u8 == State::Blocked as u8 {
                        PPROCS[ppid].state = State::Ready;
                    }
                }
                return;
            }
        }
    }
}

pub fn sys_sigreturn() -> i32 {
    // For now, just ignore — bash doesn't need user handlers to function
    0
}

pub fn sys_getcwd(buf: u32, size: u32) -> i32 {
    unsafe {
        let p = CURRENT_PID as usize;
        let len = PPROCS[p].cwd_len as usize;
        if len >= size as usize { return -syscall::ERANGE; }
        for i in 0..len {
            (buf as *mut u8).add(i).write(PPROCS[p].cwd[i]);
        }
        (buf as *mut u8).add(len).write(0);
        len as i32
    }
}

pub fn sys_chdir(path: u32) -> i32 {
    unsafe {
        let first = *(path as *const u8);
        if first == b'/' {
            let p = CURRENT_PID as usize;
            PPROCS[p].cwd[0] = b'/';
            PPROCS[p].cwd_len = 1;
            return 0;
        }
    }
    -syscall::ENOENT
}

pub fn sys_getdents(fd: u32, dent: u32, count: u32) -> i32 {
    crate::fs::sys_getdents(fd, dent, count)
}

pub fn sys_getdents64(fd: u32, dent: u32, count: u32) -> i32 {
    sys_getdents(fd, dent, count)
}

pub fn sys_access(path: u32, _mode: u32) -> i32 {
    unsafe {
        let first = *(path as *const u8);
        if first == b'/' { return 0; }
    }
    -syscall::ENOENT
}

pub fn sys_readlink(_path: u32, _buf: u32, _bufsiz: u32) -> i32 {
    -syscall::EINVAL
}

pub fn sys_pipe(fds: u32) -> i32 {
    unsafe {
        let pipe_id = match alloc_pipe() {
            Some(id) => id,
            None => return -syscall::ENFILE,
        };
        let read_fd = match alloc_fd() {
            Some(fd) => fd,
            None => { PIPE_CTL[pipe_id as usize].in_use = false; return -syscall::ENFILE; }
        };
        let write_fd = match alloc_fd() {
            Some(fd) => fd,
            None => {
                PIPE_CTL[pipe_id as usize].in_use = false;
                close_fd_for(CURRENT_PID, read_fd);
                return -syscall::ENFILE;
            }
        };
        let pid = CURRENT_PID;
        set_fd(pid, read_fd, FdType::PipeReader(pipe_id), false);
        set_fd(pid, write_fd, FdType::PipeWriter(pipe_id), false);

        // Write fds to user space
        let p = fds as *mut u32;
        p.write(read_fd);
        p.add(1).write(write_fd);
    }
    0
}

pub fn sys_pipe2(fds: u32, flags: u32) -> i32 {
    unsafe {
        let pipe_id = match alloc_pipe() {
            Some(id) => id,
            None => return -syscall::ENFILE,
        };
        let read_fd = match alloc_fd() {
            Some(fd) => fd,
            None => { PIPE_CTL[pipe_id as usize].in_use = false; return -syscall::ENFILE; }
        };
        let write_fd = match alloc_fd() {
            Some(fd) => fd,
            None => {
                PIPE_CTL[pipe_id as usize].in_use = false;
                close_fd_for(CURRENT_PID, read_fd);
                return -syscall::ENFILE;
            }
        };
        let pid = CURRENT_PID;
        let cloexec = (flags & syscall::O_CLOEXEC) != 0;
        set_fd(pid, read_fd, FdType::PipeReader(pipe_id), cloexec);
        set_fd(pid, write_fd, FdType::PipeWriter(pipe_id), cloexec);

        let p = fds as *mut u32;
        p.write(read_fd);
        p.add(1).write(write_fd);
    }
    0
}

pub fn sys_dup(oldfd: u32) -> i32 {
    unsafe {
        let pid = CURRENT_PID;
        let entry = get_fd(pid, oldfd);
        if matches!(entry.ftype, FdType::None) { return -syscall::EBADF; }
        let newfd = match alloc_fd_for(pid, 0) {
            Some(fd) => fd,
            None => return -syscall::EMFILE,
        };
        // dup doesn't set FD_CLOEXEC
        FDTABLE[pid as usize][newfd as usize] = FdEntry {
            ftype: entry.ftype, cloexec: false,
            file_offset: entry.file_offset,
            opened_idx: entry.opened_idx,
        };
        match entry.ftype {
            FdType::PipeReader(id) => PIPE_CTL[id as usize].readers += 1,
            FdType::PipeWriter(id) => PIPE_CTL[id as usize].writers += 1,
            _ => {}
        }
        newfd as i32
    }
}

pub fn sys_dup2(oldfd: u32, newfd: u32) -> i32 {
    unsafe {
        if oldfd == newfd { return newfd as i32; }
        let pid = CURRENT_PID;
        let entry = get_fd(pid, oldfd);
        if matches!(entry.ftype, FdType::None) { return -syscall::EBADF; }
        if (newfd as usize) >= MAX_FDS { return -syscall::EBADF; }

        // Close newfd if open
        let newentry = get_fd(pid, newfd);
        if !matches!(newentry.ftype, FdType::None) {
            close_fd_for(pid, newfd);
        }

        // Copy entry (dup2 clears FD_CLOEXEC)
        FDTABLE[pid as usize][newfd as usize] = FdEntry {
            ftype: entry.ftype, cloexec: false,
            file_offset: entry.file_offset,
            opened_idx: entry.opened_idx,
        };
        match entry.ftype {
            FdType::PipeReader(id) => PIPE_CTL[id as usize].readers += 1,
            FdType::PipeWriter(id) => PIPE_CTL[id as usize].writers += 1,
            _ => {}
        }
        newfd as i32
    }
}

pub fn sys_fcntl(fd: u32, cmd: u32, arg: u32) -> i32 {
    unsafe {
        let pid = CURRENT_PID;
        if (fd as usize) >= MAX_FDS { return -syscall::EBADF; }
        let entry = &mut FDTABLE[pid as usize][fd as usize];
        if matches!(entry.ftype, FdType::None) { return -syscall::EBADF; }
        match cmd as i32 {
            syscall::F_GETFD => if entry.cloexec { syscall::FD_CLOEXEC as i32 } else { 0 },
            syscall::F_SETFD => { entry.cloexec = (arg & syscall::FD_CLOEXEC as u32) != 0; 0 }
            syscall::F_GETFL => 2, // O_RDWR
            syscall::F_SETFL => 0,
            syscall::F_DUPFD => {
                let start = core::cmp::max(arg, fd + 1);
                let newfd = match alloc_fd_for(pid, start) {
                    Some(f) => f,
                    None => return -syscall::EMFILE,
                };
                FDTABLE[pid as usize][newfd as usize] = FdEntry {
                    ftype: entry.ftype, cloexec: true,
                    file_offset: entry.file_offset,
                    opened_idx: entry.opened_idx,
                };
                match entry.ftype {
                    FdType::PipeReader(id) => PIPE_CTL[id as usize].readers += 1,
                    FdType::PipeWriter(id) => PIPE_CTL[id as usize].writers += 1,
                    _ => {}
                }
                newfd as i32
            }
            _ => 0,
        }
    }
}

pub fn sys_fcntl64(fd: u32, cmd: u32, arg: u32) -> i32 {
    sys_fcntl(fd, cmd, arg)
}

pub fn sys_ioctl(_fd: u32, request: u32, argp: u32) -> i32 {
    match request {
        syscall::TCGETS => {
            if argp == 0 { return 0; }
            unsafe {
                let p = argp as *mut u32;
                p.add(0).write(syscall::IGNBRK | syscall::BRKINT | syscall::ICRNL);
                p.add(1).write(syscall::OPOST);
                p.add(2).write(syscall::CS8 | syscall::CREAD);
                p.add(3).write(syscall::ISIG | syscall::ICANON | syscall::ECHO | syscall::ECHOE | syscall::ECHOK);
                p.add(4).write(0);
                p.add(5).write(0);
                for i in 0..17 { p.add(8 + i).write(0); }
                *(argp as *mut u8).add(syscall::VMIN) = 1;
                *(argp as *mut u8).add(syscall::VTIME) = 0;
                *(argp as *mut u8).add(syscall::VINTR) = 3;
                *(argp as *mut u8).add(syscall::VQUIT) = 28;
                *(argp as *mut u8).add(syscall::VERASE) = 127;
                *(argp as *mut u8).add(syscall::VKILL) = 21;
                *(argp as *mut u8).add(syscall::VEOF) = 4;
                *(argp as *mut u8).add(syscall::VSTART) = 17;
                *(argp as *mut u8).add(syscall::VSTOP) = 19;
                *(argp as *mut u8).add(syscall::VSUSP) = 26;
            }
            0
        }
        syscall::TCSETSW | syscall::TCSETS | syscall::TCSETSF => 0,
        syscall::TIOCGWINSZ => {
            if argp != 0 {
                unsafe {
                    let p = argp as *mut u16;
                    p.add(0).write(25);
                    p.add(1).write(80);
                    p.add(2).write(0);
                    p.add(3).write(0);
                }
            }
            0
        }
        syscall::TIOCGPGRP => {
            unsafe {
                if argp != 0 {
                    (argp as *mut i32).write(PPROCS[CURRENT_PID as usize].pgid as i32);
                }
            }
            0
        }
        syscall::TIOCSPGRP => 0,
        syscall::TIOCSCTTY => 0,
        syscall::TIOCNOTTY => 0,
        syscall::FIONREAD => {
            if argp != 0 { unsafe { (argp as *mut i32).write(0); } }
            0
        }
        _ => -syscall::ENOTTY,
    }
}

pub fn sys_isatty(fd: u32) -> i32 {
    if fd <= 2 { 1 } else { 0 }
}

pub fn sys_unlink(_pathname: u32) -> i32 {
    -syscall::ENOSYS
}

pub fn sys_mkdir(_pathname: u32, _mode: u32) -> i32 {
    -syscall::ENOSYS
}

pub fn sys_rmdir(_pathname: u32) -> i32 {
    -syscall::ENOSYS
}

pub fn sys_rename(_oldpath: u32, _newpath: u32) -> i32 {
    -syscall::ENOSYS
}

pub fn sys_symlink(_target: u32, _linkpath: u32) -> i32 {
    -syscall::ENOSYS
}

pub fn sys_chmod(_pathname: u32, _mode: u32) -> i32 {
    0
}

pub fn sys_chown(_pathname: u32, _owner: u32, _group: u32) -> i32 {
    0
}

pub fn sys_utimensat(_dirfd: u32, _pathname: u32, _times: u32, _flags: u32) -> i32 {
    0
}

pub fn sys_getrandom(buf: u32, len: u32, _flags: u32) -> i32 {
    unsafe {
        let n = if len > 256 { 256 } else { len as usize };
        for i in 0..n {
            (buf as *mut u8).add(i).write((i ^ 0xAA) as u8);
        }
        n as i32
    }
}

pub fn sys_syslog(_ty: u32, _buf: u32, _len: u32) -> i32 {
    -syscall::ENOSYS
}

pub fn sys_prlimit64(_pid: u32, _resource: u32, _new_rlim: u32, old_rlim: u32) -> i32 {
    if old_rlim != 0 {
        unsafe {
            let p = old_rlim as *mut u64;
            p.write(0xFFFFFFFF);
            p.add(1).write(0xFFFFFFFF);
        }
    }
    0
}

pub fn sys_getrlimit(_resource: u32, rlim: u32) -> i32 {
    if rlim != 0 {
        unsafe {
            let p = rlim as *mut u64;
            p.write(0xFFFFFFFF);
            p.add(1).write(0xFFFFFFFF);
        }
    }
    0
}

pub fn sys_setrlimit(_resource: u32, _rlim: u32) -> i32 {
    0
}

pub fn sys_time() -> u32 {
    1740000000
}

pub fn sys_umask(_mask: u32) -> u32 {
    0o22
}

pub fn sys_read(fd: u32, buf: u32, len: u32) -> u32 {
    unsafe {
        let entry = get_fd(CURRENT_PID, fd);
        match entry.ftype {
            FdType::Stdin => {
                for i in 0..len {
                    let c = loop {
                        if let Some(k) = crate::keyboard::get_key() { break k; }
                        if let Some(s) = crate::serial_driver::get_char() { break s; }
                    };
                    (buf as *mut u8).add(i as usize).write(c);
                    if c == b'\n' || c == b'\r' { return i + 1; }
                }
                len
            }
            FdType::PipeReader(id) => pipe_read(id, buf, len),
            FdType::File => {
                let name = match opened_get_name(entry.opened_idx) {
                    Some(n) => n,
                    None => return -syscall::EBADF as u32,
                };
                let pid = CURRENT_PID as usize;
                let off = FDTABLE[pid][fd as usize].file_offset as usize;
                if let Some(data) = crate::fs::file_read(&name) {
                    let avail = data.len().saturating_sub(off);
                    let copy_len = core::cmp::min(avail, len as usize);
                    if copy_len > 0 {
                        core::ptr::copy_nonoverlapping(
                            data.as_ptr().add(off),
                            buf as *mut u8,
                            copy_len,
                        );
                    }
                    FDTABLE[pid][fd as usize].file_offset = off as u32 + copy_len as u32;
                    copy_len as u32
                } else {
                    -syscall::ENOENT as u32
                }
            }
            _ => -syscall::EBADF as u32,
        }
    }
}

pub fn sys_write(fd: u32, buf: u32, len: u32) -> u32 {
    unsafe {
        let entry = get_fd(CURRENT_PID, fd);
        match entry.ftype {
            FdType::Stdout | FdType::Stderr => {
                let slice = core::slice::from_raw_parts(buf as *const u8, len as usize);
                let s = core::str::from_utf8(slice).unwrap_or("<bin>");
                crate::vga_driver::write_str(s);
                crate::serial_driver::write_str(s);
                len
            }
            FdType::PipeWriter(id) => pipe_write(id, buf, len) as u32,
            FdType::File => {
                let name = match opened_get_name(entry.opened_idx) {
                    Some(n) => n,
                    None => return -syscall::EBADF as u32,
                };
                let slice = core::slice::from_raw_parts(buf as *const u8, len as usize);
                if crate::fs::file_write(&name, slice) {
                    len
                } else {
                    -syscall::EIO as u32
                }
            }
            _ => -syscall::EBADF as u32,
        }
    }
}

// readv/writev sobre o mesmo caminho de sys_read/sys_write.
// Port conceitual do TipOS (syscall_linux.zig): o __stdio_read do musl
// usa readv sempre, sem ele todo fread falha. i386: 145=readv, 146=writev,
// ebx=fd, ecx=iov[], edx=iovcnt; iov = {iov_base:u32, iov_len:u32}.
pub fn sys_readv(fd: u32, iov: u32, iovcnt: u32) -> i32 {
    let mut total: u32 = 0;
    unsafe {
        let p = iov as *const u32;
        for i in 0..iovcnt {
            let base = p.add((i as usize) * 2).read();
            let len = p.add((i as usize) * 2 + 1).read();
            if len == 0 { continue; }
            let n = sys_read(fd, base, len);
            // sys_read retorna -errno como u32 alto; tratar como erro
            if (n as i32) < 0 { return n as i32; }
            total = total.wrapping_add(n);
            if n < len { break; }
        }
    }
    total as i32
}

pub fn sys_writev(fd: u32, iov: u32, iovcnt: u32) -> i32 {
    let mut total: u32 = 0;
    unsafe {
        let p = iov as *const u32;
        for i in 0..iovcnt {
            let base = p.add((i as usize) * 2).read();
            let len = p.add((i as usize) * 2 + 1).read();
            if len == 0 { continue; }
            let n = sys_write(fd, base, len);
            if (n as i32) < 0 { return n as i32; }
            total = total.wrapping_add(n);
            if n < len { break; }
        }
    }
    total as i32
}

pub fn sys_open(pathname: u32, flags: u32, _mode: u32) -> u32 {
    unsafe {
        let name = raw_read_string(pathname);
        if name.is_empty() { return -syscall::ENOENT as u32; }
        let pid = CURRENT_PID;
        if name == "/dev/tty" || name == "/dev/console" {
            return sys_dup2(1, 0) as u32;
        }
        if name == "/dev/null" || name == "/dev/zero" {
            let fd = match alloc_fd() { Some(f) => f, None => return -syscall::EMFILE as u32 };
            set_fd(pid, fd, if name == "/dev/null" { FdType::Stdout } else { FdType::Stdin }, false);
            return fd;
        }
        let exists = crate::fs::file_exists(&name);
        if exists || (flags & syscall::O_CREAT != 0) {
            let fd = match alloc_fd() { Some(f) => f, None => return -syscall::EMFILE as u32 };
            let opened_idx = opened_alloc(&name);
            FDTABLE[pid as usize][fd as usize] = FdEntry {
                ftype: FdType::File, cloexec: false,
                file_offset: 0, opened_idx,
            };
            if flags & syscall::O_TRUNC != 0 {
                crate::fs::file_write(&name, &[]);
            }
            return fd;
        }
    }
    -syscall::ENOENT as u32
}

pub fn sys_close(fd: u32) -> u32 {
    unsafe {
        let entry = get_fd(CURRENT_PID, fd);
        if matches!(entry.ftype, FdType::None) { return -syscall::EBADF as u32; }
        close_fd_for(CURRENT_PID, fd);
    }
    0
}

pub fn sys_lseek(fd: u32, offset: u32, whence: u32) -> i32 {
    unsafe {
        let pid = CURRENT_PID as usize;
        let entry = get_fd(CURRENT_PID, fd);
        match entry.ftype {
            FdType::File => {
                let name = match opened_get_name(entry.opened_idx) {
                    Some(n) => n,
                    None => return -syscall::EBADF as i32,
                };
                let data = crate::fs::file_read(&name);
                let file_size = data.map(|d| d.len()).unwrap_or(0);
                let new_off = match whence {
                    0 => offset,                                       // SEEK_SET
                    1 => FDTABLE[pid][fd as usize].file_offset.wrapping_add(offset), // SEEK_CUR
                    2 => (file_size as u32).wrapping_add(offset),      // SEEK_END
                    _ => return -syscall::EINVAL as i32,
                };
                let new_off = core::cmp::min(new_off, file_size as u32);
                FDTABLE[pid][fd as usize].file_offset = new_off;
                new_off as i32
            }
            _ => -syscall::ESPIPE as i32,
        }
    }
}

pub fn sys_setuid(uid: u32) -> i32 {
    unsafe { PPROCS[CURRENT_PID as usize].uid = uid; PPROCS[CURRENT_PID as usize].euid = uid; }
    0
}

pub fn sys_setgid(gid: u32) -> i32 {
    unsafe { PPROCS[CURRENT_PID as usize].gid = gid; PPROCS[CURRENT_PID as usize].egid = gid; }
    0
}

pub fn sys_mmap(_addr: u32, length: u32, _prot: u32, flags: u32, _fd: u32, _offset: u32) -> u32 {
    if flags & syscall::MAP_ANONYMOUS == 0 && _fd != 0xFFFFFFFF && _fd != 0 {
        return syscall::MAP_FAILED;
    }
    let pages = ((length + 0xFFF) / 0x1000) as usize;
    unsafe {
        let cur = CURRENT_PID as usize;
        let base = PPROCS[cur].mmap_base;
        let kpd = PPROCS[cur].pd;
        let saved_cr3: u32;
        core::arch::asm!("mov {}, cr3", out(reg) saved_cr3);
        core::arch::asm!("mov cr3, {pd}", pd = in(reg) kpd);

        let mut page = base;
        let mut pdobj = PageDir::from_phys(kpd);
        for _ in 0..pages {
            let frame = match memory::alloc_frame() {
                Some(f) => f,
                None => {
                    core::arch::asm!("mov cr3, {pd}", pd = in(reg) saved_cr3);
                    return syscall::MAP_FAILED;
                }
            };
            pdobj.map(page as usize, frame, true);
            page += 0x1000;
        }
        PPROCS[cur].mmap_base = page;
        core::arch::asm!("mov cr3, {pd}", pd = in(reg) saved_cr3);
        base
    }
}

pub fn sys_munmap(_addr: u32, _length: u32) -> i32 {
    0
}

pub fn sys_mprotect(_addr: u32, _len: u32, _prot: u32) -> i32 {
    0
}

// ─── Fork: create child with duplicated context ─────────

pub fn sys_fork() -> i32 {
    unsafe {
        let new_pid = match alloc_pid() {
            Some(p) => p,
            None => return -syscall::ENOMEM,
        };
        let cur = CURRENT_PID as usize;
        let child = new_pid as usize;
        serial_driver::sprintln!("  fork: trying PID {}, cur={}", new_pid, cur);


        // Copy process data
        PPROCS[child] = PPROCS[cur];
        PPROCS[child].pid = new_pid;
        PPROCS[child].state = State::Ready;
        PPROCS[child].ppid = CURRENT_PID;
        PPROCS[child].exit_code = 0;
        PPROCS[child].sigpending = 0;

        // Allocate kernel stack for child
        let kstack = match alloc_kstack() {
            Some(s) => s,
            None => {
                PPROCS[child].state = State::Free;
                return -syscall::ENOMEM;
            }
        };
        PPROCS[child].kstack_base = kstack - KSTACK_SIZE;

        // Copy page directory
        let old_pd_phys = PPROCS[cur].pd;
        let new_pd_phys = match memory::alloc_frame() {
            Some(f) => f as u32,
            None => {
                PPROCS[child].state = State::Free;
                return -syscall::ENOMEM;
            }
        };
        PPROCS[child].pd = new_pd_phys;

        // Copy PDEs and page tables for user pages
        // PDE 0 is the identity-mapped kernel (shared), PDEs 768-1023 are kernel (shared)
        // We need to deep-copy user PDEs (typically PDEs 1-767 for user memory)
        let old_pd = old_pd_phys as *const u32;
        let new_pd = new_pd_phys as *mut u32;
        for i in 0..1024 {
            let pde = old_pd.add(i).read();
            if pde & 1 == 0 {
                new_pd.add(i).write(0);
                continue;
            }
            // PDE 0 is kernel identity map (shared)
            if i == 0 {
                new_pd.add(i).write(pde);
                continue;
            }
            // Check if it's a user page directory (US bit set)
            if pde & 4 != 0 {
                // Deep copy this page table
                let pt_phys = (pde & 0xFFFFF000) as *const u32;
                let new_pt = match memory::alloc_frame() {
                    Some(f) => f as *mut u32,
                    None => {
                        PPROCS[child].state = State::Free;
                        return -syscall::ENOMEM;
                    }
                };
                for j in 0..1024 {
                    let pte = pt_phys.add(j).read();
                    new_pt.add(j).write(pte);
                    // For writable pages, we need to mark them read-only for COW
                    // but for simplicity, we just share the same physical pages
                }
                new_pd.add(i).write((new_pt as u32) | (pde & 0xFFF));
            } else {
                // Kernel PDE - share
                new_pd.add(i).write(pde);
            }
        }

        // Copy kernel stack: just the context frame (17 dwords = 68 bytes)
        // Context layout: gs, fs, es, ds, edi, esi, ebp, old_esp, ebx, edx, ecx, eax
        //                + EIP, CS, EFLAGS, user_ESP, SS  (CPU-pushed)
        let frame_size = 17 * 4; // 68 bytes
        let child_esp = kstack - frame_size as u32;
        core::ptr::copy_nonoverlapping(
            SYSCALL_ESP as *const u8,
            child_esp as *mut u8,
            frame_size,
        );
        // Set EAX to 0 (fork returns 0 in child)
        let child_eax_ptr = (child_esp + 44) as *mut u32;
        child_eax_ptr.write(0);

        PPROCS[child].saved_esp = child_esp;

        // Copy FD table and update pipe refcounts
        for fd in 0..MAX_FDS as u32 {
            let entry = get_fd(cur as u32, fd);
            FDTABLE[child][fd as usize] = entry;
            match entry.ftype {
                FdType::PipeReader(id) => PIPE_CTL[id as usize].readers += 1,
                FdType::PipeWriter(id) => PIPE_CTL[id as usize].writers += 1,
                _ => {}
            }
        }

        serial_driver::sprintln!("  fork: PID {} -> PID {}, child_esp={:#x}", CURRENT_PID, new_pid, child_esp);

        // Return child PID to parent
        new_pid as i32
    }
}

// ─── Execve: replace process image ──────────────────────

pub fn sys_execve(filename: u32, argv: u32, envp: u32) -> i32 {
    unsafe {
        let fname = read_user_string(filename);
        if fname.is_empty() { return -syscall::ENOENT; }

        serial_driver::sprintln!("  execve: '{}' (pid {})", fname, CURRENT_PID);

        let data = match crate::fs::file_read(&fname) {
            Some(d) => d,
            None => {
                serial_driver::sprintln!("  execve: file not found: {}", fname);
                return -syscall::ENOENT;
            }
        };

        match crate::personality::linux::exec_elf(data, argv, envp) {
            Ok((new_pd, entry, sp)) => {
                let cur = CURRENT_PID as usize;
                PPROCS[cur].pd = new_pd;
                PPROCS[cur].brk = 0x40000000;
                PPROCS[cur].mmap_base = 0x40000000;

                // Switch CR3 to the new page directory.
                // The kernel identity map (PDE 0) covers our current stack.
                core::arch::asm!("mov cr3, {pd}", pd = in(reg) new_pd);

                // Modify the kernel stack frame to jump to the new entry point
                // The frame is at SYSCALL_ESP (captured by rust_handle_syscall).
                let frame = crate::interrupts::SYSCALL_ESP as *mut u32;
                frame.add(12).write(entry);  // user_EIP at [48] = frame + 12
                frame.add(15).write(sp);     // user_ESP at [60] = frame + 15

                serial_driver::sprintln!("  execve: loaded, entry={:#x}, sp={:#x}, pd={:#x}", entry, sp, new_pd);
                // Return 0 — the syscall handler stores this in EAX (pusha),
                // and iretd jumps to the new EIP/ESP from the modified frame.
                0
            }
            Err(e) => {
                serial_driver::sprintln!("  execve: ELF error: {}", e);
                -syscall::ENOENT
            }
        }
    }
}

// ─── Waitpid ────────────────────────────────────────────

pub fn sys_waitpid(pid: i32, status: u32, options: i32) -> i32 {
    unsafe {
        loop {
            for i in 1..MAX_PROCS {
                if PPROCS[i].ppid == CURRENT_PID && PPROCS[i].state as u8 == State::Zombie as u8 {
                    if pid <= 0 || pid as usize == i {
                        let child_pid = PPROCS[i].pid;
                        let exit_code = PPROCS[i].exit_code;
                        if status != 0 {
                            (status as *mut u32).write((exit_code & 0xFF) << 8);
                        }
                        PPROCS[i].state = State::Free;
                        serial_driver::sprintln!("  waitpid: reaped PID {} (exit {})", child_pid, exit_code);
                        return child_pid as i32;
                    }
                }
            }
            if options & syscall::WNOHANG != 0 {
                return 0;
            }
            let mut has_child = false;
            for i in 1..MAX_PROCS {
                if PPROCS[i].ppid == CURRENT_PID && PPROCS[i].state as u8 != State::Free as u8 {
                    has_child = true;
                    break;
                }
            }
            if !has_child {
                return -syscall::ECHILD;
            }
            // No zombie yet — block so scheduler switches away
            // Return -EINTR so musl's waitpid wrapper retries the syscall
            PPROCS[CURRENT_PID as usize].state = State::Blocked;
            return -syscall::EINTR;
        }
    }
}

pub fn sys_wait4(pid: i32, status: u32, options: i32, _rusage: u32) -> i32 {
    sys_waitpid(pid, status, options)
}

// ─── Context switch support ─────────────────────────────

// Called from interrupts.asm syscall_handler after rust_handle_syscall returns.
// Takes the current kernel stack ESP (pointing to saved context frame),
// saves it in the current process, finds the next ready process,
// and returns the new process's saved_esp (or 0 if no switch).
#[no_mangle]
pub extern "C" fn context_switch_handler(esp: u32) -> u32 {
    unsafe {
        let cur = CURRENT_PID as usize;
        if cur >= MAX_PROCS || PPROCS[cur].state as u8 == State::Free as u8 {
            return 0;
        }

        // Deliver pending signals before returning to userspace
        check_signals();

        // Save current context
        PPROCS[cur].saved_esp = esp;

        // If current is Blocked or Zombie, we MUST switch
        let cur_state = PPROCS[cur].state as u8;
        if cur_state == State::Blocked as u8 || cur_state == State::Zombie as u8 {
            // Debug: dump process states
            for p in 1..4.min(MAX_PROCS) {
                serial_driver::sprintln!("  sched: PID {} state={}", p, PPROCS[p].state as u8);
            }
            // Find the next process to run
            let start = cur;
            let mut next = (start + 1) % MAX_PROCS;
            while next != start {
                let st = PPROCS[next].state as u8;
                if st == State::Ready as u8 || st == State::Running as u8 {
                    break;
                }
                next = (next + 1) % MAX_PROCS;
            }

            if next == start {
                // No other process ready.
                serial_driver::sprintln!("  scheduler: no ready processes, halting");
                loop { core::arch::asm!("cli; hlt"); }
            }

            // Switch to next process
            let next_pid = next as u32;
            serial_driver::sprintln!("  sched: PID {} -> PID {} (state={})",
                CURRENT_PID, next_pid, PPROCS[next].state as u8);
            CURRENT_PID = next_pid;
            PPROCS[next].state = State::Running;
            TSS.esp0 = PPROCS[next].kstack_base + KSTACK_SIZE;
            serial_driver::sprintln!("  sched: switching to PID {}, esp={:#x}",
                next_pid, PPROCS[next].saved_esp);
            return PPROCS[next].saved_esp;
        }

        // Process is still Runnable — keep it Running, no switch
        PPROCS[cur].state = State::Running;
        TSS.esp0 = PPROCS[cur].kstack_base + KSTACK_SIZE;
        0
    }
}

fn switch_to_next() -> ! {
    unsafe {
        let cur = CURRENT_PID as usize;
        PPROCS[cur].saved_esp = 0;

        // If current became zombie, try parent first
        if PPROCS[cur].state as u8 == State::Zombie as u8 {
            let ppid = PPROCS[cur].ppid as usize;
            if ppid < MAX_PROCS && ppid != cur && PPROCS[ppid].state as u8 == State::Ready as u8 {
                CURRENT_PID = ppid as u32;
                PPROCS[ppid].state = State::Running;
                TSS.esp0 = PPROCS[ppid].kstack_base + KSTACK_SIZE;
                let new_esp = PPROCS[ppid].saved_esp;
                let new_pd = PPROCS[ppid].pd;
                core::arch::asm!(
                    "mov cr3, {pd}",
                    "mov esp, {esp}",
                    ".byte 0x0f, 0xa9",  // pop gs  (32-bit)
                    ".byte 0x0f, 0xa1",  // pop fs  (32-bit)
                    ".byte 0x07",        // pop es  (32-bit)
                    ".byte 0x1f",        // pop ds  (32-bit)
                    "popa",
                    "iretd",
                    pd = in(reg) new_pd,
                    esp = in(reg) new_esp,
                    options(noreturn)
                );
            }
        }

        let mut next = (cur + 1) % MAX_PROCS;
        while next != cur {
            if PPROCS[next].state as u8 == State::Ready as u8 || PPROCS[next].state as u8 == State::Running as u8 {
                break;
            }
            next = (next + 1) % MAX_PROCS;
        }

        if next == cur {
            serial_driver::sprintln!("  scheduler: no more processes, halting");
            loop { core::arch::asm!("cli; hlt"); }
        }

        let new_esp = PPROCS[next].saved_esp;
        CURRENT_PID = next as u32;
        PPROCS[next].state = State::Running;
        TSS.esp0 = PPROCS[next].kstack_base + KSTACK_SIZE;

        let new_pd = PPROCS[next].pd;
        core::arch::asm!(
            "mov cr3, {pd}",
            "mov esp, {esp}",
            ".byte 0x0f, 0xa9",  // pop gs  (32-bit)
            ".byte 0x0f, 0xa1",  // pop fs  (32-bit)
            ".byte 0x07",        // pop es  (32-bit)
            ".byte 0x1f",        // pop ds  (32-bit)
            "popa",
            "iretd",
            pd = in(reg) new_pd,
            esp = in(reg) new_esp,
            options(noreturn)
        );
    }
}

// ─── First process setup (called from kmain) ───────────

// Set up PID 1 (shell process) context to enter shell_loop
// Stack layout must match syscall_handler's frame:
// [ESP+0] = gs, [4] = fs, [8] = es, [12] = ds
// [16] = edi, [20] = esi, [24] = ebp, [28] = old_esp
// [32] = ebx, [36] = edx, [40] = ecx, [44] = eax
// [48] = EIP, [52] = CS, [56] = EFLAGS  (ring 0 → ring 0: 3 items)
pub fn setup_pid1(shell_entry: usize) {
    unsafe {
        let kstack = match alloc_kstack() {
            Some(s) => s,
            None => {
                serial_driver::sprintln!("  process: failed to allocate kernel stack for PID 1");
                return;
            }
        };
        PPROCS[1].kstack_base = kstack - KSTACK_SIZE;
        let mut sp = kstack;

        // iretd frame: 3 items for ring 0 → ring 0 (no SS/ESP)
        sp -= 4; *(sp as *mut u32) = 0x202;              // EFLAGS
        sp -= 4; *(sp as *mut u32) = 0x08;               // CS = kernel code
        sp -= 4; *(sp as *mut u32) = shell_entry as u32; // EIP

        // pusha: eax, ecx, edx, ebx, old_esp, ebp, esi, edi
        sp -= 4; *(sp as *mut u32) = 0; // eax
        sp -= 4; *(sp as *mut u32) = 0; // ecx
        sp -= 4; *(sp as *mut u32) = 0; // edx
        sp -= 4; *(sp as *mut u32) = 0; // ebx
        sp -= 4; *(sp as *mut u32) = 0; // old_esp
        sp -= 4; *(sp as *mut u32) = 0; // ebp
        sp -= 4; *(sp as *mut u32) = 0; // esi
        sp -= 4; *(sp as *mut u32) = 0; // edi

        // segments: ds, es, fs, gs (stack grows down; last pushed = gs = saved_esp)
        sp -= 4; *(sp as *mut u32) = 0x10; // ds
        sp -= 4; *(sp as *mut u32) = 0x10; // es
        sp -= 4; *(sp as *mut u32) = 0x10; // fs
        sp -= 4; *(sp as *mut u32) = 0x10; // gs ← ESP points here

        PPROCS[1].saved_esp = sp;
        PPROCS[1].state = State::Ready;

        serial_driver::sprintln!("  process: PID 1 (shell) context set up, esp={:#x}", sp);
    }
}

// Create a user process from ELF data and set it as Ready
// Stack layout must match syscall_handler's frame:
// [ESP+0] = gs, [4] = fs, [8] = es, [12] = ds
// [16] = edi, [20] = esi, [24] = ebp, [28] = old_esp
// [32] = ebx, [36] = edx, [40] = ecx, [44] = eax
// [48] = user_EIP, [52] = user_CS, [56] = EFLAGS
// [60] = user_ESP, [64] = user_SS  (ring 3: 5 items)
pub fn create_user(data: &[u8]) -> Result<u32, ()> {
    let (pd, entry, user_sp) = crate::personality::linux::load_elf(data).map_err(|_| ())?;

    let new_pid = alloc_pid().ok_or(())?;
    let child = new_pid as usize;
    let kstack = alloc_kstack().ok_or(())?;

    unsafe {
        PPROCS[child].state = State::Ready;
        PPROCS[child].pid = new_pid;
        PPROCS[child].ppid = 1;
        PPROCS[child].pd = pd;
        PPROCS[child].kstack_base = kstack - KSTACK_SIZE;
        PPROCS[child].uid = 1000;
        PPROCS[child].gid = 1000;
        PPROCS[child].euid = 1000;
        PPROCS[child].egid = 1000;
        PPROCS[child].pgid = new_pid;
        PPROCS[child].sid = 1;
        PPROCS[child].brk = 0x40000000;
        PPROCS[child].cwd[0] = b'/';
        PPROCS[child].cwd_len = 1;

        // Set up stdin/stdout/stderr
        set_fd(new_pid, 0, FdType::Stdin, false);
        set_fd(new_pid, 1, FdType::Stdout, false);
        set_fd(new_pid, 2, FdType::Stderr, false);

        let mut esp = kstack;

        // Push order: last push = saved_esp (gs), first push = highest addr

        // 1. iretd frame (ring 0 → ring 3) — 5 items, pushed first (highest address)
        esp -= 4; *(esp as *mut u32) = 0x23;           // user_SS
        esp -= 4; *(esp as *mut u32) = user_sp;        // user_ESP
        esp -= 4; *(esp as *mut u32) = 0x202;           // EFLAGS
        esp -= 4; *(esp as *mut u32) = 0x1B;            // user_CS
        esp -= 4; *(esp as *mut u32) = entry;           // user_EIP

        // 2. pusha: eax, ecx, edx, ebx, old_esp, ebp, esi, edi
        let pusha_old_esp = esp; // ESP before pusha = points to user_EIP
        esp -= 4; *(esp as *mut u32) = 0; // eax
        esp -= 4; *(esp as *mut u32) = 0; // ecx
        esp -= 4; *(esp as *mut u32) = 0; // edx
        esp -= 4; *(esp as *mut u32) = 0; // ebx
        esp -= 4; *(esp as *mut u32) = pusha_old_esp; // old_esp
        esp -= 4; *(esp as *mut u32) = 0; // ebp
        esp -= 4; *(esp as *mut u32) = 0; // esi
        esp -= 4; *(esp as *mut u32) = 0; // edi

        // 3. segments: ds, es, fs, gs
        esp -= 4; *(esp as *mut u32) = 0x10; // ds
        esp -= 4; *(esp as *mut u32) = 0x10; // es
        esp -= 4; *(esp as *mut u32) = 0x10; // fs
        esp -= 4; *(esp as *mut u32) = 0x10; // gs ← saved_esp

        PPROCS[child].saved_esp = esp;

        serial_driver::sprintln!("  process: created PID {}, entry={:#x}, pd={:#x}, esp={:#x}",
            new_pid, entry, pd, esp);
    }

    Ok(new_pid)
}

// Called from kmain to start the first ready process (never returns)
pub fn scheduler_enter() -> ! {
    unsafe {
        // Prefer PID 2 (user process) over PID 1 (shell) as first to run
        for i in (1..MAX_PROCS).rev() {
            if PPROCS[i].state as u8 == State::Ready as u8 {
                CURRENT_PID = i as u32;
                PPROCS[i].state = State::Running;
                TSS.esp0 = PPROCS[i].kstack_base + KSTACK_SIZE;
                let new_esp = PPROCS[i].saved_esp;
                let new_pd = PPROCS[i].pd;

                serial_driver::sprintln!("  sched: starting PID {} pd={:#x} esp={:#x}", i, new_pd, new_esp);

                // Read frame values from the saved stack (cr3-safe: both PDs identity-map 0-4MB)
                let ctx = core::slice::from_raw_parts(new_esp as *const u32, 18);
                serial_driver::sprintln!("  sched: ctx eip={:#x} cs={:#x} efl={:#x} usp={:#x} ss={:#x}",
                    ctx[12], ctx[13], ctx[14], ctx[15], ctx[16]);

                // Single asm block: switch PD, stack, then restore context
                // Use raw byte encodings for segment pops to avoid LLVM's 16-bit mode bug
                core::arch::asm!(
                    "mov cr3, {pd}",
                    "mov esp, {esp}",
                    ".byte 0x0f, 0xa9",  // pop gs  (32-bit)
                    ".byte 0x0f, 0xa1",  // pop fs  (32-bit)
                    ".byte 0x07",        // pop es  (32-bit)
                    ".byte 0x1f",        // pop ds  (32-bit)
                    "popa",
                    "iretd",
                    pd = in(reg) new_pd,
                    esp = in(reg) new_esp,
                    options(noreturn)
                );
            }
        }
        // No ready process — fall back to shell directly
        serial_driver::sprintln!("  scheduler: no ready processes, entering shell directly");
        crate::shell::shell_loop()
    }
}
