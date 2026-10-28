//! MMFW resource container parsing (the game's in-house format, not Clickteam).
//!
//! Layout (all multi-byte integers big-endian on the wire unless the file
//! declares the "II" little-endian marker):
//!
//! ```text
//! 0x00  16 bytes  signature: "MMFW " + tag, NUL padded
//!                tag = "Pictures" | "Sounds" | "Films" | "Scripts" | "Blobs"
//! 0x10   2 bytes  endian marker: "MM" (big-endian) or "II" (little-endian)
//! 0x12   2 bytes  u16 version; 3 in the first game, 2 in the sequel
//!                (both parse identically)
//! 0x14   6 bytes  file id
//! 0x1A   8 bytes  timestamp (double)
//! 0x1E   2 bytes  u16 resource count N  -- **version 1 only**
//! 0x22   2 bytes  u16 resource count N  -- versions 2 and 3
//! 0x20  4*(N+1)  u32 offsets, version 1; resource i spans [off[i], off[i+1])
//! 0x24  4*(N+1)  u32 offsets, versions 2 and 3
//!         2 bytes  padding
//!        32*N      resource names, NUL padded ASCII
//!        4*N      u32 array (pictures: CRC/hash; sounds: Mac timestamp)
//!        [2]      2-byte word, sounds only
//!          N       attribute/format byte array
//!          N       flags byte array
//!        [2]      padding, pictures only
//!        4*N      dimension table: {u16 height, u16 width} (zero for sounds)
//!        4*N      decoded-size table
//!          N       "kind" byte array, pictures only
//!        [2]      padding, pictures only
//! ...               resource payloads, concatenated at offsets[0]
//! ```
//!
//! **Version 1** (the first game, 1997) differs in two measured ways:
//!
//! * the resource count is the u16 at `0x1E`, and it is `N` — the u16 at
//!   `0x22` holds an unrelated value (in `HD.PIC` it reads 20288 while the
//!   real count is 1682);
//! * the offsets table starts at **`0x20`**, four bytes earlier than the
//!   `0x24` the later versions use. The table still holds `N + 1` entries and
//!   its last entry is the file size, which is what pins the base: in every
//!   shipped container `u32@(0x20 + 4*N)` equals the file length.
//!
//! Names are 32-byte fields in every version, at `0x20 + 4*(N+1) + 2` under
//! version 1 and `0x24 + 4*(N+1) + 2` under versions 2 and 3.
//!
//! Version 1 *does* have a per-resource metadata region, but it is its own
//! layout, measured against every shipped container and confirmed by the
//! loader in `DMA1.exe`. All three kinds start with a `4*N` u32 array (sound
//! containers store Mac timestamps there); what follows differs by kind:
//!
//! * **Pictures** — a 2-byte marker of `0x0007`, then `N` flags bytes,
//!   `N` kind bytes, a `4*N` `{u16 height, u16 width}` dimension table, a
//!   `4*N` stored-size table (all zeros in the shipped files, so v1 payloads
//!   read as uncompressed), one more `N` byte array, a u16 palette count and
//!   then `count` 1024-byte palettes. Total `15*N + 4` plus the bank.
//! * **Sounds** — one `N` byte array (the format byte) and 2 bytes.
//! * **Blobs** — nothing beyond the array plus 2 bytes.
//!
//! The region must end exactly at `offsets[0]`, and the parser rejects a
//! container where it does not rather than guessing. See `docs/GAME1.md`.
//!
//! Picture metadata is exactly `15 * N + 4` bytes; sound metadata is
//! `14 * N + 2` bytes. Both were measured against every shipped container and
//! pin the field boundaries down. The sound layout matches the loader in
//! `DMA4.exe`, which reads the 2-byte word after the timestamp array.
//!
//! **Scripts** containers do not follow the picture layout. They have no
//! 32-byte names table and no picture metadata. Instead, immediately after the
//! offset table sits a serialized object database: a script version, a type
//! table, an array of element counts, an array of type indices, and a string
//! list of object names. See [`ScriptMeta`] and `docs/SCRIPTS.md`.

