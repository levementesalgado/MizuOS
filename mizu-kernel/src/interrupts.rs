use crate::keyboard;
use crate::pit;
use crate::serial_driver;
use crate::process;
use crate::syscall;

extern "C" {
    static isr_stubs: [u32; 49];
    static syscall_isr_index: u32;
    fn idt_load();
    fn idt_set_entry(n: u32, base: u32, selector: u32, flags: u32);
}

pub fn init() {
    let cs: u32 = 0x08;
    let flags: u32 = 0x8E;
    let user_flags: u32 = 0xEE;

    for i in 0u32..48 {
        let addr = unsafe { isr_stubs[i as usize] };
        unsafe { idt_set_entry(i, addr, cs, flags); }
    }

    let syscall_idx = unsafe { syscall_isr_index };
    let syscall_addr = unsafe { isr_stubs[syscall_idx as usize] };
    unsafe { idt_set_entry(0x80, syscall_addr, cs, user_flags); }

    unsafe { idt_load(); }

    serial_driver::sprintln!("  gdt/idt: loaded");
}

// saved_regs = [gs, fs, es, ds, edi, esi, ebp, old_esp, ebx, edx, ecx, eax]
// saved as global so sys_execve can modify the kernel stack frame (EIP/ESP)
pub static mut SYSCALL_ESP: u32 = 0;

