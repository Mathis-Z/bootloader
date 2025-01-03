use core::{arch::asm, mem::size_of, slice};

use crate::mem::*;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct GdtEntry {
    limit_lower: u16,
    base_lower: u16,
    base_middle: u8,
    access_flags: u8,
    limit_higher_and_flags: u8,
    base_higher: u8,
}

#[repr(C, packed)]
struct Gdtr {
    limit: u16,
    base: u64,
}

impl GdtEntry {
    pub fn set_limit(&mut self, limit: u32) {
        self.limit_lower = limit as u16;
        let flags = self.limit_higher_and_flags & 0xF0;
        let upper_limit_bits: u8 = ((limit >> 16) & 0xF) as u8; // only 4 bits
        self.limit_higher_and_flags = flags | upper_limit_bits;
    }

    pub fn set_base(&mut self, base: u32) {
        self.base_lower = base as u16;
        self.base_middle = (base >> 16) as u8;
        self.base_higher = (base >> 24) as u8;
    }

    // only lower 4 bits of flags should be set
    pub fn set_flags(&mut self, flags: u8) {
        self.limit_higher_and_flags &= 0x0F;
        self.limit_higher_and_flags |= flags << 4;
    }

    pub fn set_access_flags(&mut self, flags: u8) {
        self.access_flags = flags;
    }
}

pub fn allocate_page_for_gdt() -> usize {
    allocate_low_pages(1)
}

pub fn create_and_set_simple_gdt(page_addr: usize) {
    const GDT_ENTRY_BOOT_CS: usize = 2;
    const GDT_ENTRY_BOOT_DS: usize = 3;

    const ACCESS_BYTE_A: u8 = 0b1;
    const ACCESS_BYTE_RW: u8 = 0b10;
    const _ACCESS_BYTE_DC: u8 = 0b100;
    const ACCESS_BYTE_E: u8 = 0b1000;
    const ACCESS_BYTE_S: u8 = 0b10000;
    const ACCESS_BYTE_P: u8 = 0b10000000;

    const FLAGS_G: u8 = 0b1000;
    const FLAGS_DB: u8 = 0b100;
    const FLAGS_L: u8 = 0b10;

    unsafe {
        let gdt_ptr = page_addr as *mut GdtEntry;
        let gdt = slice::from_raw_parts_mut(gdt_ptr, 10);

        gdt[GDT_ENTRY_BOOT_CS].set_base(0);
        gdt[GDT_ENTRY_BOOT_CS].set_limit(u32::MAX);
        gdt[GDT_ENTRY_BOOT_CS].set_access_flags(
            ACCESS_BYTE_P | ACCESS_BYTE_S | ACCESS_BYTE_E | ACCESS_BYTE_RW | ACCESS_BYTE_A,
        );
        gdt[GDT_ENTRY_BOOT_CS].set_flags(FLAGS_G | FLAGS_L);

        gdt[GDT_ENTRY_BOOT_DS].set_base(0);
        gdt[GDT_ENTRY_BOOT_DS].set_limit(u32::MAX);
        gdt[GDT_ENTRY_BOOT_DS]
            .set_access_flags(ACCESS_BYTE_P | ACCESS_BYTE_S | ACCESS_BYTE_RW | ACCESS_BYTE_A);
        gdt[GDT_ENTRY_BOOT_DS].set_flags(FLAGS_G | FLAGS_DB);

        let gdtr = Gdtr {
            limit: (size_of::<GdtEntry>() * 8) as u16,
            base: gdt_ptr as u64,
        };

        asm!(
            r#"
            lgdt [{}]
            nop
            nop
            "#,
            in(reg) &gdtr,
        );

        set_cs((GDT_ENTRY_BOOT_CS * 8) as usize);

        asm!(
            r#"
            nop
            nop
            mov rax, {}
            mov ds, ax
            mov es, ax
            mov ss, ax
            "#,
            in(reg) GDT_ENTRY_BOOT_DS * 8,
        );
    }
}

// CS cannot be set directly but only indirectly with a far return...
unsafe fn set_cs(cs: usize) {
    unsafe {
        asm!(
            "push {sel}",
            "lea {tmp}, [6 + rip]",
            "push {tmp}",
            "retfq",
            "nop",
            "nop",
            "nop",
            "nop",
            "nop",
            "nop",
            sel = in(reg) cs,
            tmp = lateout(reg) _,
            options(preserves_flags),
        );
    }
}
