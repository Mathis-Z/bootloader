extern crate alloc;

use alloc::vec::Vec;

use crate::{mem::allocate_low_pages, simple_error::SimpleResult};

use super::simple_error;

// https://github.com/torvalds/linux/blob/v4.16/Documentation/x86/boot.txt
#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct KernelHeader {
    pub setup_sects: u8,
    pub root_flags: u16,
    pub syssize: u32,
    pub ram_size: u16,
    pub vid_mode: u16,
    pub root_dev: u16,
    pub boot_flag: u16,
    pub jump: u16,
    pub header: u32,
    pub version: u16,
    pub realmode_switch: u32,
    pub start_sys_seg: u16,
    pub kernel_version: u16,
    pub type_of_loader: u8,
    pub load_flags: u8,
    pub setup_move_size: u16,
    pub code32_start: u32,
    pub ramdisk_image: u32,
    pub ramdisk_size: u32,
    pub bootsect_kludge: u32,
    pub heap_end_ptr: u16,
    pub ext_loader_ver: u8,
    pub ext_loader_type: u8,
    pub cmd_line_ptr: u32,
    pub initrd_addr_max: u32,
    pub kernel_alignment: u32,
    pub relocatable_kernel: u8,
    pub min_alignment: u8,
    pub xloadflags: u16,
    pub cmdline_size: u32,
    pub hardware_subarch: u32,
    pub hardware_subarch_data: u64,
    pub payload_offset: u32,
    pub payload_length: u32,
    pub setup_data: u64,
    pub pref_address: u64,
    pub init_size: u32,
    pub handover_offset: u32,
}

// https://github.com/torvalds/linux/blob/0adb32858b0bddf4ada5f364a84ed60b196dbcda/arch/x86/include/uapi/asm/bootparam.h#L152
// We don't need to set all of these
#[repr(C, packed)]
pub struct BootParams {
    pub screen_info: ScreenInfo,
    pub apm_bios_info: [u8; 0x14],
    _pad0: u32,
    pub tboot_addr: u64,
    pub ist_info: [u8; 0x10],
    _pad1: [u8; 0x10],
    _deprecated: [u8; 0x30],
    pub olpc_ofw_header: [u8; 0x10],
    pub ext_ramdisk_image: u32,
    pub ext_ramdisk_size: u32,
    pub ext_cmd_line_ptr: u32,
    _pad2: [u8; 0x74],
    pub edid_info: [u8; 0x80],
    pub efi_info: [u8; 0x20],
    pub alt_mem_k: u32,
    pub scratch: u32,
    pub e820_entries: u8,
    pub eddbuf_entries: u8,
    pub edd_mbr_sig_buf_entries: u8,
    pub kbd_status: u8,
    pub secure_boot: u8,
    _pad3: u16,
    sentinel: u8,
    _pad4: u8,
    pub kernel_header: KernelHeader,    // not documented in zero_page.txt but this goes here
    _pad5: [u8; 0x290-0x1f1-core::mem::size_of::<KernelHeader>()],
    pub edd_mbr_sig_buffer: [u8; 0x40],
    pub e820_table: [E820Entry; 128],
    _pad6: [u8; 0x30],
    pub eddbuf: [u8; 0x1ec],
    _pad7: [u8; 0x114],
}


// embedded in the zero page: https://github.com/torvalds/linux/blob/v4.16/Documentation/x86/zero-page.txt
// struct definition here: https://github.com/torvalds/linux/blob/81e4f8d68c66da301bb881862735bd74c6241a19/include/uapi/linux/screen_info.h#L11
#[repr(C, packed)]
pub struct ScreenInfo {
    pub orig_x: u8,             /* 0x00 */
    pub orig_y: u8,             /* 0x01 */
    pub ext_mem_k: u16,         /* 0x02 */
    pub orig_video_page: u16,   /* 0x04 */
    pub orig_video_mode: u8,    /* 0x06 */
    pub orig_video_cols: u8,    /* 0x07 */
    pub flags: u8,              /* 0x08 */
    pub unused2: u8,            /* 0x09 */
    pub orig_video_ega_bx: u16, /* 0x0a */
    pub unused3: u16,           /* 0x0c */
    pub orig_video_lines: u8,   /* 0x0e */
    pub orig_video_is_vga: u8,  /* 0x0f */
    pub orig_video_points: u16, /* 0x10 */

    /* VESA graphic mode -- linear frame buffer */
    pub lfb_width: u16,         /* 0x12 */
    pub lfb_height: u16,        /* 0x14 */
    pub lfb_depth: u16,         /* 0x16 */
    pub lfb_base: u32,          /* 0x18 */
    pub lfb_size: u32,          /* 0x1c */
    pub cl_magic: u16,          /* 0x20 */
    pub cl_offset: u16,         /* 0x22 */
    pub lfb_linelength: u16,    /* 0x24 */
    pub red_size: u8,           /* 0x26 */
    pub red_pos: u8,            /* 0x27 */
    pub green_size: u8,         /* 0x28 */
    pub green_pos: u8,          /* 0x29 */
    pub blue_size: u8,          /* 0x2a */
    pub blue_pos: u8,           /* 0x2b */
    pub rsvd_size: u8,          /* 0x2c */
    pub rsvd_pos: u8,           /* 0x2d */
    pub vesapm_seg: u16,        /* 0x2e */
    pub vesapm_off: u16,        /* 0x30 */
    pub pages: u16,             /* 0x32 */
    pub vesa_attributes: u16,   /* 0x34 */
    pub capabilities: u32,      /* 0x36 */
    // probably not aligned correctly :(
    pub ext_lfb_base: u32,      /* 0x3a */
    _reserved: [u8; 2],     /* 0x3e */
}

// These are the entries of the memory map passed to the kernel (https://github.com/torvalds/linux/blob/v4.16/Documentation/x86/zero-page.txt)
// struct definition here: https://github.com/torvalds/linux/blob/81e4f8d68c66da301bb881862735bd74c6241a19/arch/x86/include/asm/e820/types.h#L55
#[derive(Default, Copy, Clone)]
#[repr(C, packed)]
pub struct E820Entry {
    pub addr: u64,
    pub size: u64,
    pub typ: u32,
}
pub const E820_TYPE_RAM: u32 = 1;
pub const E820_TYPE_RESERVED: u32 = 2;
pub const _E820_TYPE_ACPI: u32 = 3;
pub const _E820_TYPE_NVS: u32 = 4;
pub const _E820_TYPE_UNUSABLE: u32 = 5;
pub const _E820_TYPE_PMEM: u32 = 7;


impl BootParams {
    pub fn new() -> SimpleResult<Self> {
        let page = allocate_low_pages(1)?;

        unsafe {
            Ok(core::ptr::read(page as *mut BootParams))
        }
    }
}


impl KernelHeader {
    pub fn new(kernel_image: &Vec<u8>) -> SimpleResult<&KernelHeader> {
        let kernel_header_offset = 0x1f1;
        let kernel_header_size = core::mem::size_of::<KernelHeader>();

        if kernel_image.len() < kernel_header_offset + kernel_header_size {
            return simple_error!("Kernel image is too small to contain a header.");
        }

        let kernel_header = &kernel_image[kernel_header_offset..kernel_header_offset + kernel_header_size];
        let kernel_header_ptr = kernel_header.as_ptr() as *const KernelHeader;
        unsafe {
            Ok(&*kernel_header_ptr)
        }
    }
}
