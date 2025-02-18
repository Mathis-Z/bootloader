// https://github.com/torvalds/linux/blob/v4.16/Documentation/x86/boot.txt

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use uefi::{
    boot::{self, MemoryType}, mem::memory_map::{MemoryMap, MemoryMapOwned}, proto::console::gop::GraphicsOutput
};

use crate::{disk::open_protocol_unsafe, mem::allocate_low_pages, simple_error::SimpleResult};

use super::simple_error;

#[repr(C, packed)]
struct ScreenInfo {
    orig_x: u8,             /* 0x00 */
    orig_y: u8,             /* 0x01 */
    ext_mem_k: u16,         /* 0x02 */
    orig_video_page: u16,   /* 0x04 */
    orig_video_mode: u8,    /* 0x06 */
    orig_video_cols: u8,    /* 0x07 */
    flags: u8,              /* 0x08 */
    unused2: u8,            /* 0x09 */
    orig_video_ega_bx: u16, /* 0x0a */
    unused3: u16,           /* 0x0c */
    orig_video_lines: u8,   /* 0x0e */
    orig_video_is_vga: u8,  /* 0x0f */
    orig_video_points: u16, /* 0x10 */

    /* VESA graphic mode -- linear frame buffer */
    lfb_width: u16,  /* 0x12 */
    lfb_height: u16, /* 0x14 */
    lfb_depth: u16,  /* 0x16 */
    lfb_base: u32,   /* 0x18 */
    lfb_size: u32,   /* 0x1c */
    cl_magic: u16,   /* 0x20 */
    cl_offset: u16,
    lfb_linelength: u16,  /* 0x24 */
    red_size: u8,         /* 0x26 */
    red_pos: u8,          /* 0x27 */
    green_size: u8,       /* 0x28 */
    green_pos: u8,        /* 0x29 */
    blue_size: u8,        /* 0x2a */
    blue_pos: u8,         /* 0x2b */
    rsvd_size: u8,        /* 0x2c */
    rsvd_pos: u8,         /* 0x2d */
    vesapm_seg: u16,      /* 0x2e */
    vesapm_off: u16,      /* 0x30 */
    pages: u16,           /* 0x32 */
    vesa_attributes: u16, /* 0x34 */
    capabilities: u32,    /* 0x36 */
    // probably not aligned correctly :(
    ext_lfb_base: u32,  /* 0x3a */
    _reserved: [u8; 2], /* 0x3e */
}

const E820_TYPE_RAM: u32 = 1;
const E820_TYPE_RESERVED: u32 = 2;
const _E820_TYPE_ACPI: u32 = 3;
const _E820_TYPE_NVS: u32 = 4;
const _E820_TYPE_UNUSABLE: u32 = 5;
const _E820_TYPE_PMEM: u32 = 7;
#[derive(Default, Copy, Clone)]
#[repr(C, packed)]
struct E820Entry {
    addr: u64,
    size: u64,
    typ: u32,
}

#[derive(Debug)]
pub enum KernelParam {
    SetupSects,
    _RootFlags,
    _SysSize,
    _RamSize,
    VidMode,
    _RootDev,
    _BootFlag,
    _Jump,
    Header,
    Version,
    _RealmodeSwitch,
    _StartSysSeg,
    _KernelVersion,
    TypeOfLoader,
    LoadFlags,
    _SetupMoveSize,
    Code32Start,
    RamdiskImage,
    RamdiskSize,
    _BootsectKludge,
    HeapEndPtr,
    _ExtLoaderVer,
    _ExtLoaderType,
    CmdLinePtr,
    _InitrdAddressMax,
    KernelAlignment,
    RelocatableKernel,
    MinAlignment,
    XLoadflags,
    _CmdlineSize,
    _HardwareSubarch,
    _HardwareSubarchData,
    _PayloadOffset,
    _PayloadLength,
    _SetupData,
    PrefAddress,
    _InitSize,
    HandoverOffset,
}

impl KernelParam {
    pub fn offset_and_size(&self) -> (usize, usize) {
        let (offset, size) = match self {
            KernelParam::SetupSects => (0x1f1, 1),
            KernelParam::_RootFlags => (0x1f2, 2),
            KernelParam::VidMode => (0x1fa, 2),
            KernelParam::_BootFlag => (0x1fe, 2),
            KernelParam::Header => (0x202, 4),
            KernelParam::TypeOfLoader => (0x210, 1),
            KernelParam::LoadFlags => (0x211, 1),
            KernelParam::Code32Start => (0x214, 4),
            KernelParam::RamdiskImage => (0x218, 4),
            KernelParam::RamdiskSize => (0x21c, 4),
            KernelParam::HeapEndPtr => (0x224, 2),
            KernelParam::CmdLinePtr => (0x228, 4),
            KernelParam::KernelAlignment => (0x230, 4),
            KernelParam::RelocatableKernel => (0x234, 1),
            KernelParam::MinAlignment => (0x235, 1),
            KernelParam::XLoadflags => (0x236, 2),
            KernelParam::_CmdlineSize => (0x238, 4),
            KernelParam::PrefAddress => (0x258, 8),
            KernelParam::_InitSize => (0x260, 4),
            KernelParam::HandoverOffset => (0x264, 4),
            KernelParam::Version => (0x0206, 2),
            _ => todo!(),
        };
        (offset - 0x1f1, size)
    }
}

pub struct KernelParams {
    buf: Vec<u8>,
}

