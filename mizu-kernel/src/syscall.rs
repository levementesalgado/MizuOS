// Linux i386 syscall numbers (from <asm/unistd_32.h>)
pub const SYS_RESTART_SYSCALL: u32 = 0;
pub const SYS_EXIT: u32 = 1;
pub const SYS_FORK: u32 = 2;
pub const SYS_READ: u32 = 3;
pub const SYS_WRITE: u32 = 4;
pub const SYS_OPEN: u32 = 5;
pub const SYS_CLOSE: u32 = 6;
pub const SYS_WAITPID: u32 = 7;
pub const SYS_CREAT: u32 = 8;
pub const SYS_LINK: u32 = 9;
pub const SYS_UNLINK: u32 = 10;
pub const SYS_EXECVE: u32 = 11;
pub const SYS_CHDIR: u32 = 12;
pub const SYS_TIME: u32 = 13;
pub const SYS_MKNOD: u32 = 14;
pub const SYS_CHMOD: u32 = 15;
pub const SYS_LCHOWN: u32 = 16;
pub const SYS_BREAK: u32 = 17;
pub const SYS_STAT: u32 = 18;
pub const SYS_LSEEK: u32 = 19;
pub const SYS_GETPID: u32 = 20;
pub const SYS_MOUNT: u32 = 21;
pub const SYS_UMOUNT: u32 = 22;
pub const SYS_SETUID: u32 = 23;
pub const SYS_GETUID: u32 = 24;
pub const SYS_STIME: u32 = 25;
pub const SYS_PTRACE: u32 = 26;
pub const SYS_ALARM: u32 = 27;
pub const SYS_FSTAT: u32 = 28;
pub const SYS_PAUSE: u32 = 29;
pub const SYS_UTIME: u32 = 30;
pub const SYS_ACCESS: u32 = 33;
pub const SYS_NICE: u32 = 34;
pub const SYS_SYNC: u32 = 36;
pub const SYS_KILL: u32 = 37;
pub const SYS_RENAME: u32 = 38;
pub const SYS_MKDIR: u32 = 39;
pub const SYS_RMDIR: u32 = 40;
pub const SYS_DUP: u32 = 41;
pub const SYS_PIPE: u32 = 42;
pub const SYS_TIMES: u32 = 43;
pub const SYS_BRK: u32 = 45;
pub const SYS_SETGID: u32 = 46;
pub const SYS_GETGID: u32 = 47;
pub const SYS_GETEUID: u32 = 49;
pub const SYS_GETEGID: u32 = 50;
pub const SYS_GETPGID: u32 = 132;
pub const SYS_IOCTL: u32 = 54;
pub const SYS_FCNTL: u32 = 55;
pub const SYS_SETPGID: u32 = 57;
pub const SYS_UMASK: u32 = 60;
pub const SYS_DUP2: u32 = 63;
pub const SYS_GETPPID: u32 = 64;
pub const SYS_GETPGRP: u32 = 65;
pub const SYS_SETSID: u32 = 66;
pub const SYS_SIGACTION: u32 = 67;
pub const SYS_SIGSUSPEND: u32 = 72;
pub const SYS_SIGPENDING: u32 = 73;
pub const SYS_SIGPROCMASK: u32 = 126;
pub const SYS_SIGRETURN: u32 = 119;
pub const SYS_SGETMASK: u32 = 132;
pub const SYS_GETDENTS: u32 = 141;
pub const SYS_GETSID: u32 = 147;
pub const SYS_SYSLOG: u32 = 103;
pub const SYS_UNAME: u32 = 122;
pub const SYS_MUNMAP: u32 = 91;
pub const SYS_MMAP: u32 = 192;
pub const SYS_MPROTECT: u32 = 125;
pub const SYS_GETDENTS64: u32 = 220;
pub const SYS_READLINK: u32 = 85;
pub const SYS_SYMLINK: u32 = 83;
pub const SYS_FSTAT64: u32 = 197;
pub const SYS_STAT64: u32 = 195;
pub const SYS_LSTAT: u32 = 84;
pub const SYS_LSTAT64: u32 = 196;
pub const SYS_LLSEEK: u32 = 140;
pub const SYS_FCNTL64: u32 = 221;
pub const SYS_EXIT_GROUP: u32 = 252;
pub const SYS_GETDENTS64_ALT: u32 = 220;
pub const SYS_CLOCK_GETTIME: u32 = 265;
pub const SYS_CLOCK_SETTIME: u32 = 266;
pub const SYS_NANOSLEEP: u32 = 162;
pub const SYS_GETTIMEOFDAY: u32 = 78;
pub const SYS_SETTIMEOFDAY: u32 = 79;
pub const SYS_SCHED_YIELD: u32 = 158;
pub const SYS_GETCWD: u32 = 183;
pub const SYS_UTIMENSAT: u32 = 320;
pub const SYS_READV: u32 = 145;
pub const SYS_WRITEV: u32 = 146;
pub const SYS_PIPE2: u32 = 331;
pub const SYS_GETRANDOM: u32 = 355;
pub const SYS_CLOCK_GETRES: u32 = 264;
pub const SYS_TGKILL: u32 = 270;
pub const SYS_SIGALTSTACK: u32 = 186;
pub const SYS_RT_SIGACTION: u32 = 174;
pub const SYS_RT_SIGPROCMASK: u32 = 175;
pub const SYS_RT_SIGPENDING: u32 = 176;
pub const SYS_RT_SIGSUSPEND: u32 = 179;
pub const SYS_RT_SIGTIMEDWAIT: u32 = 177;
pub const SYS_RT_SIGQUEUEINFO: u32 = 178;
pub const SYS_RT_SIGRETURN: u32 = 173;
pub const SYS_WAIT4: u32 = 114;
pub const SYS_PRLIMIT64: u32 = 319;
pub const SYS_GETRLIMIT: u32 = 191;
pub const SYS_SETRLIMIT: u32 = 190;
pub const SYS_UNAME_OLD: u32 = 122;
pub const SYS_ARCH_PRCTL: u32 = 224;
pub const SYS_OPENAT: u32 = 295;
pub const SYS_FACCESSAT: u32 = 292;
pub const SYS_FSTATAT: u32 = 300;
pub const SYS_NEWFSTATAT: u32 = 300;

