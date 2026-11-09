// ATA PIO (LBA28) mínimo para ler disco primário master.
// Sem DMA, sem IRQ, só polling BSY/DRQ. É o suficiente para ler o superblock ext4.
use core::arch::asm;

#[inline(always)]
fn outb(port: u16, val: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack)); }
}

#[inline(always)]
fn inw(port: u16) -> u16 {
    let val: u16;
    unsafe { asm!("in ax, dx", out("ax") val, in("dx") port, options(nomem, nostack)); }
    val
}

#[inline(always)]
fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe { asm!("in al, dx", out("al") val, in("dx") port, options(nomem, nostack)); }
    val
}

fn wait_bsy() {
    for _ in 0..100000 {
        if inb(0x1F7) & 0x80 == 0 { return; }
    }
}

fn wait_drq() -> bool {
    for _ in 0..100000 {
        let s = inb(0x1F7);
        if s & 0x80 == 0 {
            if s & 0x01 != 0 { return false; } // ERR
            if s & 0x08 != 0 { return true; } // DRQ
        }
    }
    false
}

pub fn read_sectors(lba: u32, count: u8, buf: *mut u8) -> bool {
    wait_bsy();
    // Drive: master (0xE0), LBA mode, bits 24-27 do LBA
    outb(0x1F6, 0xE0 | ((lba >> 24) as u8 & 0x0F));
    outb(0x1F2, count);
    outb(0x1F3, (lba & 0xFF) as u8);
    outb(0x1F4, ((lba >> 8) & 0xFF) as u8);
    outb(0x1F5, ((lba >> 16) & 0xFF) as u8);
    outb(0x1F7, 0x20); // READ SECTORS
    if !wait_drq() { return false; }
    let mut p = buf;
    for _ in 0..(count as usize) {
        for _i in 0..256 {
            let w = inw(0x1F0);
            unsafe {
                *p = (w & 0xFF) as u8;
                p = p.add(1);
                *p = ((w >> 8) & 0xFF) as u8;
                p = p.add(1);
            }
        }
    }
    // Delay 400ns via alternate status
    let _s = inb(0x3F6);
    true
}
