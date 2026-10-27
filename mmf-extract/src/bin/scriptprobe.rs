//! Probe a `MMFW Scripts` container: metadata, type table, per-object sizes.
//!
//! This is a development aid; the real extraction path lives in
//! `mmf_extract::script`.

use mmf_extract::container::Container;
use std::collections::BTreeMap;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: scriptprobe <HD.MMS>");
        std::process::exit(2);
    };
    let c = Container::open(&path).expect("open container");
    let Some(meta) = c.script.as_ref() else {
        eprintln!("{path}: not a Scripts container");
        std::process::exit(1);
    };
    println!(
        "kind={:?} big_endian={} version={} count={} types={} metadata=0x{:x}..0x{:x}",
        c.kind,
        c.big_endian,
        c.version,
        c.resource_count,
        meta.types.len(),
        meta.metadata_start,
        meta.metadata_end
    );

    let mut histogram: BTreeMap<&str, usize> = BTreeMap::new();
    for &t in &meta.object_types {
        *histogram.entry(&meta.types[t as usize]).or_default() += 1;
    }
    for (name, count) in &histogram {
        println!("  {count:>5}  {name}");
    }

    let mut payload_total = 0usize;
    let mut record_total = 0usize;
    let mut tail_total = 0usize;
    for i in 0..c.resource_count as usize {
        let type_index = meta.object_types[i] as usize;
        let type_name = &meta.types[type_index];
        let record_size = meta.type_aux[type_index] as usize;
        let count = meta.element_counts[i] as usize;
        let stored = c.payload_len(i);
        let fixed = count * record_size;
        payload_total += stored;
        record_total += fixed;
        tail_total += stored - fixed;
        println!(
            "#{i:04} {:<32} {:<24} count={:>4} record={:>3} payload={:>6} tail={}",
            c.name(i),
            type_name,
            count,
            record_size,
            stored,
            stored - fixed
        );
    }
    println!(
        "TOTAL payload={payload_total} records={record_total} tails={tail_total}"
    );
}