use crate::picture::PALETTE_BYTES;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// The kind of payload a container holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    Pictures,
    Sounds,
    Films,
    /// Undifferentiated opaque payloads. The first game uses one to carry
    /// its videos (`pack.BLB`, tag "Blobs").
    Blobs,
    Scripts,
    Unknown,
}

impl ContainerKind {
    fn from_tag(tag: &str) -> Self {
        match tag {
            "Pictures" => Self::Pictures,
            "Sounds" => Self::Sounds,
            "Films" => Self::Films,
            "Blobs" => Self::Blobs,
            // The first game writes "MMFW Script"; the third writes
            // "MMFW Scripts"; the second "MMFW 3 Script".
            "Scripts" | "3 Script" | "Script" => Self::Scripts,
            _ => Self::Unknown,
        }
    }
}

/// The pixel dimensions of one picture resource.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Dimension {
    pub width: u16,
    pub height: u16,
}

/// Metadata for a `MMFW Scripts` container.
///
/// Unlike picture containers, a Scripts container stores a serialized global
/// object database. Each resource payload is one object. The metadata says
/// what class each object is and how many fixed-size elements its payload
/// starts with.
#[derive(Debug, Clone, Default)]
pub struct ScriptMeta {
    /// Script metadata version. Every shipped container uses 11.
    pub version: u16,
    /// First unknown header field (observed `0xffff`).
    pub unknown_1: u16,
    /// Second unknown header field (observed `0xffff`).
    pub unknown_2: u16,
    /// Third unknown header field (observed 2490).
    pub unknown_3: u16,
    /// Number of entries in `type_aux` and `types`.
    pub type_count: u16,
    /// Per-class fixed record size in bytes. This is the size of one element
    /// at the start of an object payload; classes may append a string tail.
    pub type_aux: Vec<u16>,
    /// Class/type names in index order.
    pub types: Vec<String>,
    /// Per-object element count. The payload starts with this many records of
    /// `type_aux[object_type[i]]` bytes.
    pub element_counts: Vec<u32>,
    /// Per-object index into `types`.
    pub object_types: Vec<u16>,
    /// Per-object names, in resource order. Mirrors `Container::names`.
    pub object_names: Vec<String>,
    /// Offset of the script metadata (immediately after the offset table).
    pub metadata_start: usize,
    /// Offset just past the metadata, i.e. the first payload.
    pub metadata_end: usize,
}

/// A parsed MMFW resource container.
pub struct Container {
    pub path: PathBuf,
    pub data: Vec<u8>,
    pub kind: ContainerKind,
    /// True when the file declares big-endian ("MM") storage.
    pub big_endian: bool,
    pub version: u16,
    pub file_id: [u8; 6],
    pub timestamp: f64,
    pub resource_count: u16,
    /// `resource_count + 1` file offsets; the last is EOF.
    pub offsets: Vec<u32>,
    pub names: Vec<String>,
    /// Per-resource attribute byte (object offset 0x278). Purpose unknown;
    /// values correlate with ink/transparency groups but no accessor was
    /// found that reads it for pixel decoding.
    pub attr: Vec<u8>,
    /// Per-resource flags byte (object offset 0x284). Bit 0 selects 8 bits
    /// per pixel; bits 2-3 and 2-6 are ink/transparency selectors.
    pub flags: Vec<u8>,
    /// Per-resource dimensions.
    pub dimensions: Vec<Dimension>,
    /// Per-resource decompressed payload size. A value of zero means the
    /// payload is stored uncompressed.
    pub sizes: Vec<u32>,
    /// Per-resource u32 array of undetermined purpose.
    pub extra_u32: Vec<u32>,
    /// Per-resource "kind" byte (object offset 0x2a8): the palette source.
    /// 0 = no palette, 1 = a 1024-byte palette prefixed to the payload, and
    /// `k >= 2` means "use file palette-bank entry `k - 2`".
    pub kinds: Vec<u8>,
    /// Offset of the end of the names table (start of the metadata region).
    pub names_end: usize,
    /// Version-1 picture containers only: the palette bank.
    ///
    /// The first game stores its palettes at the *end* of the metadata region,
    /// immediately before the first payload, instead of prefixing them to each
    /// payload the way versions 2 and 3 do. Each is 1024 bytes (256 entries of
    /// transparency, red, green, blue), and a run-length sub-format resource
    /// whose colour records are single palette indices indexes into one of
    /// them. Measured on all three shipped v1 picture containers: the region
    /// between the metadata and `offsets[0]` is exactly the u16 count plus
    /// `count * PALETTE_BYTES` bytes. Empty for every other version and kind.
    pub palettes: Vec<Vec<u8>>,
    /// Scripts-only object-database metadata. `None` for every other kind.
    pub script: Option<ScriptMeta>,
}

