// Hako stdlib source — injected as a module into generated output
pub const STDLIB: &str = r##"
// --- Port I/O ---
fn port_outb(value: u32, port: u32) {
    unsafe { core::arch::asm!("out dx, al", in("dx") port as u16, in("al") value as u8); }
}

fn port_inb(port: u32) -> u32 {
    let result: u8;
    unsafe { core::arch::asm!("in al, dx", in("dx") port as u16, out("al") result); }
    result as u32
}

// --- Serial (COM) ---
pub fn serial_config(com: u32) {
    port_outb(0x80, com + 3);
    port_outb(1, com);
    port_outb(0, com + 1);
    port_outb(3, com + 3);
}

pub fn serial_write_byte(b: u32, com: u32) {
    loop {
        if port_inb(com + 5) & 0x20 != 0 { break; }
    }
    port_outb(b, com);
}

pub fn serial_read_byte(com: u32) -> u32 {
    loop {
        if port_inb(com + 5) & 1 != 0 { break; }
    }
    port_inb(com)
}

// --- VGA text mode ---
pub fn vga_put_char(c: u32, x: u32, y: u32) {
    let pos = x + y * 80;
    let addr = 0xB8000 + pos * 2;
    unsafe {
        core::ptr::write_volatile(addr as *mut u8, c as u8);
        core::ptr::write_volatile((addr + 1) as *mut u8, 0x07);
    }
}

pub fn vga_write_str(s: &str) {
    let mut x = 0u32;
    let mut y = 0u32;
    for c in s.bytes() {
        if c == b'\n' { x = 0; y += 1; continue; }
        vga_put_char(c as u32, x, y);
        x += 1;
        if x >= 80 { x = 0; y += 1; }
    }
}

pub fn vga_clear() {
    unsafe {
        let buf = 0xB8000 as *mut u16;
        for i in 0..(80 * 25) {
            core::ptr::write_volatile(buf.add(i), 0x0720);
        }
    }
}

pub fn vga_scroll() {
    unsafe {
        core::ptr::copy(0xB8000 as *const u16, 0xB8000 as *mut u16, 80 * 24);
        let buf = 0xB8000 as *mut u16;
        for i in (80 * 24)..(80 * 25) {
            core::ptr::write_volatile(buf.add(i), 0x0720);
        }
    }
}

pub fn vga_set_cursor(x: u32, y: u32) {
    let pos = x + y * 80;
    port_outb(14, 0x3D4);
    port_outb((pos >> 8) as u32, 0x3D5);
    port_outb(15, 0x3D4);
    port_outb(pos as u32, 0x3D5);
}

// --- PIT ---
pub fn pit_config(freq: u32) {
    let divisor = 1193180u32 / freq;
    port_outb(0x36, 0x43);
    port_outb(divisor & 0xFF, 0x40);
    port_outb((divisor >> 8) & 0xFF, 0x40);
}

// --- Keyboard ---
pub fn keyboard_init() {
    port_outb(0xAE, 0x64);
    port_outb(0xF3, 0x60);
    port_outb(0x00, 0x60);
}
"##;
