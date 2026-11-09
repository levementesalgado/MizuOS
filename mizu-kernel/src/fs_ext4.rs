// ext4 mínimo em Rust puro (read-only, sem journal, sem extents profundos).
// Lágrimas sobre o ZFS: amor à primeira vista, mas portar OpenZFS (C, CDDL, pool)
// em um kernel solo seria outro OS. Ext4 é o caminho honesto agora, com VFS fina
// para depois plugar ext4 completo ou até um módulo ZFS.
// Formato: superblock em 1024 bytes, grupo de blocos, inode 256 bytes, extent header.
// Suporta só INCOMPAT_EXTENTS com profundidade 0 (sem árvore, sem journal replay).

use crate::ata;
use crate::serial_driver;

const SB_OFFSET_SECTORS: u32 = 2; // 1024 / 512
const MAGIC: u16 = 0xEF53;
const EXT4_EXTENTS_FLG: u32 = 0x40;

fn rd_u16(b: &[u8], off: usize) -> u16 {
    (b[off] as u16) | ((b[off + 1] as u16) << 8)
}

fn rd_u32(b: &[u8], off: usize) -> u32 {
    (b[off] as u32)
        | ((b[off + 1] as u32) << 8)
        | ((b[off + 2] as u32) << 16)
        | ((b[off + 3] as u32) << 24)
}

pub struct Mount {
    pub block_size: u32,
    pub blocks_count: u32,
    pub inodes_count: u32,
    pub inodes_per_group: u32,
    pub inode_size: u16,
    pub has_extents: bool,
}

// Tenta ler o superblock do disco primário master. None = sem disco / sem ext4.
pub fn try_mount() -> Option<Mount> {
    let mut buf = alloc::vec::Vec::<u8>::with_capacity(1024);
    buf.resize(1024, 0);
    if !ata::read_sectors(SB_OFFSET_SECTORS, 2, buf.as_mut_ptr()) {
        return None;
    }
    let sb = &buf;
    if rd_u16(sb, 0x38) != MAGIC { // s_magic em offset 56 (0x38)
        return None;
    }
    let block_size = 1024u32 << rd_u32(sb, 0x18);
    let m = Mount {
        block_size,
        blocks_count: rd_u32(sb, 0x04),
        inodes_count: rd_u32(sb, 0x00),
        inodes_per_group: rd_u32(sb, 0x28),
        inode_size: rd_u16(sb, 0x3A),
        has_extents: rd_u32(sb, 0x5C) & EXT4_EXTENTS_FLG != 0,
    };
    serial_driver::sprintln!(
        "  ext4: mount ok block={} blocks={} inodes={}/grp {} inode_size={} extents={}",
        m.block_size, m.blocks_count, m.inodes_count, m.inodes_per_group, m.inode_size, m.has_extents
    );
    Some(m)
}

// Esqueleto de leitura futura: dado o inode, seguir extent header.
// Profundidade 0 = header + até 4 extents inline em i_block (60 bytes).
pub fn extent0_check(inode_block: &[u8]) -> bool {
    if inode_block.len() < 60 { return false; }
    rd_u16(inode_block, 0) == 0xF30A // eh_magic
}
