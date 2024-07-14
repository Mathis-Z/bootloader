// https://github.com/torvalds/linux/blob/v4.16/Documentation/x86/boot.txt

extern crate alloc;
use core::{mem, slice};

use alloc::vec::Vec;
use uefi::{prelude::BootServices, println, table::Boot};
use uefi_raw::PhysicalAddress;

use crate::memory::{allocate_low_pages, copy_buf_to_low_aligned_address};

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
    orig_video_isVGA: u8,   /* 0x0f */
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

#[derive(Debug)]
pub enum KernelParam {
    SetupSects,
    RootFlags,
    SysSize,
    RamSize,
    VidMode,
    RootDev,
    BootFlag,
    Jump,
    Header,
    Version,
    RealmodeSwitch,
    StartSysSeg,
    KernelVersion,
    TypeOfLoader,
    LoadFlags,
    SetupMoveSize,
    Code32Start,
    RamdiskImage,
    RamdiskSize,
    BootsectKludge,
    HeapEndPtr,
    ExtLoaderVer,
    ExtLoaderType,
    CmdLinePtr,
    InitrdAddressMax,
    KernelAlignment,
    RelocatableKernel,
    MinAlignment,
    XLoadflags,
    CmdlineSize,
    HardwareSubarch,
    HardwareSubarchData,
    PayloadOffset,
    PayloadLength,
    SetupData,
    PrefAddress,
    InitSize,
    HandoverOffset,
}

impl KernelParam {
    pub fn offset_and_size(&self) -> (usize, usize) {
        let (offset, size) = match self {
            KernelParam::SetupSects => (0x1f1, 1),
            KernelParam::RootFlags => (0x1f2, 2),
            KernelParam::SysSize => todo!(),
            KernelParam::RamSize => todo!(),
            KernelParam::VidMode => (0x1fa, 2),
            KernelParam::RootDev => todo!(),
            KernelParam::BootFlag => (0x1fe, 2),
            KernelParam::Jump => todo!(),
            KernelParam::Header => (0x202, 4),
            KernelParam::Version => todo!(),
            KernelParam::RealmodeSwitch => todo!(),
            KernelParam::StartSysSeg => todo!(),
            KernelParam::KernelVersion => todo!(),
            KernelParam::TypeOfLoader => (0x210, 1),
            KernelParam::LoadFlags => todo!(),
            KernelParam::SetupMoveSize => todo!(),
            KernelParam::Code32Start => todo!(),
            KernelParam::RamdiskImage => todo!(),
            KernelParam::RamdiskSize => todo!(),
            KernelParam::BootsectKludge => todo!(),
            KernelParam::HeapEndPtr => (0x224, 2),
            KernelParam::ExtLoaderVer => todo!(),
            KernelParam::ExtLoaderType => todo!(),
            KernelParam::CmdLinePtr => (0x228, 4),
            KernelParam::InitrdAddressMax => todo!(),
            KernelParam::KernelAlignment => todo!(),
            KernelParam::RelocatableKernel => (0x234, 1),
            KernelParam::MinAlignment => todo!(),
            KernelParam::XLoadflags => (0x236, 2),
            KernelParam::CmdlineSize => (0x238, 4),
            KernelParam::HardwareSubarch => todo!(),
            KernelParam::HardwareSubarchData => todo!(),
            KernelParam::PayloadOffset => todo!(),
            KernelParam::PayloadLength => todo!(),
            KernelParam::SetupData => todo!(),
            KernelParam::PrefAddress => todo!(),
            KernelParam::InitSize => (0x260, 4),
            KernelParam::HandoverOffset => todo!(),
        };
        (offset - 0x1f1, size)
    }
}

pub struct KernelParams {
    buf: Vec<u8>,
}

impl KernelParams {
    pub fn new(kernel_image: &Vec<u8>) -> KernelParams {
        KernelParams {
            buf: (kernel_image[0x1f1..0x268].to_vec()),
        }
    }

    pub fn get_param(&self, param: KernelParam) -> u64 {
        let (offset, size) = param.offset_and_size();
        let bytes = &self.buf[offset..offset + size];

        from_bytes(bytes)
    }

    pub fn set_param(&mut self, param: KernelParam, value: u64) {
        let (offset, size) = param.offset_and_size();

        let old_slice = &mut self.buf[offset..offset + size];
        let value_bytes = &to_bytes(value)[0..size];

        old_slice.copy_from_slice(value_bytes);
    }

    pub fn check(&self) {
        let boot_flag = self.get_param(KernelParam::BootFlag);
        let header = self.get_param(KernelParam::Header);

        if boot_flag != 0xaa55 {
            println!(
                "Error checking kernel params: boot_flag is {:x} and not 0xaa55!",
                boot_flag
            );
        }
        if header != 0x53726448 {
            println!(
                "Error checking kernel params: header is {:x} and not 0x53726448!",
                header
            );
        }
    }

    pub fn copy_to_aligned_address(&self, bs: &BootServices) -> u64 {
        unsafe { copy_buf_to_low_aligned_address(bs, self.buf.as_slice()) }
    }

    pub fn copy_into_zero_page(&self, bs: &BootServices) -> u64 {
        let zero_page = allocate_low_pages(bs, 1);
        unsafe {
            core::ptr::copy(
                self.buf.as_ptr(),
                (zero_page + 0x1f1) as *mut u8,
                self.buf.len(),
            );
        }
        KernelParams::set_video_params(zero_page);

        zero_page
    }

    fn set_video_params(zero_page: u64) {
        let screen_info = unsafe { &mut *(zero_page as *mut ScreenInfo) };

        screen_info.orig_x = 0;
        screen_info.orig_y = 25;
        screen_info.ext_mem_k = 0; // what is this?
        screen_info.orig_video_page = 0;
        screen_info.orig_video_mode = 0x07;
        screen_info.orig_video_cols = 80;
        screen_info.orig_video_lines = 25;
        screen_info.orig_video_isVGA = 1;
        screen_info.orig_video_points = 16;
    }
}

fn from_bytes<T: From<u64>>(bytes: &[u8]) -> T {
    // Pad the bytes slice with zeros to make it 8 bytes long
    let mut array = [0u8; 8];
    let len = bytes.len();
    array[..len].copy_from_slice(&bytes);

    let num = u64::from_le_bytes(array);

    T::from(num)
}

fn to_bytes<T: Into<u64>>(num: T) -> [u8; 8] {
    let num_u64: u64 = num.into();
    num_u64.to_le_bytes()
}

pub fn allocate_cmdline(bs: &BootServices) -> PhysicalAddress {
    let addr = allocate_low_pages(bs, 1);

    let auto_buf: [u8; 4] = [0x61, 0x75, 0x74, 0x6f]; // 'auto' as kernel cmdline

    unsafe {
        let ptr = addr as *mut u8;
        for (i, byte) in auto_buf.iter().enumerate() {
            *ptr.offset(i.try_into().unwrap()) = *byte;
        }
    }
    addr
}
