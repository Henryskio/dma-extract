//! Print the sha256 of chosen frames of the 3D game's films.
//!
//! Scratch bin, used once to fill in the golden hashes in `tests/film_3d.rs`.

use clap::Parser;
use mmf_extract::container::Container;
use mmf_extract::film;
use sha2::{Digest, Sha256};

#[derive(Parser, Debug)]
#[command(name = "filmhash")]
struct Args {
    #[arg(value_name = "CONTAINER")]
    path: std::path::PathBuf,
    /// `film:frame` pairs, comma separated.
    #[arg(long, default_value = "0:0,0:60,1:0,1:61,2:0,2:900")]
    picks: String,
}

fn main() {
    let args = Args::parse();
    let data = std::fs::read(&args.path).expect("read container");
    let container = Container::parse(&args.path, data).expect("open Films");
    for pick in args.picks.split(',') {
        let (f, k) = pick.split_once(':').expect("film:frame");
        let (f, k): (usize, usize) = (f.parse().unwrap(), k.parse().unwrap());
        let decoded = film::decode_film(&container, f).expect("decode");
        let digest = Sha256::digest(&decoded.frames[k].indices);
        println!("({f}, {k}, \"{}\"),", digest.iter().map(|b| format!("{b:02x}")).collect::<String>());
    }
}
