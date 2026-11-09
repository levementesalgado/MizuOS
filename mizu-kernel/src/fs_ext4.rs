// ext4 mínimo em Rust puro (read-only, sem journal, sem extents profundos).
// Lágrimas sobre o ZFS: amor à primeira vista, mas portar OpenZFS (C, CDDL, pool)
// em um kernel solo seria outro OS. Ext4 é o caminho honesto agora, com VFS fina
// para depois plugar ext4 completo ou até um módulo ZFS.
// Formato: superblock em 1024 bytes, grupo de blocos, inode 256 bytes, extent header.
// Suporta só INCOMPAT_EXTENTS com profundidade 0 (sem árvore, sem journal replay).
//
// O que entra nesta fase:
// - superblock + try_mount (já existia)
// - group descriptor (32 bytes, sem 64bit) e inode_offset
// - leitura do diretório raiz (inode 2) via extents profundidade 0
// - listagem de entradas (dirent linear) via serial
//
// A avaliação do transpilar (c2rust) fica em docs/linux-abi-rust.md:
// ext4 completo em C tem 50-100k linhas + jbd2/mballoc; c2rust geraria
// dezenas de milhares de linhas unsafe para auditar, e ainda depende de
// APIs do kernel Linux (bio, vfs, page cache) que não existem aqui.

use crate::ata;
use crate::serial_driver;
use alloc::format;
use alloc::vec::Vec;

const SB_OFFSET_SECTORS: u32 = 2; // 1024 / 512
const MAGIC: u16 = 0xEF53;
const EXT4_EXTENTS_FLG: u32 = 0x40;
const S_IFDIR: u16 = 0x4000;

fn rd_u16(b: &[u8], off: usize) -> u16 {
    (b[off] as u16) | ((b[off + 1] as u16) << 8)
}

fn rd_u32(b: &[u8], off: usize) -> u32 {
    (b[off] as u32)
        | ((b[off + 1] as u32) << 8)
        | ((b[off + 2] as u32) << 16)
        | ((b[off + 3] as u32) << 24)
}

#[derive(Clone)]
pub struct Mount {
    pub block_size: u32,
    pub blocks_count: u32,
    pub inodes_count: u32,
    pub inodes_per_group: u32,
    pub inode_size: u16,
    pub has_extents: bool,
}

impl Mount {
    fn sectors_per_block(&self) -> u32 {
        self.block_size / 512
    }

    fn block_lba(&self, block: u32) -> u32 {
        block * self.sectors_per_block()
    }

    fn read_block(&self, block: u32) -> Option<Vec<u8>> {
        let mut buf = Vec::with_capacity(self.block_size as usize);
        buf.resize(self.block_size as usize, 0);
        // Leitura setor a setor: mais confiável no PIO do QEMU que 1 comando para N setores.
        let lba = self.block_lba(block);
        for i in 0..self.sectors_per_block() {
            if !ata::read_sectors(lba + i, 1, unsafe { buf.as_mut_ptr().add((i as usize) * 512) }) {
                return None;
            }
        }
        Some(buf)
    }

    // Group 0 descriptors: 1K -> bloco 2, senão bloco 1 (32 bytes por grupo).
    fn group0_desc(&self) -> Option<(u32, u32, u32)> {
        let blk = if self.block_size == 1024 { 2 } else { 1 };
        let data = self.read_block(blk)?;
        // bg_block_bitmap_lo=0, bg_inode_bitmap_lo=4, bg_inode_table_lo=8
        Some((rd_u32(&data, 0), rd_u32(&data, 4), rd_u32(&data, 8)))
    }