#[no_mangle]
pub extern "C" fn rust_handle_syscall(syscall_num: u32, saved_regs: &[u32; 12]) -> u32 {
    unsafe { SYSCALL_ESP = saved_regs as *const _ as u32; }

    let ebx = saved_regs[8];
    let ecx = saved_regs[10];
    let edx = saved_regs[9];
    let esi = saved_regs[5];
    let edi = saved_regs[4];

    match syscall_num {
        syscall::SYS_EXIT => {
            process::sys_exit(ebx)
        }
        syscall::SYS_EXIT_GROUP => {
            process::sys_exit_group(ebx)
        }
        syscall::SYS_FORK => {
            process::sys_fork() as u32
        }
        syscall::SYS_READ => {
            process::sys_read(ebx, ecx, edx)
        }
        syscall::SYS_WRITE => {
            process::sys_write(ebx, ecx, edx)
        }
        syscall::SYS_READV => {
            process::sys_readv(ebx, ecx, edx) as u32
        }
        syscall::SYS_WRITEV => {
            process::sys_writev(ebx, ecx, edx) as u32
        }
        syscall::SYS_OPEN => {
            process::sys_open(ebx, ecx, edx)
        }
        syscall::SYS_OPENAT => {
            process::sys_openat(ebx, ecx, edx, esi)
        }
        syscall::SYS_FSTATAT => {
            process::sys_fstatat(ebx, ecx, edx, esi) as u32
        }
        syscall::SYS_NEWFSTATAT => {
            process::sys_fstatat(ebx, ecx, edx, esi) as u32
        }
        syscall::SYS_ARCH_PRCTL => {
            process::sys_arch_prctl(ebx, ecx) as u32
        }
        syscall::SYS_CLOSE => {
            process::sys_close(ebx)
        }
        syscall::SYS_WAITPID => {
            process::sys_waitpid(ebx as i32, ecx, edx as i32) as u32
        }
        syscall::SYS_WAIT4 => {
            process::sys_wait4(ebx as i32, ecx, edx as i32, esi) as u32
        }
        syscall::SYS_EXECVE => {
            process::sys_execve(ebx, ecx, edx) as u32
        }
        syscall::SYS_CHDIR => {
            process::sys_chdir(ebx) as u32
        }
        syscall::SYS_TIME => {
            process::sys_time()
        }
        syscall::SYS_GETPID => {
            process::sys_getpid()
        }
        syscall::SYS_GETPPID => {
            process::sys_getppid()
        }
        syscall::SYS_GETUID => {
            process::sys_getuid()
        }
        syscall::SYS_GETEUID => {
            process::sys_geteuid()
        }
        syscall::SYS_GETGID => {
            process::sys_getgid()
        }
        syscall::SYS_GETEGID => {
            process::sys_getegid()
        }
        syscall::SYS_SETUID => {
            process::sys_setuid(ebx) as u32
        }
        syscall::SYS_SETGID => {
            process::sys_setgid(ebx) as u32
        }
        syscall::SYS_ALARM => {
            process::sys_alarm(ebx)
        }
        syscall::SYS_BRK => {
            process::sys_brk(ebx)
        }
        syscall::SYS_ACCESS => {
            process::sys_access(ebx, ecx) as u32
        }
        syscall::SYS_KILL => {
            process::sys_kill(ebx, ecx) as u32
        }
        syscall::SYS_TGKILL => {
            process::sys_tgkill(ebx, ecx, edx) as u32
        }
        syscall::SYS_IOCTL => {
            process::sys_ioctl(ebx, ecx, edx) as u32
        }
        syscall::SYS_FCNTL => {
            process::sys_fcntl(ebx, ecx, edx) as u32
        }
        syscall::SYS_FCNTL64 => {
            process::sys_fcntl64(ebx, ecx, edx) as u32
        }
        syscall::SYS_UNAME => {
            process::sys_uname(ebx) as u32
        }
        syscall::SYS_GETCWD => {
            process::sys_getcwd(ebx, ecx) as u32
        }
        syscall::SYS_READLINK => {
            process::sys_readlink(ebx, ecx, edx) as u32
        }
        syscall::SYS_PIPE => {
            process::sys_pipe(ebx) as u32
        }
        syscall::SYS_PIPE2 => {
            process::sys_pipe2(ebx, ecx) as u32
        }
        syscall::SYS_DUP => {
            process::sys_dup(ebx) as u32
        }
        syscall::SYS_DUP2 => {
            process::sys_dup2(ebx, ecx) as u32
        }
        syscall::SYS_GETPGRP => {
            process::sys_getpgrp()
        }
        syscall::SYS_GETPGID => {
            process::sys_getpgid(ebx) as u32
        }
        syscall::SYS_SETSID => {
            process::sys_setsid()
        }
        syscall::SYS_SETPGID => {
            process::sys_setpgid(ebx, ecx) as u32
        }
        syscall::SYS_GETSID => {
            process::sys_getsid(ebx)
        }
        syscall::SYS_SIGACTION => {
            process::sys_sigaction(ebx, ecx, edx) as u32
        }
        syscall::SYS_RT_SIGACTION => {
            process::sys_sigaction(ebx, ecx, edx) as u32
        }
        syscall::SYS_SIGPROCMASK => {
            process::sys_sigprocmask(ebx, ecx, edx) as u32
        }
        syscall::SYS_RT_SIGPROCMASK => {
            process::sys_sigprocmask(ebx, ecx, edx) as u32
        }
        syscall::SYS_SIGPENDING => {
            process::sys_sigpending(ebx) as u32
        }
        syscall::SYS_SIGSUSPEND => {
            process::sys_sigsuspend(ebx) as u32
        }
        syscall::SYS_SIGALTSTACK => {
            process::sys_sigaltstack(ebx, ecx) as u32
        }
        syscall::SYS_SIGRETURN => {
            process::sys_sigreturn() as u32
        }
        syscall::SYS_RT_SIGRETURN => {
            process::sys_sigreturn() as u32
        }
        syscall::SYS_SCHED_YIELD => {
            process::sys_sched_yield() as u32
        }
        syscall::SYS_GETDENTS => {
            process::sys_getdents(ebx, ecx, edx) as u32
        }
        syscall::SYS_GETDENTS64 => {
            process::sys_getdents64(ebx, ecx, edx) as u32
        }
        syscall::SYS_LSEEK => {
            process::sys_lseek(ebx, ecx, edx) as u32
        }
        syscall::SYS_STAT => {
            sys_stat(ebx, ecx)
        }
        syscall::SYS_LSTAT => {
            sys_stat(ebx, ecx)
        }
        syscall::SYS_LSTAT64 => {
            sys_stat(ebx, ecx)
        }
        syscall::SYS_STAT64 => {
            sys_stat(ebx, ecx)
        }
        syscall::SYS_FSTAT => {
            sys_fstat(ebx, ecx)
        }
        syscall::SYS_FSTAT64 => {
            sys_fstat(ebx, ecx)
        }
        syscall::SYS_MMAP => {
            process::sys_mmap(ebx, ecx, edx, esi, edi, saved_regs[6])
        }
        syscall::SYS_MUNMAP => {
            process::sys_munmap(ebx, ecx) as u32
        }
        syscall::SYS_MPROTECT => {
            process::sys_mprotect(ebx, ecx, edx) as u32
        }
        syscall::SYS_LLSEEK => {
            edx
        }
        syscall::SYS_SYMLINK => {
            process::sys_symlink(ebx, ecx) as u32
        }
        syscall::SYS_UNLINK => {
            process::sys_unlink(ebx) as u32
        }
        syscall::SYS_LINK => {
            process::sys_link(ebx, ecx) as u32
        }
        syscall::SYS_MKDIR => {
            process::sys_mkdir(ebx, ecx) as u32
        }
        syscall::SYS_RMDIR => {
            process::sys_rmdir(ebx) as u32
        }
        syscall::SYS_RENAME => {
            process::sys_rename(ebx, ecx) as u32
        }
        syscall::SYS_CLONE => {
            process::sys_clone(ebx, ecx, edx, esi, edi) as u32
        }
        syscall::SYS_SET_TID_ADDRESS => {
            process::sys_set_tid_address(ebx) as u32
        }
        syscall::SYS_STATX => {
            process::sys_statx(ebx, ecx, edx, esi) as u32
        }
        syscall::SYS_FACCESSAT => {
            process::sys_faccessat(ebx, ecx, edx, esi) as u32
        }
        syscall::SYS_CHMOD => {
            process::sys_chmod(ebx, ecx) as u32
        }
        syscall::SYS_CLOCK_GETTIME => {
            process::sys_clock_gettime(ebx, ecx) as u32
        }
        syscall::SYS_NANOSLEEP => {
            process::sys_nanosleep(ebx, ecx) as u32
        }
        syscall::SYS_GETTIMEOFDAY => {
            edx
        }
        syscall::SYS_UMASK => {
            process::sys_umask(ebx)
        }
        syscall::SYS_GETRANDOM => {
            process::sys_getrandom(ebx, ecx, edx) as u32
        }
        syscall::SYS_PRLIMIT64 => {
            process::sys_prlimit64(ebx, ecx, edx, esi) as u32
        }
        syscall::SYS_GETRLIMIT => {
            process::sys_getrlimit(ebx, ecx) as u32
        }
        syscall::SYS_SETRLIMIT => {
            process::sys_setrlimit(ebx, ecx) as u32
        }
        syscall::SYS_UTIMENSAT => {
            process::sys_utimensat(ebx, ecx, edx, esi) as u32
        }
        syscall::SYS_SYSLOG => {
            process::sys_syslog(ebx, ecx, edx) as u32
        }
        _ => {
            serial_driver::sprintln!("  [PID {}] unknown syscall {} (ebx={:#x}, ecx={:#x}, edx={:#x})",
                unsafe { crate::process::CURRENT_PID }, syscall_num, ebx, ecx, edx);
            -syscall::ENOSYS as u32
        }
    }
}

