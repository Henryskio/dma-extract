//! Print the decoded playback format and output byte order of every sound.
//!
//! Scratch bin: the byte-order question is a *data* question, and the fastest
//! way to answer it for every resource is to decode each one and measure the
//! high-byte statistic of the resulting 16-bit stream. A quiet 16-bit signal
//! puts nearly all of its energy in the low byte, so the byte that is almost
//! always `00`/`ff` is the high one -- which names the order.

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
        println!(
            "=== {path}: version {}, {} resources",
            c.version, c.resource_count
        );
        for i in 0..c.resource_count as usize {
            let attr = c.attr_of(i);
            let payload = c.payload(i);
            let head: String = payload[..payload.len().min(6)]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            match sound::decode(&c, i) {
                Ok(d) => {
                    println!(
                        "{i:4} {:16} attr={attr:#04x} enc={:?} rate={} ch={} bits={} len={} head={} out={}",
                        c.name(i),
                        d.encoding,
                        d.format.sample_rate,
                        d.format.channels,
                        d.format.bits_per_sample,
                        payload.len(),
                        head,
                        d.pcm.len()
                    );
                    if d.format.bits_per_sample == 16 && d.pcm.len() >= 4096 {
                        let n = d.pcm.len().min(200_000) / 2 * 2;
                        let b = &d.pcm[..n];
                        let hi = b
                            .iter()
                            .step_by(2)
                            .filter(|x| **x == 0x00 || **x == 0xff)
                            .count() as f64;
                        let lo = b[1..]
                            .iter()
                            .step_by(2)
                            .filter(|x| **x == 0x00 || **x == 0xff)
                            .count() as f64;
                        let (ev, od) = (hi / (n / 2) as f64, lo / (n / 2) as f64);
                        let order = if (ev - od).abs() < 0.10 {
                            "?"
                        } else if ev > od {
                            "BIG-ENDIAN"
                        } else {
                            "little-endian"
                        };
                        println!("        -> output order {order} even={ev:.0} odd={od:.0}");
                    }
                }
                Err(e) => println!("{i:4} {:16} attr={attr:#04x} FAIL {e}", c.name(i)),
            }
        }
    }
}
