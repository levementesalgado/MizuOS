# Mizu OS + Hako

Kernel i686 bare-metal com o transpilador Hako embutido como linguagem de driver de primeiro nível.

## Estrutura

```
mizu-kernel/          → kernel i686 (Rust + assembly)
  src/
    main.rs           → entry point (kmain)
    vga_driver.rs     → driver VGA modo texto (80×25)
    serial_driver.rs  → driver serial COM1
    keyboard.rs       → driver PS/2 com buffer circular
    shell.rs          → shell interativo (12 comandos)
    interrupts.rs     → GDT, IDT, PIC, PIT
    memory.rs         → alocador de frames bitmap + heap free-list
    fs.rs             → initramfs tar parser
    arch/i686/
      boot.asm        → multiboot header + stack setup
      interrupts.asm  → GDT/IDT assembly, stubs de interrupção
      link.ld         → linker script (GRUB-compliant)
    src/hako/         → fontes Hako transpilados em build.rs
hako/                 → transpilador Hako → Rust
  src/
    ast.rs            → tipos da AST
    parser.rs         → parser recursivo descendo
    codegen.rs        → gerador de código Rust
    stdlib.rs         → stdlib predefinida (serial, VGA, PIT, teclado, port I/O)
    lib.rs            → API pública (transpile_file)
    main.rs           → CLI
```

## Build

```bash
# Hako (standalone)
cargo run -p hako -- input.hako -o output.rs

# Kernel + Hako
cd mizu-kernel
cargo +nightly build -Zjson-target-spec -Zbuild-std-features=compiler-builtins-mem \
  --target i686-mizu.json --release

# ISO
cp target/i686-mizu/release/mizu-kernel /tmp/mizu-iso/boot/mizu.bin
grub-mkrescue -o /tmp/mizu.iso /tmp/mizu-iso

# QEMU
qemu-system-x86_64 -cdrom /tmp/mizu.iso -m 128M -no-reboot -nographic
```

## Status

- Kernel boota (GRUB → protected mode → kmain)
- GDT, IDT, PIC, PIT configurados
- Drivers: VGA texto, serial COM1, PS/2 keyboard (polling)
- Heap allocator, frame allocator, initramfs
- Shell com 12 comandos
- Hako demo roda na inicialização (serial + VGA)
- Interrupções desabilitadas (sti causa hang — PIT IRQ0 pendente)