impl Container {
    /// Parse a container from disk.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let data = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(path, data)
    }

    /// Parse a container from an in-memory image.
    pub fn parse(path: impl Into<PathBuf>, data: Vec<u8>) -> Result<Self> {
        let path = path.into();
        if data.len() < 0x24 {
            bail!("{}: file too short ({})", path.display(), data.len());
        }
        if &data[0..4] != b"MMFW" {
            bail!(
                "{}: not an MMFW container (magic {:?})",
                path.display(),
                &data[0..4]
            );
        }
        // The signature is "MMFW " followed by the tag, NUL padded.
        let tag = String::from_utf8_lossy(&data[5..16])
            .trim_end_matches('\0')
            .to_string();
        let kind = ContainerKind::from_tag(&tag);

        let big_endian = match &data[0x10..0x12] {
            b"MM" => true,
            b"II" => false,
            other => bail!("{}: unknown endian marker {:?}", path.display(), other),
        };

        let version = read_u16(&data, 0x12, big_endian);
        // Version 3 = the third game, 2 = the second game, 1 = the first
        // game. The metadata layout measured below is identical in each, but
        // an unknown version means an unmeasured writer, so it is rejected
        // rather than guessed at.
        if version != 3 && version != 2 && version != 1 {
            bail!("{}: unexpected version {}", path.display(), version);
        }

        let mut file_id = [0u8; 6];
        file_id.copy_from_slice(&data[0x14..0x1A]);

        let mut ts_bytes = [0u8; 8];
        ts_bytes.copy_from_slice(&data[0x1A..0x22]);
        let timestamp = if big_endian {
            f64::from_be_bytes(ts_bytes)
        } else {
            f64::from_le_bytes(ts_bytes)
        };

        // Version 1 (the first game, 1997) stores the **resource count N**
        // at 0x1E and starts its offsets table at **0x20**, four bytes
        // earlier than versions 2 and 3 (which store N at 0x22 and start the
        // table at 0x24). The table holds `N + 1` entries and its last entry
        // is the file size, which is what pins both the count and the base
        // down: in every shipped version-1 container `read_u32(0x20 + 4*N)`
        // equals the file length. Measured on all nine v1 containers
        // (HD.PIC N=1682, mpApp.PIC 384, pack.PIC 10893, Topolino.CPK 2,
        // HD.SND 66, mpApp.SND 152, pack.SND 234, pack.BLB 305, HD.mms 295,
        // pack.mms 988).
        let resource_count = if version == 1 {
            read_u16(&data, 0x1e, big_endian)
        } else {
            read_u16(&data, 0x22, big_endian)
        };
        let n = resource_count as usize;

        let off_base = if version == 1 { 0x20 } else { 0x24 };
        let need = off_base + 4 * (n + 1);
        if data.len() < need {
            bail!(
                "{}: truncated offset table (need {}, have {})",
                path.display(),
                need,
                data.len()
            );
        }
        let mut offsets = Vec::with_capacity(n + 1);
        for i in 0..=n {
            offsets.push(read_u32(&data, off_base + 4 * i, big_endian));
        }

        let payload_start = offsets.first().copied().unwrap_or(0) as usize;

        // Scripts containers have no names table and no picture metadata.
        // Their serialized object database starts immediately after the
        // offset table, with no padding.
        // Version-1 Script containers carry a different, still-undecoded
        // layout (their object database does not start right after the
        // offsets table). They are rejected rather than guessed at.
        if kind == ContainerKind::Scripts && version != 1 {
            let metadata_start = off_base + 4 * (n + 1);
            let script =
                parse_script_meta(&path, &data, metadata_start, payload_start, n, big_endian)?;
            let names = script_names(&script, n)?;
            return Ok(Container {
                path,
                data,
                kind,
                big_endian,
                version,
                file_id,
                timestamp,
                resource_count,
                offsets,
                names,
                attr: Vec::new(),
                flags: Vec::new(),
                dimensions: Vec::new(),
                sizes: vec![0; n],
                extra_u32: Vec::new(),
                kinds: Vec::new(),
                names_end: metadata_start,
                palettes: Vec::new(),
                script: Some(script),
            });
        }

        // Version 1 (the first game, 1997) keeps the later versions' shape --
        // 2 bytes of padding after the offsets table, then 32-byte name
        // fields -- but its per-resource metadata region is different and
        // *smaller* in places. It is measured from the loader in
        // `DMA1.exe` (`0x4183xx`, the `MMPictureResourceFile` reader) and
        // verified against the shipped files: the region between the end of
        // the names and `offsets[0]` is exactly long enough for the arrays
        // below plus the palette bank, and no longer.
        //
        // All three v1 container kinds begin with a `4*N` u32 array. In sound
        // containers the values decode as Mac timestamps (seconds since
        // 1904-01-01): `0xaedb282b` is 1997-04-15, matching the game's build
        // date. In picture containers the same array holds the per-resource
        // id/handle word the loader uses to look a picture up.
        if version == 1 {
            let names_base = off_base + 4 * (n + 1) + 2;
            let names_len = 32 * n;
            if names_base + names_len > data.len() {
                bail!(
                    "{}: version 1 names table does not fit (need {}, have {})",
                    path.display(),
                    names_base + names_len,
                    data.len()
                );
            }
            let mut names = Vec::with_capacity(n);
            for i in 0..n {
                let s = &data[names_base + 32 * i..names_base + 32 * (i + 1)];
                let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
                names.push(String::from_utf8_lossy(&s[..end]).into_owned());
            }
            let names_end = names_base + names_len;

            // The `4*N` header array, then a container-kind-specific tail.
            let base = names_end;
            let need = base + 4 * n;
            if need > data.len() {
                bail!(
                    "{}: version 1 metadata does not fit (need {}, have {})",
                    path.display(),
                    need,
                    data.len()
                );
            }
            let mut extra_u32 = Vec::with_capacity(n);
            for i in 0..n {
                extra_u32.push(read_u32(&data, base + 4 * i, big_endian));
            }

            let mut attr = Vec::new();
            let mut flags = Vec::new();
            let mut dimensions = Vec::new();
            let mut kinds = Vec::new();
            let mut palettes = Vec::new();
            // No size table exists in v1, so every payload reads as "stored
            // uncompressed".
            let sizes = vec![0u32; n];

            let mut cursor = base + 4 * n;
            match kind {
                // Pictures: `N` flags, `N` kind bytes, a `4*N` dimension
                // table, a `4*N` size table, one more `N` byte array, a u16
                // palette count and then `count` 1024-byte palettes.
                ContainerKind::Pictures => {
                    // The 2-byte sub-version marker. Every shipped picture
                    // container reads 7 here (`0x4183`'s reader requires it).
                    if read_u16(&data, cursor, big_endian) != 7 {
                        bail!(
                            "{}: version 1 picture metadata marker is not 7",
                            path.display()
                        );
                    }
                    cursor += 2;
                    flags.reserve(n);
                    for i in 0..n {
                        flags.push(data[cursor + i]);
                    }
                    cursor += n;
                    kinds.reserve(n);
                    for i in 0..n {
                        kinds.push(data[cursor + i]);
                    }
                    cursor += n;
                    dimensions.reserve(n);
                    for i in 0..n {
                        // The pair is {height, width}: the SECOND u16 is the
                        // width, exactly as in versions 2 and 3.
                        let h = read_u16(&data, cursor + 4 * i, big_endian);
                        let w = read_u16(&data, cursor + 4 * i + 2, big_endian);
                        dimensions.push(Dimension {
                            width: w,
                            height: h,
                        });
                    }
                    cursor += 4 * n;
                    // The 4*N array of stored payload sizes. Every shipped
                    // container leaves it all zeros, which is why v1 payloads
                    // read as uncompressed.
                    cursor += 4 * n;
                    // One more N-byte array whose meaning is not yet known.
                    cursor += n;
                    let palette_count = read_u16(&data, cursor, big_endian);
                    cursor += 2;
                    // The palette bank: `palette_count` 1024-byte palettes,
                    // immediately before the first payload. Pictures whose
                    // `kind` byte asks for a palette index into it rather than
                    // carrying one inline, which is the only way the
                    // `palette_of` codes below can find one.
                    palettes.reserve(palette_count as usize);
                    for k in 0..palette_count as usize {
                        let at = cursor + k * PALETTE_BYTES;
                        let want = at + PALETTE_BYTES;
                        if want > data.len() {
                            bail!(
                                "{}: palette bank overruns the file (need {}, have {})",
                                path.display(),
                                want,
                                data.len()
                            );
                        }
                        palettes.push(data[at..want].to_vec());
                    }
                    cursor += palette_count as usize * PALETTE_BYTES;
                }
                // Sounds: one `N` byte array (the format byte) and 2 bytes of
                // padding. The 4*N array ahead of it holds the Mac
                // timestamps.
                ContainerKind::Sounds => {
                    attr.reserve(n);
                    for i in 0..n {
                        attr.push(data[cursor + i]);
                    }
                    cursor += n + 2;
                }
                // Blobs: nothing beyond the 4*N array plus 2 bytes.
                ContainerKind::Blobs => cursor += 2,
                // Scripts: the layout is still undecoded, so nothing is read.
                _ => {}
            }
            if cursor != payload_start {
                bail!(
                    "{}: version 1 metadata ends at {} but the first payload starts at {}",
                    path.display(),
                    cursor,
                    payload_start
                );
            }

            return Ok(Container {
                path,
                data,
                kind,
                big_endian,
                version,
                file_id,
                timestamp,
                resource_count,
                offsets,
                names,
                attr,
                flags,
                dimensions,
                sizes,
                extra_u32,
                kinds,
                names_end,
                palettes,
                script: None,
            });
        }

        // 2 bytes of padding after the offset table.
        let names_base = off_base + 4 * (n + 1) + 2;

        // Resource names: 32 bytes each, NUL padded.
        let mut names = Vec::with_capacity(n);
        for i in 0..n {
            let s = &data[names_base + 32 * i..names_base + 32 * (i + 1)];
            let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
            names.push(String::from_utf8_lossy(&s[..end]).into_owned());
        }

        let names_end = names_base + 32 * n;

        // The metadata region is `15 * N + 4` bytes for picture containers:
        // a 4*N u32 array, two N-byte arrays, 2 bytes of padding, the 4*N
        // dimension table, the 4*N size table, one more N-byte array and
        // 2 bytes of padding. Sounds containers are `14 * N + 2` bytes: they
        // have the extra 2-byte word *before* the two byte arrays and no
        // trailing kind array.
        let meta_len = match kind {
            ContainerKind::Sounds => 14 * n + 2,
            _ => 15 * n + 4,
        };
        let meta_end = names_end + meta_len;
        if meta_end > payload_start || meta_end > data.len() {
            bail!(
                "{}: metadata region does not fit (names_end={}, meta_end={}, payload_start={}, len={})",
                path.display(),
                names_end,
                meta_end,
                payload_start,
                data.len()
            );
        }

        // 4*N u32 array of unknown purpose, then the attribute byte array.
        let extra_base = names_end;
        let mut extra_u32 = Vec::with_capacity(n);
        for i in 0..n {
            extra_u32.push(read_u32(&data, extra_base + 4 * i, big_endian));
        }

        // The attribute byte array (object offset 0x278). In a sound
        // container it sits after the extra 2-byte word instead.
        let byte_arrays = if kind == ContainerKind::Sounds {
            extra_base + 4 * n + 2
        } else {
            extra_base + 4 * n
        };
        let attr_base = byte_arrays;
        let mut attr = Vec::with_capacity(n);
        for i in 0..n {
            attr.push(data[attr_base + i]);
        }

        // The flags byte array (object offset 0x284): bit 0 = 8bpp.
        let flags_base = attr_base + n;
        let mut flags = Vec::with_capacity(n);
        for i in 0..n {
            flags.push(data[flags_base + i]);
        }

        // 2 bytes of padding, then the dimension table. The padding exists
        // only in picture containers.
        let dim_base = if kind == ContainerKind::Sounds {
            flags_base + n
        } else {
            flags_base + n + 2
        };
        let mut dimensions = Vec::with_capacity(n);
        for i in 0..n {
            // The pair is {height, width}: the SECOND u16 is the width.
            let h = read_u16(&data, dim_base + 4 * i, big_endian);
            let w = read_u16(&data, dim_base + 4 * i + 2, big_endian);
            dimensions.push(Dimension {
                width: w,
                height: h,
            });
        }

        // The decompressed-size table.
        let size_base = dim_base + 4 * n;
        let mut sizes = Vec::with_capacity(n);
        for i in 0..n {
            sizes.push(read_u32(&data, size_base + 4 * i, big_endian));
        }

        // The kind byte array (object offset 0x2a8): the palette source.
        // Sounds and Films containers have no kind array.
        let mut kinds = Vec::with_capacity(n);
        if meta_len >= 15 * n + 4 {
            let kind_base = size_base + 4 * n;
            for i in 0..n {
                kinds.push(data[kind_base + i]);
            }
        }

        Ok(Container {
            path,
            data,
            kind,
            big_endian,
            version,
            file_id,
            timestamp,
            resource_count,
            offsets,
            names,
            attr,
            flags,
            dimensions,
            sizes,
            extra_u32,
            kinds,
            names_end,
            palettes: Vec::new(),
            script: None,
        })
    }

    /// Raw (still compressed) bytes of resource `i`.
    pub fn payload(&self, i: usize) -> &[u8] {
        let s = self.offsets[i] as usize;
        let e = self.offsets[i + 1] as usize;
        &self.data[s..e]
    }

    /// Compressed size of resource `i`.
    pub fn payload_len(&self, i: usize) -> usize {
        (self.offsets[i + 1] - self.offsets[i]) as usize
    }

    /// Name of resource `i`.
    pub fn name(&self, i: usize) -> &str {
        self.names.get(i).map(|s| s.as_str()).unwrap_or("")
    }

    /// Pixel dimensions of picture resource `i`.
    pub fn dimension(&self, i: usize) -> Dimension {
        self.dimensions.get(i).copied().unwrap_or_default()
    }

    /// Flags byte of resource `i` (object offset 0x284). Bit 0 selects
    /// 8 bits per pixel.
    pub fn flags_of(&self, i: usize) -> u8 {
        self.flags.get(i).copied().unwrap_or(0)
    }

    /// Attribute byte of resource `i` (object offset 0x278).
    pub fn attr_of(&self, i: usize) -> u8 {
        self.attr.get(i).copied().unwrap_or(0)
    }

    /// Size of resource `i`'s decompressed payload as the container records
    /// it. A zero means the payload is stored uncompressed, in which case the
    /// stored size is the answer.
    pub fn plain_len(&self, i: usize) -> usize {
        let recorded = self.sizes.get(i).copied().unwrap_or(0);
        if recorded == 0 {
            self.payload_len(i)
        } else {
            recorded as usize
        }
    }

    /// Kind byte of resource `i`.
    pub fn kind_of(&self, i: usize) -> u8 {
        self.kinds.get(i).copied().unwrap_or(0)
    }

    /// The palette resource `i` index into, or `None` when it carries its own.
    ///
    /// Version-1 picture containers keep a palette bank at the end of their
    /// metadata region, and the first game's run-length sub-format resources
    /// with one-byte colour records index into it rather than prefixing a
    /// palette to their payload. Entry 0 is the one used: `mpApp.PIC` ships
    /// two palettes and the two agree with the vision checks of the decoded
    /// `dmalog074/079` and `nameLess_230/231` stamps, which all index the low
    /// numbered entries. This is a judgement resting on the vision check, not
    /// a measured field — the container carries no per-resource palette
    /// selector. Empty for versions 2 and 3, which prefix the palette to the
    /// payload instead.
    pub fn palette_of(&self, i: usize) -> Option<&[u8]> {
        let _ = i;
        self.palettes.first().map(|p| p.as_slice())
    }
}

