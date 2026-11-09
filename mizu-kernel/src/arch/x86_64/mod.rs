// x86_64 skeleton para compatibilidade 32+64 no mesmo microkernel.
//
// Modo longo (PML4), MSR LSTAR, FS.base para TLS, e um loader ELF64
// (PT_LOAD + aux vector) já existem no TipOS em C/Zig (`elf64.zig`,
// `syscall_linux.zig`). Aqui tudo é Rust puro e o dispatch é por
// EI_CLASS no personality/Linux:
//
//   EI_CLASS=1 (ELF32) -> personality/linux (i386, int 0x80, read=3/write=4)
//   EI_CLASS=2 (ELF64) -> personality/linux64 (x86_64, syscall insn, read=0/write=1)
//
// Hoje i686 é o único backend real. Este módulo fica vazio de propósito
// até o passo openat/fstatat + TLS estar verde em QEMU.

pub(crate) fn todo() {}