// arch_prctl codes (Linux)
pub const ARCH_SET_FS: u32 = 0x1002;
pub const ARCH_GET_FS: u32 = 0x1003;
pub const ARCH_SET_GS: u32 = 0x1001;
pub const ARCH_GET_GS: u32 = 0x1004;

// AT_FDCWD as u32 (i.e. -100)
pub const AT_FDCWD: u32 = 0xFFFFFF9C;

// errno values (Linux <asm-generic/errno.h>)
pub const ENOENT: i32 = 2;
pub const EIO: i32 = 5;
pub const EBADF: i32 = 9;
pub const ECHILD: i32 = 10;
pub const EAGAIN: i32 = 11;
pub const ENOMEM: i32 = 12;
pub const EACCES: i32 = 13;
pub const EFAULT: i32 = 14;
pub const ENOTDIR: i32 = 20;
pub const EISDIR: i32 = 21;
pub const EINVAL: i32 = 22;
pub const ENFILE: i32 = 23;
pub const EMFILE: i32 = 24;
pub const ENOSYS: i32 = 38;
pub const ENOTSUP: i32 = 95;
pub const EADDRINUSE: i32 = 98;
pub const EWOULDBLOCK: i32 = 11;
pub const ENODEV: i32 = 19;
pub const ESPIPE: i32 = 29;
pub const ESRCH: i32 = 3;
pub const EINTR: i32 = 4;
pub const ENOTTY: i32 = 25;
pub const EPIPE: i32 = 32;
pub const ERANGE: i32 = 34;

pub const FD_CLOEXEC: u32 = 1;

pub const STDIN_FILENO: u32 = 0;
pub const STDOUT_FILENO: u32 = 1;
pub const STDERR_FILENO: u32 = 2;

// O_* constants for open
pub const O_RDONLY: u32 = 0;
pub const O_WRONLY: u32 = 1;
pub const O_RDWR: u32 = 2;
pub const O_CREAT: u32 = 0o100;
pub const O_TRUNC: u32 = 0o1000;
pub const O_APPEND: u32 = 0o2000;
pub const O_NONBLOCK: u32 = 0o4000;
pub const O_CLOEXEC: u32 = 0o2000000;

// fcntl commands
pub const F_DUPFD: i32 = 0;
pub const F_GETFD: i32 = 1;
pub const F_SETFD: i32 = 2;
pub const F_GETFL: i32 = 3;
pub const F_SETFL: i32 = 4;

// ioctl commands (Linux <asm-generic/ioctls.h>)
pub const TCGETS: u32 = 0x5401;
pub const TCSETS: u32 = 0x5402;
pub const TCSETSW: u32 = 0x5403;
pub const TCSETSF: u32 = 0x5404;
pub const TIOCGWINSZ: u32 = 0x5413;
pub const TIOCSPGRP: u32 = 0x5410;
pub const TIOCGPGRP: u32 = 0x540F;
pub const TIOCSCTTY: u32 = 0x540E;
pub const TIOCNOTTY: u32 = 0x5422;
pub const FIONREAD: u32 = 0x541B;
pub const TIOCGSERIAL: u32 = 0x541E;
pub const TIOCGPTN: u32 = 0x80045430;
pub const TIOCSPTLCK: u32 = 0x40045431;