// ─── File I/O stubs ─────────────────────────────────────

fn sys_stat(pathname: u32, statbuf: u32) -> u32 {
    unsafe {
        let name = crate::process::raw_read_string(pathname);
        if name.is_empty() || !crate::fs::file_exists(&name) {
            return -syscall::ENOENT as u32;
        }
        if statbuf != 0 {
            let p = statbuf as *mut u8;
            for i in 0..syscall::STAT_STRUCT_SIZE {
                p.add(i).write(0);
            }
            // Set file type to regular
            *(statbuf as *mut u32).add(4) = 0o100777; // st_mode
        }
    }
    0
}

fn sys_fstat(fd: u32, statbuf: u32) -> u32 {
    if fd <= 2 {
        // stdin/stdout/stderr: set as character device
        if statbuf != 0 {
            unsafe {
                let p = statbuf as *mut u8;
                for i in 0..syscall::STAT_STRUCT_SIZE {
                    p.add(i).write(0);
                }
                *(statbuf as *mut u32).add(4) = 0o20777; // character device
            }
        }
        return 0;
    }
    // Regular file
    if statbuf != 0 {
        unsafe {
            let p = statbuf as *mut u8;
            for i in 0..syscall::STAT_STRUCT_SIZE {
                p.add(i).write(0);
            }
            *(statbuf as *mut u32).add(4) = 0o100777;
        }
    }
    0
}

// ─── Interrupt handler ──────────────────────────────────

#[no_mangle]
pub extern "C" fn interrupt_handler(isr: u32, err: u32, eip: u32, cs: u32) {
    match isr {
        0 => kpanic!("division by zero"),
        6 => kpanic!("invalid opcode"),
        8 => kpanic!("double fault (err={:#x})", err),
        13 => {
            serial_driver::sprintln!("GPF cs={:#x} eip={:#x} err={:#x}", cs, eip, err);
            kpanic!("general protection fault (err={:#x})", err)
        }
        14 => {
            let cr2: u32;
            unsafe { core::arch::asm!("mov {}, cr2", out(reg) cr2); }
            kpanic!("page fault (err={:#x}, cr2={:#x})", err, cr2)
        }
        32 => pit::tick(),
        33 => keyboard::irq_handler(),
        _ => {
            serial_driver::sprintln!("  unhandled ISR {}", isr);
        }
    }

    if isr >= 32 && isr <= 47 {
        pic_eoi(isr);
    }
}

fn pic_eoi(irq: u32) {
    if irq >= 40 {
        unsafe { core::arch::asm!("out dx, al", in("dx") 0xA0u16, in("al") 0x20u8, options(nomem, nostack)); }
    }
    unsafe { core::arch::asm!("out dx, al", in("dx") 0x20u16, in("al") 0x20u8, options(nomem, nostack)); }
}

macro_rules! kpanic {
    ($($arg:tt)*) => {{
        serial_driver::sprintln!("KERNEL PANIC: {}", format_args!($($arg)*));
        loop { unsafe { core::arch::asm!("cli; hlt"); } }
    }};
}
pub(crate) use kpanic;