    // Retorna (mode, size_lo, i_block[60]) do inode ino.
    fn read_inode(&self, ino: u32) -> Option<(u16, u32, [u8; 60])> {
        if ino == 0 || self.inodes_per_group == 0 { return None; }
        let group = (ino - 1) / self.inodes_per_group;
        if group != 0 { return None; } // só suporte a grupo 0 por enquanto
        let (bm, im, table_blk) = self.group0_desc()?;
        let idx = (ino - 1) * self.inode_size as u32;
        let blk = table_blk + idx / self.block_size;
        let off = (idx % self.block_size) as usize;
        let data = self.read_block(blk)?;
        if off + 60 > data.len() { return None; }
        let mode = rd_u16(&data, off);
        let size_lo = rd_u32(&data, off + 4);
        let mut i_block = [0u8; 60];
        i_block.copy_from_slice(&data[off + 40..off + 100]);
        Some((mode, size_lo, i_block))
    }

    // Lê dados via extents (profundidade 0: inline; >0: árvore de blocos).
    fn read_via_extents(&self, i_block: &[u8; 60], size: u32) -> Vec<u8> {
        if rd_u16(i_block, 0) != 0xF30A { return Vec::new(); }
        let depth = rd_u16(i_block, 6) as usize;
        let entries = rd_u16(i_block, 2) as usize;
        if depth == 0 {
            return self.read_leaf(i_block, size, 12);
        }
        // Profundidade >0: nós de índice apontando para blocos de extent.
        let mut out = Vec::new();
        for e in 0..entries {
            let base = 12 + e * 12;
            if base + 12 > 60 { break; }
            let leaf_blk = rd_u32(i_block, base + 8);
            out.extend_from_slice(&self.read_extent_tree(leaf_blk, depth - 1, size, 0));
            if out.len() >= size as usize { out.truncate(size as usize); break; }
        }
        out
    }

    fn read_leaf(&self, node: &[u8], size: u32, header_size: usize) -> Vec<u8> {
        let mut out = Vec::new();
        if rd_u16(node, 0) != 0xF30A { return out; }
        let entries = rd_u16(node, 2) as usize;
        let mut block_off: u32 = 0;
        for e in 0..entries {
            let base = header_size + e * 12;
            if base + 12 > node.len() { break; }
            let ee_block = rd_u32(node, base);
            let ee_len = rd_u16(node, base + 4) as u32;
            let ee_start = rd_u32(node, base + 8);
            if ee_block != block_off { break; }
            if let Some(data) = self.read_block(ee_start) {
                let already = (block_off * self.block_size) as usize;
                if already >= size as usize { break; }
                let take = core::cmp::min((ee_len * self.block_size) as usize, (size as usize) - already);
                out.extend_from_slice(&data[..take]);
            }
            block_off = block_off.wrapping_add(ee_len);
        }
        out
    }

    fn read_extent_tree(&self, block: u32, depth: usize, size: u32, file_block_off: u32) -> Vec<u8> {
        let Some(node) = self.read_block(block) else { return Vec::new() };
        if node.len() < 12 || rd_u16(&node, 0) != 0xF30A { return Vec::new(); }
        let entries = rd_u16(&node, 2) as usize;
        if depth == 0 {
            return self.read_leaf(&node, size, 12);
        }
        let mut out = Vec::new();
        for e in 0..entries {
            let base = 12 + e * 12;
            if base + 12 > node.len() { break; }
            let child = rd_u32(&node, base + 8);
            out.extend_from_slice(&self.read_extent_tree(child, depth - 1, size, file_block_off));
            if out.len() >= size as usize { out.truncate(size as usize); break; }
        }
        out
    }

    fn dir_data(&self, ino: u32) -> Option<Vec<u8>> {
        let (mode, size, i_block) = self.read_inode(ino)?;
        if mode & 0xF000 != S_IFDIR { return None; }
        Some(self.read_via_extents(&i_block, size))
    }

    fn find_in_dir(&self, dir_ino: u32, name: &str) -> Option<u32> {
        let data = self.dir_data(dir_ino)?;
        let mut off = 0usize;
        while off + 8 <= data.len() {
            let ino = rd_u32(&data, off);
            let rec_len = rd_u16(&data, off + 4) as usize;
            if rec_len < 8 || off + rec_len > data.len() { break; }
            let nlen = data[off + 6] as usize;
            if ino != 0 && nlen > 0 && off + 8 + nlen <= data.len() {
                if &data[off + 8..off + 8 + nlen] == name.as_bytes() {
                    return Some(ino);
                }
            }
            off += rec_len;
        }
        None
    }

