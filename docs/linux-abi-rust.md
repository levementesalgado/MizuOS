# ABI Linux (i386) em Rust — MizuOS

> Port conceitual do TipOS-Staging (`syscall_linux.zig`, x86-64) para i686 em Rust puro.
> Não copiar números: i386 e x86-64 têm tabelas diferentes.

## Status (esta madrugada)

- `int 0x80` via `syscall_handler` em `interrupts.asm`, dispatcher em `interrupts.rs`.
- Handler Linux já dispacha ~70 syscalls i386 (`syscall.rs`): read/write/open/close, fork/execve/waitpid/wait4, brk/mmap/munmap/mprotect, pipe/pipe2/dup/dup2/fcntl, ioctl TCGETS/TIOCGWINSZ, sigaction/sigprocmask (aceita e guarda), getdents/getdents64, stat/fstat (stub), uname, getcwd, getpid/uid/gid, setuid/setgid, get/set pgid/sid, sched_yield, clock_gettime/nanosleep (stub), getrandom (determinístico), rlimit, utimensat (stub), syslog (-ENOSYS), unlink/mkdir/rmdir/rename/symlink (-ENOSYS), etc.
- **Adicionado:** `readv` (145) e `writev` (146) em `process.rs` + dispatch em `interrupts.rs`. Port do TipOS: o `__stdio_read` do `musl` sempre usa `readv`; sem ele todo `fread` falha.

## Interrupções (ordem corrigida)

- PIC/PIT via Hako `sys.hako` (`pic_init` remapeando IRQ0→0x20, `pit_init(100)`), IDT via `interrupts.rs` + `interrupts.asm`.
- `sti` movido para **tarde**, logo antes de `scheduler_enter()` em `main.rs`. Antes era cedo (logo após `pic/pit`), com IRQ0 pendente antes de paging/processos prontos: risco de hang documentado no `README.md`.
- PIT 100 Hz só incrementa `TICKS`; preempção pronta virá quando a IRQ 32 também puder trocar ESP (hoje a troca só ocorre no `syscall_handler` via `context_switch_handler`: cooperativo por syscall).

## ELF loader (Rust, sem QEMU no PC)

- `personality/linux.rs`: ELF32 `ET_EXEC`, `PT_LOAD`, cópia para o PD novo, BSS zerado, `STACK_TOP=0xC0000000`, stack com `argc/argv/envp/auxvec` (`AT_RANDOM`, `AT_PHDR`, `AT_PHNUM`, `AT_ENTRY`, `AT_PAGESZ`, `AT_BASE=0`).
- `exec_elf()` reusa argv/envp do user e renderiza a mesma stack.

## Próximo (sem copiar o TipOS 1:1)

1. `openat` (295) / `fstatat` / `newfstatat` para o musl moderno (hoje `open` só aceita caminhos do initramfs).
2. `clone(120)` para threads/posix_spawn (TipOS já fez `fork` eager; `clone` é o passo seguinte).
3. `readv/writev` validar em QEMU com um `fopen/fread` musl.
4. `stat64` com tamanho real do arquivo (hoje zero + `st_mode`).

## Shell universal (ideia)

- Shell padrão estilo `nushell` (pipeline com dados estruturados), mas **no modo da personalidade**: ao detectar o binário (ELF `0x7F454C46` → Linux, `MZ` → DOS/Win32, classe JAVA `0xCAFEBABE`, WASM), a shell monta o contexto certo e traduz pipe/redirecionamento/expansão `$VAR` para a ABI.
- Dialetos: `bash` (Linux), `zsh` (Solaris, `=(cmd)` etc.), `powershell` (Windows, `cmdlet | Where-Object`). A shell oferece um parser comum + afinadores por dialeto, sem virar três shells separados.
- Termios já cobre `TCGETS/TCSETS/TIOCGWINSZ` para `isatty()` do bash/zsh.
