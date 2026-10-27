//! Scripts-container tests.
//!
//! The important one is the lossless guarantee: extracting a script object in
//! `--original` mode must write exactly the bytes stored in the container,
//! with nothing added, dropped, reordered or transformed. The other tests pin
//! down the metadata that the modern output relies on.

use mmf_extract::cli::sanitize;
use mmf_extract::container::{Container, ContainerKind};
use mmf_extract::script;
use std::path::{Path, PathBuf};

/// Where the game's assets live.
fn raw_assets_dir() -> Option<PathBuf> {
    // The asset folder is normally mounted at /RawAssets; point DMA_ASSETS at
    // another copy if needed.
    let extra = std::env::var("DMA_ASSETS").unwrap_or_default();
    for candidate in ["/RawAssets", extra.as_str()] {
        let p = PathBuf::from(candidate);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

fn script_container() -> Option<(PathBuf, Container)> {
    let dir = raw_assets_dir()?;
    let path = dir.join("HD.MMS");
    if !path.exists() {
        return None;
    }
    let container = Container::open(&path).expect("open HD.MMS");
    assert_eq!(container.kind, ContainerKind::Scripts);
    Some((path, container))
}

/// Every `--original` file must be a byte-exact slice of the container.
#[test]
fn original_script_payloads_are_byte_identical() {
    let Some((path, container)) = script_container() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let n = container.resource_count as usize;
    let meta = container.script.as_ref().expect("script metadata");
    assert_eq!(n, 4813);
    assert_eq!(meta.types.len(), 89);

    let tmp = tempdir("scripts-original");
    let written = script::extract_container(&container, &tmp, true).expect("extract");
    assert_eq!(written, n, "every script object must be written");

    let on_disk = std::fs::read(&path).expect("read container");

    for i in 0..n {
        let safe = sanitize(container.name(i), i);
        let file = tmp.join(format!("{i:05}_{safe}.bin"));
        let extracted = std::fs::read(&file)
            .unwrap_or_else(|e| panic!("reading {}: {e}", file.display()));

        let start = container.offsets[i] as usize;
        let end = container.offsets[i + 1] as usize;

        // Scripts payloads are stored uncompressed: the file is the stored
        // payload, and the stored payload is the container's own bytes.
        assert_eq!(
            extracted,
            container.payload(i),
            "resource {i} ('{}'): extracted bytes differ from the payload",
            container.name(i)
        );
        assert_eq!(
            extracted,
            &on_disk[start..end],
            "resource {i} ('{}'): extracted bytes differ from [{start},{end}) on disk",
            container.name(i)
        );
        assert_eq!(
            container.plain_len(i),
            container.payload_len(i),
            "resource {i}: payload is not stored plainly"
        );
    }
}

/// Metadata must account for every payload: either exactly `count * record`
/// bytes, or that plus a class-specific string tail.
#[test]
fn script_metadata_accounts_for_every_payload_byte() {
    let Some((_path, container)) = script_container() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let meta = container.script.as_ref().expect("script metadata");
    let n = container.resource_count as usize;

    assert_eq!(meta.version, 11);
    assert_eq!(meta.type_count as usize, meta.types.len());
    assert_eq!(meta.element_counts.len(), n);
    assert_eq!(meta.object_types.len(), n);
    assert_eq!(meta.object_names.len(), n);
    assert_eq!(meta.metadata_end, container.offsets[0] as usize);

    let mut payload_bytes = 0usize;
    let mut record_bytes = 0usize;
    let mut tail_bytes = 0usize;

    for i in 0..n {
        let type_index = meta.object_types[i] as usize;
        let type_name = &meta.types[type_index];
        let record_size = meta.type_aux[type_index] as usize;
        let count = meta.element_counts[i] as usize;
        let len = container.payload_len(i);
        let fixed = count * record_size;

        assert!(record_size > 0, "resource {i}: zero record size");
        assert!(
            fixed <= len,
            "resource {i} ('{}', {type_name}): {count} x {record_size} = {fixed} exceeds {len}",
            container.name(i)
        );
        payload_bytes += len;
        record_bytes += fixed;

        match type_name.as_str() {
            // These append a string tail; the tail size is checked by the
            // modern extraction test.
            "Animation" | "creditsTextEntry" | "StdBrush" | "AnimBrush" | "TextList"
            | "StringTable" => {
                tail_bytes += len - fixed;
            }
            // Everything else must be an exact array of fixed records.
            _ => {
                assert_eq!(
                    fixed,
                    len,
                    "resource {i} ('{}', {type_name}) has {} trailing bytes",
                    container.name(i),
                    len - fixed
                );
            }
        }
    }

    assert_eq!(payload_bytes, 3_329_791);
    assert_eq!(record_bytes, 3_282_998);
    assert_eq!(tail_bytes, 46_793);
    assert_eq!(record_bytes + tail_bytes, payload_bytes);
}

/// The modern output must describe the same objects and decode the string
/// tables, not just emit opaque blobs.
#[test]
fn modern_script_output_is_structured_and_complete() {
    let Some((_path, container)) = script_container() else {
        eprintln!("skipping: game assets not present");
        return;
    };
    let n = container.resource_count as usize;
    let tmp = tempdir("scripts-modern");

    let written = script::extract_container(&container, &tmp, false).expect("extract");
    assert_eq!(written, n);

    let json = std::fs::read_to_string(tmp.join("container.json")).expect("container.json");
    assert!(json.contains("\"resource_count\": 4813"));
    assert!(json.contains("\"opaque_bytes\": 0"));
    assert!(json.contains("\"type\": \"StringTable\""));
    assert!(json.contains("\"record_bytes\": 3282998"));

    let types = std::fs::read_to_string(tmp.join("types.txt")).expect("types.txt");
    let type_lines = types
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .count();
    assert_eq!(type_lines, 89);

    let strings = std::fs::read_to_string(tmp.join("strings.txt")).expect("strings.txt");
    assert!(strings.contains("Too many blueprints"));
    assert!(strings.contains("Disney Interactive"));
    assert!(strings.contains("FineSpray 1"));
    assert!(strings.contains(
        "TextList[4810] \"creationsPath\" [2] offset=0x000a \"Creations\""
    ));
    assert!(strings.contains(
        "AnimBrush[1393] \"WhippedCream\" [4] offset=0x003c \"Whipped Cream5\""
    ));
    assert!(strings.contains("# 632 payload strings decoded"));

    let object = std::fs::read_to_string(
        tmp.join("objects").join("00000_localizationStrs.txt"),
    )
    .expect("object dump");
    assert!(object.contains("StringTable entries (130 x 10 bytes"));
    assert!(object.contains("Too many blueprints"));
    assert!(object.contains("All 3717 payload bytes are represented above."));

    let text_list = std::fs::read_to_string(
        tmp.join("objects").join("04810_creationsPath.txt"),
    )
    .expect("TextList dump");
    assert!(text_list.contains("## String offsets (3 x 4 bytes"));
    assert!(text_list.contains("offset=0x000a -> \"Creations\""));
    assert!(text_list.contains("All 32 payload bytes are represented above."));

    let anim_brush = std::fs::read_to_string(
        tmp.join("objects").join("01393_WhippedCream.txt"),
    )
    .expect("AnimBrush dump");
    assert!(anim_brush.contains("ref=1393 name_offset=0x003c flags=0x0 -> \"Whipped Cream5\""));
    assert!(anim_brush.contains("All 115 payload bytes are represented above."));

    let objects = std::fs::read_dir(tmp.join("objects")).expect("objects dir");
    assert_eq!(objects.count(), n);
}

/// Make a temporary directory under the job's temp area.
fn tempdir(tag: &str) -> PathBuf {
    let base = std::env::var("MMF_TEST_TMP")
        .unwrap_or_else(|_| std::env::temp_dir().join("mmf-extract-tests").display().to_string());
    let dir = Path::new(&base).join(format!("{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}