    // Caminho absoluto "/a/b/c" ou relativo "a/b" a partir do root (ino 2).
    pub fn lookup(&self, path: &str) -> Option<u32> {
        let mut cur: u32 = 2;
        for comp in path.split('/').filter(|s| !s.is_empty()) {
            if comp == "." { continue; }
            if comp == ".." { continue; } // sem pai real por enquanto (root é /)
            cur = self.find_in_dir(cur, comp)?;
        }
        Some(cur)
    }

    pub fn find_in_root(&self, name: &str) -> Option<u32> {
        self.find_in_dir(2, name)
    }

    pub fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        // Aceita "hello.txt" ou "/hello.txt" ou "/dir/hello.txt".
        let path = if name.starts_with('/') { name } else { alloc::format!("/{}", name).leak() as &str };
        let ino = self.lookup(path)?;
        let (_, size, i_block) = self.read_inode(ino)?;
        Some(self.read_via_extents(&i_block, size))
    }

    pub fn list_root(&self) {
        let Some((mode, size, i_block)) = self.read_inode(2) else { return };
        if mode & 0xF000 != S_IFDIR {
            serial_driver::sprintln!("  ext4: inode 2 não é diretório (mode={:#x})", mode);
            return;
        }
        let data = self.read_via_extents(&i_block, size);
        let mut off = 0usize;
        let mut count = 0;
        while off + 8 <= data.len() && count < 16 {
            let ino = rd_u32(&data, off);
            let rec_len = rd_u16(&data, off + 4) as usize;
            if rec_len < 8 || off + rec_len > data.len() { break; }
            let name_len = data[off + 6] as usize;
            if ino != 0 && name_len > 0 && off + 8 + name_len <= data.len() {
                let name = core::str::from_utf8(&data[off + 8..off + 8 + name_len]).unwrap_or("?");
                serial_driver::sprintln!("  ext4: /{} (ino {}, {} bytes)", name, ino, rec_len);
                count += 1;
            }
            off += rec_len;
        }
        if count == 0 {
            serial_driver::sprintln!("  ext4: / (vazio ou sem extents depth 0)");
        }
    }
}

// Tenta ler o superblock do disco primário master. None = sem disco / sem ext4.
pub fn try_mount() -> Option<Mount> {
    let mut buf = Vec::with_capacity(1024);
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
        inode_size: rd_u16(sb, 88),
        has_extents: rd_u32(sb, 96) & EXT4_EXTENTS_FLG != 0,
    };
    serial_driver::sprintln!(
        "  ext4: mount ok block={} blocks={} inodes={}/grp {} inode_size={} extents={}",
        m.block_size, m.blocks_count, m.inodes_count, m.inodes_per_group, m.inode_size, m.has_extents
    );
    m.list_root();
    Some(m)
}

// Esqueleto de leitura futura: dado o inode, seguir extent header.
// Profundidade 0 = header + até 4 extents inline em i_block (60 bytes).
pub fn extent0_check(inode_block: &[u8]) -> bool {
    if inode_block.len() < 60 { return false; }
    rd_u16(inode_block, 0) == 0xF30A // eh_magic
}

// Mount global para VFS: process.rs consulta sem precisar do Mount no caller.
static mut MOUNT: Option<Mount> = None;

pub fn mount() {
    unsafe { MOUNT = try_mount(); }
}

pub fn file_exists_ext4(name: &str) -> bool {
    unsafe { MOUNT.as_ref().map(|m| m.find_in_root(name).is_some()).unwrap_or(false) }
}

pub fn read_file_ext4(name: &str) -> Option<Vec<u8>> {
    unsafe { MOUNT.as_ref().and_then(|m| m.read_file(name)) }
}