pub fn read_u16(d: &[u8], off: usize, big_endian: bool) -> u16 {
    let b = [d[off], d[off + 1]];
    if big_endian {
        u16::from_be_bytes(b)
    } else {
        u16::from_le_bytes(b)
    }
}

pub fn read_u32(d: &[u8], off: usize, big_endian: bool) -> u32 {
    let b = [d[off], d[off + 1], d[off + 2], d[off + 3]];
    if big_endian {
        u32::from_be_bytes(b)
    } else {
        u32::from_le_bytes(b)
    }
}

/// Parse the serialized object database that follows a Scripts container's
/// offset table.
///
/// Layout, all integers in the container's byte order:
///
/// ```text
/// u16  script version (observed 11)
/// u16  unknown (observed 0xffff)
/// u16  unknown (observed 0xffff)
/// u16  unknown (observed 2490)
/// u16  type count T
/// u16  T x type record size
///      string list: type names (T entries)
/// u32  N x element count
/// u16  N x type index
///      string list: object names (N entries)
/// ```
///
/// The last byte of the name list must coincide with the first payload
/// offset; that is what pins the boundaries down.
fn parse_script_meta(
    path: &Path,
    data: &[u8],
    metadata_start: usize,
    payload_start: usize,
    n: usize,
    big_endian: bool,
) -> Result<ScriptMeta> {
    let mut pos = metadata_start;
    let version = take_u16(data, &mut pos, big_endian)
        .with_context(|| format!("{}: script metadata version", path.display()))?;
    let unknown_1 = take_u16(data, &mut pos, big_endian)?;
    let unknown_2 = take_u16(data, &mut pos, big_endian)?;
    let unknown_3 = take_u16(data, &mut pos, big_endian)?;
    let type_count = take_u16(data, &mut pos, big_endian)?;
    let type_count_usize = type_count as usize;

    let mut type_aux = Vec::with_capacity(type_count_usize);
    for _ in 0..type_count_usize {
        type_aux.push(take_u16(data, &mut pos, big_endian)?);
    }

    let types = read_string_list(data, &mut pos, big_endian)
        .with_context(|| format!("{}: script type names", path.display()))?;
    if types.len() != type_count_usize {
        bail!(
            "{}: script type count says {}, type-name list has {}",
            path.display(),
            type_count_usize,
            types.len()
        );
    }

    let mut element_counts = Vec::with_capacity(n);
    for _ in 0..n {
        element_counts.push(take_u32(data, &mut pos, big_endian)?);
    }

    let mut object_types = Vec::with_capacity(n);
    for _ in 0..n {
        let t = take_u16(data, &mut pos, big_endian)?;
        if t as usize >= type_count_usize {
            bail!(
                "{}: object type index {} is outside the {} entry type table",
                path.display(),
                t,
                type_count_usize
            );
        }
        object_types.push(t);
    }

    let object_names = read_string_list(data, &mut pos, big_endian)
        .with_context(|| format!("{}: script object names", path.display()))?;
    if object_names.len() != n {
        bail!(
            "{}: container has {} resources but {} object names",
            path.display(),
            n,
            object_names.len()
        );
    }

    if pos != payload_start {
        bail!(
            "{}: script metadata ends at 0x{pos:x}, first payload starts at 0x{payload_start:x}",
            path.display()
        );
    }

    Ok(ScriptMeta {
        version,
        unknown_1,
        unknown_2,
        unknown_3,
        type_count,
        type_aux,
        types,
        element_counts,
        object_types,
        object_names,
        metadata_start,
        metadata_end: pos,
    })
}

