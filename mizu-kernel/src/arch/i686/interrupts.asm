.intel_syntax noprefix

/* ─── GDT (agora definida em Rust) ───────────── */
.section .text
.globl gdt_reload
gdt_reload:
    mov eax, [esp + 4]    /* gdt_ptr address */
    lgdt [eax]
    /* far jump to reload CS (0x08 = kernel code) */
    push 0x08
    push offset .reload_cs
    retf
.reload_cs:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    ret

/* ─── TSS ────────────────────────────────────── */
.globl tss_load
tss_load:
    mov ax, [esp + 4]    /* TSS selector */
    ltr ax
    ret

/* ─── IDT ────────────────────────────────────── */
.section .bss
.align 8
idt:
    .space 256 * 8

.section .rodata
.align 4
idt_ptr:
    .word (256 * 8) - 1
    .long idt

.section .text
.globl idt_load
idt_load:
    lidt [idt_ptr]
    ret

/* idt_set_entry(entry_num, base_addr, selector, flags) */
.globl idt_set_entry
idt_set_entry:
    push ebp
    mov ebp, esp
    push ebx

    mov eax, [ebp + 8]    /* entry_num */
    mov ebx, [ebp + 12]   /* base_addr (handler) */
    mov ecx, [ebp + 16]   /* selector */
    mov edx, [ebp + 20]   /* flags */

    shl eax, 3            /* entry_num * 8 */
    add eax, offset idt

    mov [eax], bx         /* offset low 16 bits */
    mov [eax + 2], cx     /* selector */
    mov byte ptr [eax + 4], 0  /* reserved */
    mov [eax + 5], dl     /* access byte (P, DPL, gate type) */
    shr ebx, 16
    mov [eax + 6], bx     /* offset high 16 bits */

    pop ebx
    pop ebp
    ret

/* ─── Interrupt stubs ────────────────────────── */
/* Normal ISR: push ISR number, call Rust handler.
   O handler retorna new_ESP (0 = sem troca) para preempção via PIT. */
isr_common:
    pusha
    push ds
    push es
    push fs
    push gs

    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    cld
    /* Read all values first, then push to avoid offset drift */
    mov eax, [esp + 48]    /* ISR number */
    mov ebx, [esp + 52]    /* error code */
    mov ecx, [esp + 56]    /* EIP */
    mov edx, [esp + 60]    /* CS */
    lea esi, [esp]         /* saved_esp (gs) */
    push esi               /* 5th arg: saved_esp */
    push edx               /* 4th arg: CS */
    push ecx               /* 3rd arg: EIP */
    push ebx               /* 2nd arg: error code */
    push eax               /* 1st arg: ISR number */
    call interrupt_handler
    add esp, 20

    /* eax = new_esp (0 = sem troca, como no syscall_handler) */
    test eax, eax
    jz .isr_restore
    mov esp, eax

.isr_restore:
    pop gs
    pop fs
    pop es
    pop ds
    popa
    add esp, 8
    iretd

/* Syscall handler (int 0x80, ring 3 → ring 0) */
/* When entered: SS, ESP, EFLAGS, CS, EIP, error-code on kernel stack */
.globl syscall_handler
syscall_handler:
    pusha
    push ds
    push es
    push fs
    push gs

    mov ax, 0x10           /* kernel data segment */
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    cld
    /* saved_regs pointer + syscall_num */
    lea eax, [esp]         /* pointer to GS push (top of stack frame) */
    push eax               /* &saved_regs */
    mov eax, [esp + 48]    /* pusha EAX at offset 48 from current ESP */
    push eax               /* syscall_num */
    call rust_handle_syscall
    add esp, 8

    /* eax = return value. Store in pusha EAX slot */
    /* offsets from ESP: gs=0, fs=4, es=8, ds=12, edi=16, esi=20, ebp=24, esp=28, ebx=32, edx=36, ecx=40, eax=44 */
    mov [esp + 44], eax

    /* Check for context switch */
    lea eax, [esp]         /* current ESP pointing to gs */
    push eax               /* pass ESP as argument */
    call context_switch_handler
    add esp, 4             /* pop argument */
    test eax, eax          /* 0 = no switch */
    jz .restore
    mov esp, eax           /* switch to new process's stack */

.restore:
    pop gs
    pop fs
    pop es
    pop ds
    popa
    iretd

/* Exceptions with error codes: 8, 10, 11, 12, 13, 14, 17, 21 */
.macro isr_err n
.globl isr\n
isr\n:
    push \n
    jmp isr_common
.endm

.macro isr_noerr n
.globl isr\n
isr\n:
    push 0
    push \n
    jmp isr_common
.endm

    isr_noerr 0
    isr_noerr 1
    isr_noerr 2
    isr_noerr 3
    isr_noerr 4
    isr_noerr 5
    isr_noerr 6
    isr_noerr 7
    isr_err   8
    isr_noerr 9
    isr_err   10
    isr_err   11
    isr_err   12
    isr_err   13
    isr_err   14
    isr_noerr 15
    isr_noerr 16
    isr_err   17
    isr_noerr 18
    isr_noerr 19
    isr_noerr 20
    isr_err   21
    isr_noerr 22
    isr_noerr 23
    isr_noerr 24
    isr_noerr 25
    isr_noerr 26
    isr_noerr 27
    isr_noerr 28
    isr_noerr 29
    isr_err   30
    isr_noerr 31
    isr_noerr 32  /* IRQ0 (PIT) */
    isr_noerr 33  /* IRQ1 (keyboard) */
    isr_noerr 34
    isr_noerr 35
    isr_noerr 36
    isr_noerr 37
    isr_noerr 38
    isr_noerr 39
    isr_noerr 40
    isr_noerr 41
    isr_noerr 42
    isr_noerr 43
    isr_noerr 44
    isr_noerr 45
    isr_noerr 46
    isr_noerr 47
    isr_noerr 128 /* Linux syscall (int 0x80) */

.globl isr_stubs
isr_stubs:
    .long isr0, isr1, isr2, isr3, isr4, isr5, isr6, isr7
    .long isr8, isr9, isr10, isr11, isr12, isr13, isr14, isr15
    .long isr16, isr17, isr18, isr19, isr20, isr21, isr22, isr23
    .long isr24, isr25, isr26, isr27, isr28, isr29, isr30, isr31
    .long isr32, isr33, isr34, isr35, isr36, isr37, isr38, isr39
    .long isr40, isr41, isr42, isr43, isr44, isr45, isr46, isr47
    .long syscall_handler   /* isr 48 = syscall handler for int 0x80 */

.globl syscall_isr_index
syscall_isr_index:
    .long 48               /* index in isr_stubs for int 0x80 handler */
