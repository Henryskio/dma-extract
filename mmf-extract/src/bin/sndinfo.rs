//! Dump every per-resource field of a sound container.
//!
//! Scratch bin: when two resources are declared 8-bit but sound like noise, the
//! question is which field is wrong, and answering it needs all of the fields
//! side by side rather than the one the extractor happens to read.

use mmf_extract::container::{Container, ContainerKind};
use mmf_extract::sound;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    for path in &args {
        let c = match Container::open(std::path::Path::new(path)) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{path}: {e}");
                continue;
            }
        };
        if c.kind != ContainerKind::Sounds {
            eprintln!("{path}: not a Sounds container ({:?})", c.kind);
            continue;
        }
        println!("=== {path}: version {}, {} resources", c.version, c.resource_count);
        for i in 0..c.resource_count as usize {
            let d = sound::decode(&c, i).ok();
            let enc = d.as_ref().map(|x| format!("{:?}", x.encoding)).unwrap_or_else(|| "FAIL".into());
            let dim = c.dimension(i);
            println!(
                "{i:4} {:16} attr={:#04x} flags={:#04x} kind={:#04x} \
                 declared={} len={} dim={}x{} enc={}",
                c.name(i),
                c.attr_of(i),
                c.flags_of(i),
                c.kind_of(i),
                c.plain_len(i),
                c.payload_len(i),
                dim.width,
                dim.height,
                enc
            );
        }
    }
}
