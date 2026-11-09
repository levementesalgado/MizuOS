#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::alloc::Layout;
use core::panic::PanicInfo;

#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    panic!("out of memory");
}

mod vga_driver;
mod serial_driver;
mod keyboard;
mod shell;
mod interrupts;
mod pic;
mod pit;
mod memory;
mod fs;
mod ata;
mod fs_ext4;
mod personality;
mod process;
mod syscall;
mod arch;

macro_rules! include_hako {
    () => {
        include!(concat!(env!("OUT_DIR"), "/hako_gen/mod.rs"));
    };
}
include_hako!();

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    vga_driver::write_str("\n*** KERNEL PANIC ***\n");
    serial_driver::write_str("*** KERNEL PANIC ***\n");
    loop { unsafe { core::arch::asm!("cli; hlt"); } }
}

fn multiboot_mem_upper(info: u32) -> u32 {
    let ptr = info as *const u32;
    let flags = unsafe { *ptr };
    if flags & 1 == 0 {
        return 128 * 1024;
    }
    unsafe { *ptr.add(2) }
}

#[no_mangle]
pub extern "C" fn kmain(_magic: u32, info: u32) -> ! {
    serial_driver::init();
    vga_driver::clear();
    vga_driver::set_cursor_visible(true);

    kprintln!("☯ Mizu Kernel v0.1.0");

    let mem_upper = multiboot_mem_upper(info);
    memory::init(mem_upper);
    fs::init();

    // Sonda ext4 (somente leitura) no disco primário, se houver.
    // Sem disco, continua só com initramfs — nada quebra.
    let _ext4 = crate::fs_ext4::try_mount();

    arch::gdt::init(0x11B590);
    interrupts::init();
    crate::sys::pic_init();
    crate::sys::pit_init(100);

    kprintln!("  pic/pit: OK (masked, sti deferred)");

    keyboard::init();
    // NOTE: sti intentionally deferred until after paging + process setup.
    // Enabling IRQ0 timer before TSS/paging/scheduler are ready causes a
    // spurious PIT interrupt to fire into a half-initialized kernel stack
    // (documented hang). Early boot uses serial + VGA, keyboard polling works
    // without IRQ1 for the first prompts.

    use alloc::vec::Vec;
    let mut v = Vec::new();
    v.push(42u32);
    v.push(7u32);
    kprintln!("  heap test: vec.len={}, vec[0]={}", v.len(), v[0]);

    let (tf, uf, ff) = memory::stats();
    kprintln!("  frames: total={}, used={}, free={}", tf, uf, ff);

    // Enable paging: identity-map kernel + heap, then set PG bit
    kprintln!("  paging: enabling...");
    let kpd_phys: u32;
    unsafe {
        let mut kpd = arch::i686::paging::PageDir::new().unwrap();
        kpd.identity_map_kernel();
        kpd_phys = kpd.phys_addr();
        serial_driver::sprintln!("  paging: pd at {:#x}", kpd_phys);
        core::arch::asm!("mov cr3, {0}", in(reg) kpd_phys);
        let cr0: u32;
        core::arch::asm!("mov {0}, cr0", out(reg) cr0);
        core::arch::asm!("mov cr0, {0}", in(reg) cr0 | 0x80000000);
    }
    kprintln!("  paging: enabled");

    kprintln!("  hako: running...");
    flow_default();
    kprintln!("  hako: OK");

    // Initialize process module
    process::init(kpd_phys);

    // Set up PID 1 (shell) context with its own kernel stack
    process::setup_pid1(shell::shell_loop as usize);

    // Load init.elf as the user-space init process (PID 2)
    if let Some(elf_data) = fs::file_read("init.elf") {
        kprintln!("  main: loading init.elf...");
        match process::create_user(elf_data) {
            Ok(pid) => {
                serial_driver::sprintln!("  main: created PID {} from init.elf", pid);
            }
            Err(()) => {
                serial_driver::sprintln!("  main: failed to create user process from init.elf");
            }
        }
    } else {
        serial_driver::sprintln!("  main: init.elf not found in filesystem");
    }

    // Enter scheduler — picks first ready process and runs it (never returns)
    // Enable hardware interrupts only now that GDT/IDT/PIC/PIT, paging,
    // process table and TSS.esp0 are fully initialized.
    kprintln!("  main: enabling interrupts (sti late)...");
    crate::sys::sti();
    kprintln!("  interrupts: enabled");
    kprintln!("  main: entering scheduler...");
    process::scheduler_enter()
}