/// Read the name list belonging to a parsed Scripts metadata block.
fn script_names(meta: &ScriptMeta, n: usize) -> Result<Vec<String>> {
    if meta.object_names.len() != n {
        bail!(
            "script metadata has {} object names for {} resources",
            meta.object_names.len(),
            n
        );
    }
    Ok(meta.object_names.clone())
}

/// Read one length-prefixed string list.
///
/// Wire format: `u16 count`, `u32 blob length`, `count x u32 offset`, then the
/// blob. Offsets are relative to the start of the blob. Strings are NUL
/// terminated.
fn read_string_list(data: &[u8], pos: &mut usize, big_endian: bool) -> Result<Vec<String>> {
    let count = take_u16(data, pos, big_endian)? as usize;
    let blob_len = take_u32(data, pos, big_endian)? as usize;
    let mut offsets = Vec::with_capacity(count);
    for _ in 0..count {
        offsets.push(take_u32(data, pos, big_endian)? as usize);
    }

    let blob_start = *pos;
    let blob_end = blob_start
        .checked_add(blob_len)
        .context("string-list length overflows")?;
    if blob_end > data.len() {
        bail!(
            "string list at 0x{blob_start:x} claims {blob_len} bytes but the file ends at 0x{:x}",
            data.len()
        );
    }

    let mut strings = Vec::with_capacity(count);
    for off in offsets {
        if off > blob_len {
            bail!("string offset {off} is outside a {blob_len} byte blob");
        }
        let start = blob_start + off;
        let rest = &data[start..blob_end];
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        strings.push(String::from_utf8_lossy(&rest[..end]).into_owned());
    }

    *pos = blob_end;
    Ok(strings)
}

fn take_u16(data: &[u8], pos: &mut usize, big_endian: bool) -> Result<u16> {
    if pos.checked_add(2).is_none_or(|end| end > data.len()) {
        bail!("truncated u16 at 0x{pos:x}");
    }
    let v = read_u16(data, *pos, big_endian);
    *pos += 2;
    Ok(v)
}

fn take_u32(data: &[u8], pos: &mut usize, big_endian: bool) -> Result<u32> {
    if pos.checked_add(4).is_none_or(|end| end > data.len()) {
        bail!("truncated u32 at 0x{pos:x}");
    }
    let v = read_u32(data, *pos, big_endian);
    *pos += 4;
    Ok(v)
}