impl KernelParams {
    pub fn new(kernel_image: &Vec<u8>) -> SimpleResult<KernelParams> {
        if kernel_image[0x01FE] != 0x55 || kernel_image[0x01FF] != 0xAA {
            return simple_error!("Kernel image does not have the correct magic number");
        }

        let hex_string: String = kernel_image[0x1f1..0x268]
            .iter()
            .map(|byte| alloc::format!("{:02x}", byte))
            .collect();
        uefi::println!("{}", hex_string);

        Ok(KernelParams {
            buf: (kernel_image[0x1f1..0x268].to_vec()),
        })
    }

    pub fn get_param(&self, param: KernelParam) -> usize {
        let (offset, size) = param.offset_and_size();

        // pad with zeros
        let mut bytes = [0u8; core::mem::size_of::<usize>()];
        bytes[..size].copy_from_slice(&self.buf[offset..offset + size]);

        usize::from_le_bytes(bytes)
    }

    pub fn set_param(&mut self, param: KernelParam, value: usize) {
        let (offset, size) = param.offset_and_size();
        let old_slice = &mut self.buf[offset..offset + size];

        let value_bytes = &value.to_le_bytes()[0..size];

        old_slice.copy_from_slice(value_bytes);
    }

    pub fn copy_into_zero_page(&self) -> SimpleResult<usize> {
        let zero_page = allocate_low_pages(1)?;
        unsafe {
            core::ptr::copy(
                self.buf.as_ptr(),
                (zero_page + 0x1f1) as *mut u8,
                self.buf.len(),
            );
        }
        KernelParams::set_video_params(zero_page);

        Ok(zero_page)
    }

    // this is partially guessed and partially taken from grub2 source code because there was not much documentation available
    pub fn set_video_params(zero_page: usize) {
        const VIDEO_TYPE_EFI: u8 = 0x70;    // linux kernel screen_info.h

        let screen_info = unsafe { &mut *(zero_page as *mut ScreenInfo) };

        let gop_handle = boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
        let mut gop = open_protocol_unsafe::<GraphicsOutput>(gop_handle).unwrap();

        uefi::println!("GOP pixel format: {:?}", gop.current_mode_info().pixel_format());

        // pretty sure about these
        screen_info.orig_video_page = 0;
        screen_info.orig_video_points = 16;
        screen_info.lfb_width = gop.current_mode_info().resolution().0 as u16;
        screen_info.lfb_height = gop.current_mode_info().resolution().1 as u16;
        screen_info.lfb_depth = 32;
        screen_info.lfb_linelength = gop.current_mode_info().stride() as u16 * (screen_info.lfb_depth >> 3);
        screen_info.lfb_base = gop.frame_buffer().as_mut_ptr() as u32;
        screen_info.ext_lfb_base = (gop.frame_buffer().as_mut_ptr() as u64 >> 32) as u32;
        screen_info.capabilities |= 0b10;     // mark framebuffer address as 64 bit
        screen_info.lfb_size = gop.frame_buffer().size() as u32;
        
        screen_info.orig_video_ega_bx = 0;

        // for BGR8 (from grub2 source code)
        // TODO: support pixel formats other than BGR8
        screen_info.red_size = 8;
        screen_info.red_pos = 16;
        screen_info.green_size = 8;
        screen_info.green_pos = 8;
        screen_info.blue_size = 8;
        screen_info.blue_pos = 0;
        screen_info.rsvd_size = 8;
        screen_info.rsvd_pos = 24;

        // not so sure about these
        screen_info.orig_x = 0;
        screen_info.orig_y = 0;
        screen_info.orig_video_cols = 80;
        screen_info.orig_video_lines = 25;
        screen_info.orig_video_is_vga = VIDEO_TYPE_EFI;
        screen_info.orig_video_mode = 0;      // could also be 0x03

        // sketchy
        screen_info.ext_mem_k = ((32 * 0x100000) >> 10) as u16;

    }

    pub fn set_memory_map(zero_page: usize, mmap: &MemoryMapOwned) {
        const MAX_E820_ENTRIES: usize = 128;

        let e820_table = unsafe {
            core::slice::from_raw_parts_mut((zero_page + 0x2d0) as *mut E820Entry, MAX_E820_ENTRIES)
        };

        let mut i = 0;
        for entry in mmap.entries() {
            let typ = match entry.ty {
                MemoryType::CONVENTIONAL => E820_TYPE_RAM,
                MemoryType::BOOT_SERVICES_CODE => E820_TYPE_RAM,
                MemoryType::BOOT_SERVICES_DATA => E820_TYPE_RAM,
                MemoryType::LOADER_CODE => E820_TYPE_RAM,
                MemoryType::LOADER_DATA => E820_TYPE_RAM,
                _ => E820_TYPE_RESERVED,
            };

            e820_table[i] = E820Entry {
                addr: entry.phys_start,
                size: entry.page_count * 4096,
                typ,
            };

            i += 1;
            if i >= MAX_E820_ENTRIES {
                break;
            }
        }

        let e820_entries_ptr = (zero_page + 0x1e8) as *mut u8;

        unsafe {
            *e820_entries_ptr = i as u8;
        };
    }
}

pub fn allocate_cmdline(cmdline: &str) -> SimpleResult<usize> {
    let addr = allocate_low_pages(1)?;

    unsafe {
        let ptr = addr as *mut u8;
        for (i, byte) in cmdline.as_bytes().iter().enumerate() {
            *ptr.offset(i as isize) = *byte;
        }
    }
    Ok(addr)
}