// ioctl directions
pub const IOC_NONE: u32 = 0;
pub const IOC_WRITE: u32 = 1;
pub const IOC_READ: u32 = 2;
pub const IOC_SIZE_BITS: u32 = 14;

// termios c_iflag
pub const IGNBRK: u32 = 1;
pub const BRKINT: u32 = 2;
pub const IGNPAR: u32 = 4;
pub const PARMRK: u32 = 8;
pub const INPCK: u32 = 16;
pub const ISTRIP: u32 = 32;
pub const INLCR: u32 = 64;
pub const IGNCR: u32 = 128;
pub const ICRNL: u32 = 256;

// termios c_oflag
pub const OPOST: u32 = 1;

// termios c_cflag
pub const CBAUD: u32 = 0x100F;
pub const CSIZE: u32 = 0x30;
pub const CS7: u32 = 0x20;
pub const CS8: u32 = 0x30;
pub const CSTOPB: u32 = 0x40;
pub const CREAD: u32 = 0x80;
pub const PARENB: u32 = 0x100;
pub const PARODD: u32 = 0x200;

// termios c_lflag
pub const ISIG: u32 = 1;
pub const ICANON: u32 = 2;
pub const ECHO: u32 = 8;
pub const ECHOE: u32 = 16;
pub const ECHOK: u32 = 32;
pub const ECHONL: u32 = 64;
pub const NOFLSH: u32 = 256;
pub const TOSTOP: u32 = 512;
pub const IEXTEN: u32 = 0x8000;

// termios NCCS
pub const NCCS: usize = 19;

// termios VMIN/VTIME
pub const VINTR: usize = 0;
pub const VQUIT: usize = 1;
pub const VERASE: usize = 2;
pub const VKILL: usize = 3;
pub const VEOF: usize = 4;
pub const VTIME: usize = 5;
pub const VMIN: usize = 6;
pub const VSWTC: usize = 7;
pub const VSTART: usize = 8;
pub const VSTOP: usize = 9;
pub const VSUSP: usize = 10;
pub const VEOL: usize = 11;
pub const VREPRINT: usize = 12;
pub const VDISCARD: usize = 13;
pub const VWERASE: usize = 14;
pub const VLNEXT: usize = 15;
pub const VEOL2: usize = 16;

// mmap flags
pub const PROT_READ: u32 = 1;
pub const PROT_WRITE: u32 = 2;
pub const PROT_EXEC: u32 = 4;
pub const PROT_NONE: u32 = 0;
pub const MAP_SHARED: u32 = 1;
pub const MAP_PRIVATE: u32 = 2;
pub const MAP_ANONYMOUS: u32 = 0x20;
pub const MAP_FAILED: u32 = 0xFFFFFFFF;

// signals
pub const SIGHUP: u32 = 1;
pub const SIGINT: u32 = 2;
pub const SIGQUIT: u32 = 3;
pub const SIGILL: u32 = 4;
pub const SIGTRAP: u32 = 5;
pub const SIGABRT: u32 = 6;
pub const SIGBUS: u32 = 7;
pub const SIGFPE: u32 = 8;
pub const SIGKILL: u32 = 9;
pub const SIGUSR1: u32 = 10;
pub const SIGSEGV: u32 = 11;
pub const SIGUSR2: u32 = 12;
pub const SIGPIPE: u32 = 13;
pub const SIGALRM: u32 = 14;
pub const SIGTERM: u32 = 15;
pub const SIGSTKFLT: u32 = 16;
pub const SIGCHLD: u32 = 17;
pub const SIGCONT: u32 = 18;
pub const SIGSTOP: u32 = 19;
pub const SIGTSTP: u32 = 20;
pub const SIGTTIN: u32 = 21;
pub const SIGTTOU: u32 = 22;
pub const SIGURG: u32 = 23;
pub const SIGXCPU: u32 = 24;
pub const SIGXFSZ: u32 = 25;
pub const SIGVTALRM: u32 = 26;
pub const SIGPROF: u32 = 27;
pub const SIGWINCH: u32 = 28;
pub const SIGIO: u32 = 29;
pub const SIGCLD: u32 = 17;
pub const SIGPOLL: u32 = 29;
pub const SIGPWR: u32 = 30;
pub const NSIG: u32 = 32;

// sigaction flags
pub const SA_NOCLDSTOP: u32 = 1;
pub const SA_NOCLDWAIT: u32 = 2;
pub const SA_SIGINFO: u32 = 4;
pub const SA_RESTART: u32 = 0x10000000;

// waitpid options
pub const WNOHANG: i32 = 1;
pub const WUNTRACED: i32 = 2;

// stat structure size
pub const STAT_STRUCT_SIZE: usize = 88;
pub const STAT64_STRUCT_SIZE: usize = 100;

// utsname for uname
pub const UTSNAME_LEN: usize = 65;
