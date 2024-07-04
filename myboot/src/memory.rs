use uefi::{prelude::*, println};

pub fn print_memory_map(bs: &BootServices) {
    let memory_map_size = bs.memory_map_size();

    let mut buf: [u8; 20000] = [0; 20000];

    match bs.memory_map(&mut buf) {
        Ok(mut memory_map) => {
            memory_map.sort();
            for md in memory_map.entries().take(40) {
                println!(
                    "phys {:#X} virt {:#X} size {} ty {:?}",
                    md.phys_start,
                    md.virt_start,
                    md.page_count * 4096,
                    md.ty
                );
            }
        }
        Err(err) => println!("Could not get memory map because of: {}", err),
    }
}
