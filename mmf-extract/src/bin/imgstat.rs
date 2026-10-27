//! Report basic statistics about a PNG so renders can be sanity-checked
//! without loading the image into the conversation.
//!
//! `imgstat <png>...` prints, per file, the dimensions, pixel count, the
//! number of fully transparent and fully opaque pixels, the count of distinct
//! colours, and the top few colours by frequency.

use png::Decoder;
use std::collections::HashMap;
use std::fs::File;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: imgstat <png>...");
        return;
    }
    for path in &args {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                println!("{path}: open error {e}");
                continue;
            }
        };
        let mut reader = match Decoder::new(std::io::BufReader::new(file)).read_info() {
            Ok(d) => d,
            Err(e) => {
                println!("{path}: decode header error {e}");
                continue;
            }
        };
        let (w, h) = {
            let i = reader.info();
            (i.width, i.height)
        };
        let mut buf = vec![0u8; (w * h * 4) as usize + 4096];
        let info = match reader.next_frame(&mut buf) {
            Ok(i) => i,
            Err(e) => {
                println!("{path}: read frame error {e}");
                continue;
            }
        };
        let (w, h) = (info.width, info.height);
        let rgba = &buf[..info.buffer_size()];
        let mut transparent = 0usize;
        let mut opaque = 0usize;
        let mut colors: HashMap<[u8; 4], usize> = HashMap::new();
        for px in rgba.chunks_exact(4) {
            let c = [px[0], px[1], px[2], px[3]];
            *colors.entry(c).or_default() += 1;
            if c[3] == 0 {
                transparent += 1;
            } else if c[3] == 255 {
                opaque += 1;
            }
        }
        let total = (w * h) as usize;
        let mut top: Vec<(&[u8; 4], &usize)> = colors.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        let top_str: Vec<String> = top
            .iter()
            .take(12)
            .map(|(c, n)| format!("{}x{}x{}x{}:{}", c[0], c[1], c[2], c[3], n))
            .collect();
        println!(
            "{}:\t{w}x{h}\ttotal={total}\ttransparent={transparent}\topaque={opaque}\tdistinct={}\ttop=[{}]",
            std::path::Path::new(path)
                .file_name()
                .unwrap()
                .to_string_lossy(),
            colors.len(),
            top_str.join(" ")
        );
    }
}
