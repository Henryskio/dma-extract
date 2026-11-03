# Binary audit against the shipped executables

Five specific claims were re-derived **from the binaries only** (REA / Ghidra 12.1.4
over `/RawAssets/DMA4.exe`, and byte-level measurement of the containers carved out
of the 3D game's disc image). Documentation, the Rust source and the Lean source
were used only to know *what* was claimed, never as evidence for the verdict.

Authoritative artifacts actually consulted:

| Artifact | What it is |
| --- | --- |
| `/RawAssets/DMA4.exe` | third game, `sha256 d8ff696e43b965b4a87bb765dbc8a333a0be73f369242cdcd0dab41b9826aeca`, image base `0x400000`, x86-32, MSVC 6 (2002-06-15), 7367 procedures, RTTI present |
| `/RawAssets/DMA4.exe` file offset `0x132778` | the DPCM table (VA `0x532778`) |
| `/RawAssets/Disney Creazioni in 3D (Italy).bin` | the 3D game's disc image, read-only, carved in `/tmp/d3d` |
| `/tmp/d3d` carve of that image | `DMA3D.MMM` (N=460), `DMA3D.MMX` (N=705), `DMA3D.MMF` (N=3) and the rest |

**Not available in this workspace, and what that costs.** Two claims could not be
closed against the binary they name, and both limits are stated in their sections
rather than papered over:

* The **second game's** `DMA.exe` is absent (`find / -name 'DMA*.exe'` returns only
  `DMA4.exe`; `/PathAssets/Dati` holds only first-game containers). Claim 4's
  second-game chunk types `0x00`/`0x01` therefore rest on the container
  measurement in `docs/SEQUEL.md`, not on a decompiled `DMA.exe`.
* The 3D game's **`DMA3D.exe` is a Macintosh PowerPC CodeWarrior build on this
  disc, and the 1,662,976-byte Windows PE that `docs/DMA3D.md` quotes
  (`0x477940`, image base `0x400000`) is not present anywhere in the image.** A
  full-image scan for `MZ`/`PE\x00\x00` returns zero hits; the only
  `MM2DFilmResourceFile` RTTI on the disc is inside the PPC binary. Claim 4's
  `0x477940` addresses are therefore reported as *unverified from bytes here*,
  while everything the loader reads is confirmed against the shipped container.

Everything below is quoted from the REA `analyze_function` output for
`/RawAssets/DMA4.exe`.

---

## 1. Sound format byte `C` (`attr`) decoding — **CORRECT**

### What was claimed

`docs/SOUNDS.md` §2 and `src/sound.rs::format_of` (Lean
`MmfExtract/Sound.lean::formatOf`) decode the sound metadata byte `C` from object
offset `0x278`:

| Bits | Meaning |
| --- | --- |
| 0–1 | `0` = 11025, `1` = 22050, `2` or `3` = 44100 Hz |
| 2 | 16-bit samples |
| 3 | stereo |

### The instructions that read it

The four accessors are inlined siblings in `DMA4.exe`, each a 4-instruction body
reading `[this + 0x278]` and masking it. `REA analyze_function` on each:

```
FUN_004c5b32   (rate code)
  0x4c5b32: MOVSX EAX, word ptr [ESP + 0x4]
  0x4c5b37: MOV   ECX, dword ptr [ECX + 0x278]
  0x4c5b3d: MOV   AL,  byte ptr [EAX + ECX*0x1]
  0x4c5b40: AND   EAX, 0x3          <-- attr & 3
  0x4c5b43: RET   0x4

FUN_004c5b46   (16-bit flag)
  0x4c5b46: MOVSX EAX, word ptr [ESP + 0x4]
  0x4c5b4b: MOV   ECX, dword ptr [ECX + 0x278]
  0x4c5b51: MOV   AL,  byte ptr [EAX + ECX*0x1]
  0x4c5b54: SHR   EAX, 0x2
  0x4c5b57: AND   EAX, 0x1          <-- (attr & 4) >> 2
  0x4c5b5a: RET   0x4

FUN_004c5b5d   (stereo flag)
  0x4c5b5d: MOVSX EAX, word ptr [ESP + 0x4]
  0x4c5b62: MOV   ECX, dword ptr [ECX + 0x278]
  0x4c5b68: MOV   AL,  byte ptr [EAX + ECX*0x1]
  0x4c5b6b: SHR   EAX, 0x3
  0x4c5b6e: AND   EAX, 0x1          <-- (attr & 8) >> 3
  0x4c5b71: RET   0x4

FUN_004c5b74   (auxiliary bits, unused by extraction)
  0x4c5b74: MOVSX EAX, word ptr [ESP + 0x4]
  0x4c5b79: MOV   ECX, dword ptr [ECX + 0x278]
  0x4c5b7f: MOV   AL,  byte ptr [EAX + ECX*0x1]
  0x4c5b82: SHR   EAX, 0x4
  0x4c5b85: AND   EAX, 0x3          <-- (attr & 0x30) >> 4
  0x4c5b88: RET   0x4
```

The rate constants are not an array in `.rdata`; they are materialised by the
select branch in `FUN_004c5dd4` (`0x4c5dd4`), whose call chain reaches it through
`FUN_004c5b8b`:

```
FUN_004c5b8b
  0x4c5b93: CALL 0x4c5b5d        ; stereo
  0x4c5b9c: CALL 0x4c5b46        ; 16-bit
  0x4c5ba5: CALL 0x4c5b32        ; rate code
  0x4c5bae: CALL 0x4c5c64        ; payload size
  0x4c5bb4: CALL 0x4c5dd4        ; (size, rate_code, is_16, is_stereo) -> duration

FUN_004c5dd4  0x4c5dfb: MOV EAX, dword ptr [EBP + 0xc]   ; rate_code
  0x4c5dfe: SUB EAX, 0x0
  0x4c5e01: JZ  0x4c5e14
  0x4c5e03: DEC EAX
  0x4c5e04: JZ  0x4c5e0d
  0x4c5e06: MOV EAX, 0xac44       ; 44100   <- rate_code 2 or 3
  0x4c5e0b: JMP 0x4c5e19
  0x4c5e0d: MOV EAX, 0x5622       ; 22050   <- rate_code 1
  0x4c5e12: JMP 0x4c5e19
  0x4c5e14: MOV EAX, 0x2b11       ; 11025   <- rate_code 0
  0x4c5e19: PUSH ESI / PUSH 0x19 / CDQ / POP ESI / IDIV ESI   ; rate / 25
  0x4c5e20: MOV ESI, EAX
  0x4c5e22: LEA EAX, [ECX + ECX*0x4]  ; len * 5
  0x4c5e25: SHL EAX, 0x3              ; len * 40
  0x4c5e28: CDQ / IDIV ESI            ; -> milliseconds
```

`0xac44 == 44100`, `0x5622 == 22050`, `0x2b11 == 11025`, and the halves applied
earlier in the function (`0x4c5dd7..0x4c5df9`, two `DEC`/`NZ`-guarded `SAR EAX,1`
on the `is_16` and `is_stereo` arguments at `[EBP+0x10]` and `[EBP+0x14]`) confirm
both flags divide the byte count by two.

### Verdict

**CORRECT.** `attr & 3` selects 11025 / 22050 / 44100 with `2` and `3` folding to
44100 (`FUN_004c5dd4`'s `else` arm, `0x4c5e06`), `attr & 4` is 16-bit
(`FUN_004c5b46`), `attr & 8` is stereo (`FUN_004c5b5d`), exactly as
`docs/SOUNDS.md` §2 and `src/sound.rs::format_of` state. Bits 4–5 exist
(`FUN_004c5b74`) and are unused by the extraction path, also as documented.

The rate of a sound resource is therefore *derived*, never read from the file,
and matches the observed `C` values in §2's table (e.g. `0x1d` -> 22050/16/2,
`0x1e` -> 44100/16/2).

---

## 2. The 256-entry DPCM table at `0x532778` and the silence code `0xf8` — **CORRECT**

### What was claimed

`src/sound.rs::DPCM_TABLE` and `lean/src/MmfExtract/Sound.lean::dpcmTable` hold the
256 little-endian `i16` values at file offset `0x132778`; entries 0..47 and
128..175 are absolute, the rest are added to the previous sample of the channel;
`0xf8` is `DPCM_SILENCE`. The decoder `FUN_004c6b80` transliterates to
`decode_dpcm`.

### The binary's table

`REA read_bytes` at VA `0x532778` (image base `0x400000`, so RVA `0x132778`,
file offset `0x132778` — confirmed, the `.data` segment starts at `0x507000` so
the mapping is an identity RVA):

```
0100 0600 1300 2900 4900 7800 b400 0201 6101 d401 5b02 f902 ae03 7d04
6505 6906 8907 c708 240a a10b 3f0d ff0e e310 ec12 1915 6e17 e919 8e1a ...
```

read as little-endian `i16` gives exactly the claimed sequence:

```
[0]=1        [1]=6        [2]=19       [3]=41       [4]=73       [5]=120
[6]=180      [7]=258      [8]=353      [9]=468      [10]=603     [11]=761
[12]=942     [13]=1149    [14]=1381    [15]=1641    [16]=1929    [17]=2247
...
[47]=30826   [48..55]=2048,2304,2560,2816,3072,3328,3584,3840
[56..63]=1024,1152,1280,1408,1536,1664,1792,1920
...
[71]=0  [72]=256 [73]=288 ... [79]=480
[112]=0 [113]=1 ... [119]=7
[128]=-1 [129]=-6 ... [175]=-7
```

### The full 256-entry comparison

Every index was compared against the table transcribed from the two source files
(Rust `DPCM_TABLE` and Lean `dpcmTable`, which are identical):

```
count            = 256
rustMismatches   = 0
leanMismatches   = 0
idx 0..15        = 1,6,19,41,73,120,180,258,353,468,603,761,942,1149,1381,1641
idx 248..255     = 0,-1,-2,-3,-4,-5,-6,-7
```

There are **no differing indices** in either table.

### The silence code `0xf8`

`0xf8 == 248`. The binary's entry `248` is `0`, and the symmetric shadow entry
`0x78 == 120` is also `0`. The decoder reads the table as a signed 16-bit array
indexed by the raw byte, so `0xf8` selects the zero step:

```
FUN_004c6b80  (decompiled by REA)
  for (; param_2 != 0; param_2 = param_2 + -1) {
    bVar1 = *param_1;
    sVar2 = (&DAT_00532778)[bVar1];
    if ((0x2f < (bVar1 & 0x7f)) && (bVar1 != 0xf8)) {     <-- silence code 0xf8
      sVar2 = sVar2 + *(short *)((int)this + iVar3 * 2);
    }
    *(short *)((int)this + iVar3 * 2) = sVar2;
    *param_3 = sVar2;
    iVar3 = (iVar3 + 1) % *(int *)((int)this + 8);
    ...
```

`&DAT_00532778` is the table's address. `src/sound.rs::decode_dpcm` mirrors this
line for line:

```rust
let mut sample = DPCM_TABLE[usize::from(code)];
if (code & 0x7f) > 0x2f && code != DPCM_SILENCE {   // DPCM_SILENCE == 0xF8
    sample = sample.wrapping_add(previous[channel]);
}
```

The callers confirm the table's role is per-channel: `FUN_004c0651` keeps
`[this+0x10]` as a remaining-count, passes `this+0x14` as the predictor array and
`[this+8]` as the channel count; `FUN_004c6c5b` does the same through a
`[this+4]`-vtable stream reader. Both feed `FUN_004c6b80` one byte at a time.

### Verdict

**CORRECT.** The 256 `i16` at `0x532778`/file offset `0x132778` match
`dpcmTable` in `lean/src/MmfExtract/Sound.lean` and `DPCM_TABLE` in
`src/sound.rs` at **every index, zero differences**. `0xf8` is the silence code:
the decoder compares the incoming byte against `0xf8` at `FUN_004c6b80` and
suppresses the predictor feedback, and the table entry for `0xf8` is `0`.
`DPCM_SILENCE = 0xF8` and `dpcmSilence = 0xF8` are both right.

---

## 3. The LZ77 decompressor `FUN_004c557a` — **CORRECT**

### What was claimed

`docs/PICTURES.md` §1 and `lean/src/MmfExtract/Lz77.lean` describe four
control-byte forms dispatched by the match table at `0x4c562d`, plus an
end-of-stream marker at `0x4c56bd`:

| control | meaning |
| --- | --- |
| `0x00..0x0F` | 2-byte match, distance `(O0 << 2) + (C >> 2) + 1` |
| `0x10..0x1F` | 3-byte match, distance `(O0 >> 2) + (O1 << 6) + 0x4000+` |
| `0x20..0x3F` | match, length `C & 0x1F` (0 = extended), 2-byte offset |
| `0x40..0xFF` | match, length `(C >> 5) + 1`, 1-byte offset |

Plus: `C > 0x11` copies `C - 0x11` literals first (`0x4c5593`); after a literal
run a control byte below `0x10` selects a 3-byte match at distance
`(O0 << 2) + (C >> 2) + 0x801` (`0x4c55ef`); after a match, the low two bits of
the byte two back in the input give trailing literal count (`0x4c5616`); extended
lengths are runs of `0x00` each worth `0xFF` then a final byte.

### The dispatch instructions, quoted

Entry and the literal-run prologue:

```
0x4c557a: PUSH EBX / PUSH ESI / PUSH EDI
0x4c557c: MOV  ECX, dword ptr [ESP + 0x14]      ; ecx = output cursor
0x4c5581: MOV  EDI, dword ptr [ESP + 0x10]      ; eax = input pointer
0x4c5585: MOV  EBX, 0xff                        ; the 0xFF constant for extension
0x4c558c: MOV  DL,  byte ptr [EDI]
0x4c558e: CMP  DL,  0x11
0x4c5591: JBE  0x004c55ac                       ; <= 0x11 -> literal control
0x4c5593: MOVZX ESI, DL
0x4c5596: SUB  ESI, 0x11                        ; ESI = C - 0x11  (literal count)
0x4c5599: LEA  EAX, [EDI + 0x1]
0x4c559c: CMP  ESI, 0x4
0x4c559f: JC   0x004c5620                       ; fewer than 4 literals -> no selector
0x4c55a1: MOV DL, byte ptr [EAX] / MOV byte ptr [ECX], DL / INC ECX / INC EAX / DEC ESI
0x4c55a8: JNZ  0x004c55a1                       ; copy C-0x11 literals
0x4c55aa: JMP  0x004c55e6                       ; -> after-literal-run state
```

Literal-run form (`0x00..0x0F` at a control position):

```
0x4c55ac: MOVZX ESI, byte ptr [EAX] / INC EAX
0x4c55b0: CMP  ESI, 0x10
0x4c55b3: JNC  0x004c562d                       ; >= 0x10 -> the match table
0x4c55b5: TEST ESI, ESI / JNZ 0x004c55cb        ; C != 0 -> direct length
0x4c55bb: ADD  ESI, EBX / INC EAX               ; each 0x00 adds 0xff
0x4c55be: CMP  byte ptr [EAX], 0x0 / JZ 0x004c55bb
0x4c55c3: MOVZX EDX, byte ptr [EAX] / INC EAX
0x4c55c7: LEA  ESI, [ESI + EDX*0x1 + 0xf]       ; + 0x0f
0x4c55cb: three literal bytes (DL/INC x3), then DEC ESI / JNZ 0x004c55dd
```

So the literal run copies **3 + len** bytes, with `len` extended from `0x0f` when
`C == 0` — matching `Lz77.lean`'s `literalRun` and `extendedLen`.

The three-byte special after a literal run:

```
0x4c55e6: MOVZX ESI, byte ptr [EAX] / INC EAX
0x4c55ea: CMP  ESI, 0x10
0x4c55ed: JNC  0x004c562d                       ; >= 0x10 -> the match table
0x4c55ef: MOVZX EDX, byte ptr [EAX]
0x4c55f2: SHL  EDX, 0x2                         ; O0 << 2
0x4c55f5: MOV  EDI, ECX
0x4c55f7: SUB  EDI, EDX
0x4c55f9: SHR  ESI, 0x2                         ; C >> 2
0x4c55fc: SUB  EDI, ESI
0x4c55fe: SUB  EDI, 0x801                       ; distance = (O0<<2) + (C>>2) + 0x801
0x4c5604: INC  EAX
0x4c5605..0x4c560f: one byte at [ECX]
0x4c560b..0x4c5615: two more bytes at [EDI], [EDI+1]   ; 3-byte match
```

The trailing-literal selector after a match:

```
0x4c5616: MOV  DL, byte ptr [EAX + -0x2]        ; two back in the input
0x4c5619: AND  EDX, 0x3
0x4c561c: MOV  ESI, EDX
0x4c561e: JZ   0x004c55ac                       ; 0 -> literal control state
0x4c5620: MOV DL, byte ptr [EAX] / MOV byte ptr [ECX], DL / INC ECX / INC EAX / DEC ESI / JNZ
0x4c5629: MOVZX ESI, byte ptr [EAX] / INC EAX   ; next control byte
0x4c562d: (the match table)
```

**The match table at `0x4c562d`:**

```
0x4c562d: CMP  ESI, 0x40
0x4c5630: JC   0x004c564e                       ; < 0x40 -> narrower forms
  ; ---- 0x40..0xFF: 1-byte offset, length (C>>5)+1
0x4c5632: MOV  EDX, ESI / MOV EDI, ECX
0x4c5636: SHR  EDX, 0x2
0x4c5639: AND  EDX, 0x7                         ; (C >> 2) & 7
0x4c563c: SUB  EDI, EDX
0x4c563e: MOVZX EDX, byte ptr [EAX]
0x4c5641: SHL  EDX, 0x3                         ; O0 << 3
0x4c5644: SUB  EDI, EDX
0x4c5646: DEC  EDI                              ; distance = ((C>>2)&7) + (O0<<3) + 1
0x4c5648: SHR  ESI, 0x5
0x4c564b: DEC  ESI                              ; length = (C>>5) + 1
0x4c564c: JMP  0x004c56cc                       ; -> the copy loop (length+1 iterations)

0x4c564e: CMP  ESI, 0x20
0x4c5651: JC   0x004c5682                       ; < 0x20 -> 0x10..0x1F form
  ; ---- 0x20..0x3F: length C & 0x1F (0 = extended from 0x1f), 2-byte offset
0x4c5653: AND  ESI, 0x1f
0x4c5656: JNZ  0x004c566a
0x4c565a: ADD  ESI, EBX / INC EAX               ; 0x00 -> +0xff each
0x4c565d: CMP  byte ptr [EAX], 0x0 / JZ 0x004c565a
0x4c5662: MOVZX EDX, byte ptr [EAX] / INC EAX
0x4c5666: LEA  ESI, [ESI + EDX*0x1 + 0x1f]      ; + 0x1f
0x4c566a: MOVZX EDX, byte ptr [EAX + 0x1]
0x4c566e: SHL  EDX, 0x6                         ; O1 << 6
0x4c5671: MOV  EDI, ECX
0x4c5673: SUB  EDI, EDX
0x4c5675: MOVZX EDX, byte ptr [EAX]
0x4c5678: SHR  EDX, 0x2                         ; O0 >> 2
0x4c567b: SUB  EDI, EDX
0x4c567d: DEC  EDI                              ; distance = (O0>>2) + (O1<<6) + 1
0x4c567e/0x4c567f: INC EAX, INC EAX             ; two offset bytes
0x4c5680: JMP  0x004c56cc

0x4c5682: CMP  ESI, 0x10
0x4c5685: MOV  EDI, ECX
0x4c5687: JC   0x004c56e6                       ; < 0x10 -> 0x00..0x0F form
  ; ---- 0x10..0x1F: length C & 7 (0 = extended from 7), 2-byte offset, 16/32 KiB
0x4c5689: MOV  EDX, ESI
0x4c568b: AND  EDX, 0x8
0x4c568e: SHL  EDX, 0xb                         ; 0x8 << 11 == 0x4000 when bit 3 set
0x4c5691: SUB  EDI, EDX
0x4c5693: AND  ESI, 0x7
0x4c5696: JNZ  0x004c56aa
0x4c569a: ADD  ESI, EBX / INC EAX               ; 0x00 -> +0xff each
0x4c569d: CMP  byte ptr [EAX], 0x0 / JZ 0x004c569a
0x4c56a2: MOVZX EDX, byte ptr [EAX] / INC EAX
0x4c56a6: LEA  ESI, [ESI + EDX*0x1 + 0x7]       ; + 7
0x4c56aa: MOVZX EDX, byte ptr [EAX]
0x4c56ad: MOVZX EBX, byte ptr [EAX + 0x1]
0x4c56b1: SHR  EDX, 0x2                         ; O0 >> 2
0x4c56b4: SHL  EBX, 0x6                         ; O1 << 6
0x4c56b7: ADD  EDX, EBX
0x4c56b9: SUB  EDI, EDX                         ; distance = (O0>>2)+(O1<<6) - (0x4000 if bit3)
0x4c56bb: INC  EAX / INC EAX
0x4c56bd: CMP  EDI, ECX
0x4c56bf: JZ   0x004c56fa                       ; ---- END OF STREAM ----
0x4c56c1: SUB  EDI, 0x4000                      ; the base 0x4000 window step
0x4c56c7: MOV  EBX, 0xff
0x4c56cc: (the copy loop)

0x4c56e6: MOVZX EDX, byte ptr [EAX]
0x4c56e9: SHL  EDX, 0x2                         ; O0 << 2
0x4c56ec: SUB  EDI, EDX
0x4c56ee: SHR  ESI, 0x2                         ; C >> 2
0x4c56f1: SUB  EDI, ESI
0x4c56f3: DEC  EDI                              ; distance = (O0<<2) + (C>>2) + 1
0x4c56f4: INC  EAX
0x4c56f5: JMP  0x004c560b                       ; 2-byte match
```

The shared copy loop is `0x4c56cc..0x4c56df`: two bytes unconditionally, then
`length - 1` more, one at a time — byte-at-a-time so overlapping back-references
work.

### The end-of-stream marker at `0x4c56bd`

Verified exactly: `CMP EDI, ECX; JZ 0x004c56fa` (the epilogue `POP EDI/POP ESI/
POP EBX/RET`). In the `0x10..0x1F` form this fires precisely when
`(O0>>2) + (O1<<6) == 0` and bit 3 is clear, i.e. a zero distance with the 16 KiB
window. `Lz77.lean::threeByteOffset` encodes this as `base == 0 && (c & 8) == 0`
and `compress` closes a stream with `0x11 0x00 0x00` — control `0x11` (bit 3
clear, length `0x11 & 7 = 1`), two zero offset bytes, base `0`, so the marker
fires.

### One discrepancy worth naming, on a point the claim does not cover

The `0x10..0x1F` form's distance is **not** `(O0 >> 2) + (O1 << 6) + 0x4000` in
the sense of one extra window step. The assembly subtracts the offset **then**
compares, and only afterwards subtracts the `0x4000` base:

* distance used for the end-of-stream test = `(O0>>2) + (O1<<6)` minus
  `(0x4000 if bit 3 set)`;
* distance used for the actual copy = `0x4000 + (O0>>2) + (O1<<6)` minus
  `(0x4000 if bit 3 set)`.

`Lz77.lean::threeByteOffset` computes `base + 0x4000 + (bit3 ? 0x4000 : 0)`,
which is the same total. So the Lean implementation is right; only the prose
table's `+ 0x4000+` shorthand is loose. **This does not change the verdict** on
the four forms and the marker, which is what claim 3 asks about.

### Verdict

**CORRECT.** The four control-byte forms dispatched at `0x4c562d`
(`0x00..0x0F` 2-byte, `0x10..0x1F` 3-byte with the 16/32 KiB window, `0x20..0x3F`
2-byte offset, `0x40..0xFF` 1-byte offset), the literal prologue at `0x4c5593`,
the `0x801` three-byte match at `0x4c55ef`, the trailing-literal selector at
`0x4c5616`, the extended lengths, and the end-of-stream marker at `0x4c56bd` all
match `lean/src/MmfExtract/Lz77.lean` and `docs/PICTURES.md` §1.

---

## 4. The film metadata layout and chunk types — layout CORRECT, `0x477940` NOT VERIFIED, second-game types NOT VERIFIED

### 4a. The per-film tables and the rect field order — **CORRECT** (measured on the shipped container)

`/RawAssets/DMA4.exe` is the third game, so `MM2DFilmResourceFile`'s loader at
`0x477940` (an address in the 3D game's Windows PE, absent here — see the header)
could not be disassembled. The layout it is claimed to read was instead checked
**field by field against the shipped `DMA3D.MMF`** (N=3, carved from the disc
image at disc offset `0x2f7f160`, container length `0x8e8e50`, films
`introAxDisney.flm`, `introAxImageBuilder.flm`, `introAxSplash.flm`):

```
names_end = 0x24 + 4*(N+1) + 2 + 32*N = 0x2f7f1f6
generic 4*N u32      [0]     3050513359, 3050515809, 3063772596
marker   (u16)               7                     <- must be 7
table 1  (N bytes)           1, 3, 1
table 2  (N bytes)           2, 3, 4
rects    (4*N u16)           (0, 0, 480, 640)
                              (0, 27, 441, 616)
                              (0, 0, 480, 640)
table 4  (N u32)             0x3525, 0x4118, 0x139e4
table 5  (4*N bytes)         77,0,0,0, 77,0,0,0, 77,0,0,0
table 6  (N u16)             1500, 1500, 1500
table 7  (N u32)             0x21860, 0x217e0, 0x21a40
table 8  (N u16)             250, 250, 1000
palette count (u16)          3
remaining bytes              3072 == 1024 * 3      <- exactly
```

Every element of the claimed order and size is present, in the claimed order, and
the block ends exactly at the first payload (`0x2f7fe54`, matching `off[0] =
0xcf4`). The total is `4*N + 2 + 2*N + 8*N + 4*N + 4*N + 2*N + 4*N + 2*N + 2`
+ `1024*P` = `26*N + 2 + 1024*P`, agreeing with `docs/FILMS.md` §2.

**The rect array and its field order.** `tables_start = names_end + 4*N + 2`;
the rect for film `i` sits at `tables_start + 2*N + «8 * i»`:

```
rect[0] @ tables_start + 6 + 0   -> top=0, left=0,    bottom=480, right=640  => 640 x 480
rect[1] @ tables_start + 6 + 8   -> top=0, left=27,   bottom=441, right=616  => 589 x 441
rect[2] @ tables_start + 6 + 16  -> top=0, left=0,    bottom=480, right=640  => 640 x 480
```

Read as `{top, left, bottom, right}`, big-endian, this gives the documented
geometries exactly — including film 1's `589` width, the detail `docs/FILMS.md`
§2 and `docs/DMA3D.md` §6 both call the proof of the order (reading it
`{left, top, right, bottom}` instead would yield 636x379). Film 0 and film 2 are
640x480 = the rect's own `right - left` x `bottom - top`.

The documented rects for `introAxImageBuilder` (`60, 107, 486, 696`, 589 wide)
describe a **different cut of the same film** — that row of `docs/DMA3D.md` §6
matches neither of the two 589-wide values this Italian container actually holds
(`0, 27, 441, 616`). See "second discrepancy" below.

### 4b. The film chunk types — the third game's are **CORRECT**; the second game's are **NOT VERIFIED**

The chunk framing is `tag(u16) kind(u16) len(u32)`, tag `0xF789`, per
`src/film.rs::parse_chunks`. Scanning every `f7 89` in each of the three shipped
`.flm` payloads and reading the following kind word:

| kind | film 0 | film 1 | film 2 | `src/film.rs` name |
| --- | --- | --- | --- | --- |
| `0x02` | 54 | 52 | 838 | `TYPE_AUDIO` |
| `0x03` | — | 1 | 1 | `TYPE_MARKER` |
| `0x06` | 41 | 55 | 784 | `TYPE_TIMESTAMP` |
| `0x07` | — | 1 | 1 | `TYPE_END` |
| `0x0a` | 33 | 13 | 12 | `TYPE_FULL_FRAME` (third game) |
| `0x0b` | 9 | 40 | 735 | `TYPE_DELTA` (third game) |

(The additional kinds — `0x5c4`, `0x91a`, `0x9d2`, … — are all coincidental
`f7 89` byte pairs occurring inside compressed frame data; each has a length that
overruns the payload, and the false positives scale with payload size, 1 in film
0 against ~120 in film 2's 8.25 MB stream.)

So **full frame `0x0A`, delta `0x0B`, timestamp `0x06`, audio `0x02` and tag
`0xF789` are all confirmed on the third game's container**, and the two extra
kinds `src/film.rs` also declares (`0x03`, `0x07`) occur exactly once each in
films 1 and 2, as one would expect for a marker and an end-of-stream marker.

**The second game's `0x00` / `0x01` could not be verified.** The only version-2
Films container reachable from this workspace would come from `DMA.exe`, which
is absent; `/RawAssets/DMA.MMF` is a **version-3** container (`0x12` = `0x0003`,
`N = 1`), i.e. the *third* game's films, so it exercises the `0x0A`/`0x0B` path
and says nothing about the second game. `docs/SEQUEL.md` records `0x00`/`0x01`
from the second game's own binary; that claim is unverified here.

### 4c. Discrepancies found (both in `docs/`, neither in the Lean/Rust code)

1. **`docs/DMA3D.md` §6's rect table does not match the shipped Italian
   container.** It records

   | film | documented rect | measured rect (this Italian disc) |
   | --- | --- | --- |
   | `introAxDisney` | `0, 0, 600, 800` | `0, 0, 480, 640` |
   | `introAxImageBuilder` | `60, 107, 486, 696` | `0, 27, 441, 616` |
   | `introAxSplash` | `60, 80, 540, 720` | `0, 0, 480, 640` |

   both with width 640/589/640 respectively. The three rects I measured from
   `DMA3D.MMF` on this disc are `(0,0,480,640)`, `(0,27,441,616)`,
   `(0,0,480,640)`. The two sets agree that the films are 640 wide and that film
   1 is 589 wide, but the actual top/left/bottom/right values differ, so at most
   one of the two is from an Italian-disc container. The `589` figure is
   corroborated by both, which is the part that proves the field order; the
   absolute offsets are the part that disagrees. This needs re-checking against
   whichever disc `docs/DMA3D.md` §6 was measured from (the prior agent worked at
   `/tmp/dma3d/pc`, the English disc) — a page in `docs/DMA3D.md` §6 and a row in
   `tests/film_3d.rs`, not a decoder.

2. **`docs/DMA3D.md` §6 says "each confirmed by its own key-frame chunk's
   declared decoded size".** That check cannot be reproduced against this disc's
   payloads without decompressing the film chunks; my structured walk of film 2
   parsed the first four `0x06` + three `0x0b` chunks cleanly and then met a bad
   tag, because the frame bodies are not plain data. So the rect-to-declared-size
   link is *measured but not reproduced here*, and I report it as such rather
   than asserting it either way.

### Verdict on claim 4

**Layout: CORRECT** — marker 7, two N-byte tables, `4*N` big-endian u16, `N` u32,
`4*N` raw bytes, `N` u16, `N` u32, `N` u16, u16 palette count, and the rects as
`{top, left, bottom, right}` big-endian at `tables_start + 2*N + 8*index`, all
confirmed byte-for-byte on the shipped `DMA3D.MMF`. **Chunk types: CORRECT for the
third game** (`0x0a`, `0x0b`, `0x06`, `0x02`, tag `0xF789`) on the shipped
container. **NOT VERIFIED from bytes in this workspace:** the `0x477940` loader
disassembly (its Windows PE is not on this disc) and the second game's
`0x00`/`0x01` (`DMA.exe` absent, `/RawAssets/DMA.MMF` is version 3).

---

## 5. The 3D metadata region lengths — **CORRECT** (both source values; `docs/DMA3D.md` is WRONG on one)

### What was claimed

`src/dma3d.rs` implements `models_meta_len(N) == 4*N + 100` (a 100-byte trailer)
and `textures_meta_len(N) == 6*N + 2`.

### Measured against the shipped containers

Both containers were carved from the 3D game's disc image
(`/tmp/d3d/Disney Creazioni in 3D (Italy).bin`, offset from the `MMFW ` header):

**`DMA3D.MMM` (Models), N = 460**

```
header           0x39baac0, sig "MMFW Models", endian "MM", version 3, N = 460
off[0..3]        0x486e, 0x48da, 0x4fb2, 0x549a     (strictly increasing)
names_start      0x24 + 4*(N+1) + 2               = 0x39bb21a
names_end        names_start + 32*N               = 0x39beb9a
first payload    container + off[0]                = 0x39bf32e
names_end -> offsets[0]                           = 1940 bytes
4*N + 100                                        = 1940   <-- matches
4*N + 2                                          = 1842   <-- does not
6*N + 2                                          = 2762   <-- does not
```

**`DMA3D.MMX` (Textures), N = 705**

```
header           0x3f474a0, sig "MMFW Textures", endian "MM", version 3, N = 705
off[0..3]        0x73d6, 0xb796, 0xc036, 0xc87a
names_start      0x24 + 4*(N+1) + 2               = 0x3f47fce
names_end        names_start + 32*N               = 0x3f4d7ee
first payload    container + off[0]                = 0x3f4e876
names_end -> offsets[0]                           = 4232 bytes
6*N + 2                                            = 4232   <-- matches
4*N + 100                                          = 2920   <-- does not
32*N + 4*N + 2*N + 2   (= 38*N + 2)               = 26792  <-- does not
```

Both measurements are the arithmetic the audit asked for: `names_end` to
`offsets[0]`, on the shipped files, with the offsets read as big-endian u32 from
`0x24`.

### The internal structure, as measured

**Models (`DMA3D.MMM`), region 1940 bytes.** The first `4*N = 1840` bytes are a
column of 460 big-endian u32 that is **not** a timestamp column: the first entry
is `778923876` and the remaining 459 are all `0`. The remaining 100 bytes are the
trailer: 32-byte-padded model names (`shrBtQuit.mod`, `shrDgAnotherTextbox.mod`,
`shrDgParentalBtCheckboxN.mod`), then a truncated `shrDgPA9`, an `0xe2ee` marker,
seven `0x00`, and a 16-byte GUID `{780F7A49-3ED3-6511-9D9B-2D366EFAB8}`. So the
trailer *contains* names, not a `u16 2, u16 1, u32 0, 4-byte timestamp, 30-byte
"DMA3D.MMX", 58 bytes 0xff` record.

The `src/dma3d.rs` doc comment on `MODELS_META_TRAILER` describes exactly that
other content ("`u16 2`, `u16 1`, `u32 0`, the 4-byte timestamp of the textures
container, a 30-byte NUL-padded name field holding `DMA3D.MMX`, then 58 bytes of
`0xff` padding"). I read the trailer's actual 100 bytes:

```
73 68 72 44 47 41 6e 6f 74 68 65 72 54 65 78 74 62 6f 78 2e 6d 6f 64 00 ...   "shrDgAnotherTextbox.mod"
73 68 72 44 47 50 61 72 65 6e 74 61 6c 42 74 43 68 65 63 6b 62 6f 78 4e 2e 6d 6f 64 00 00 00 00
73 68 72 44 47 50 41 39 e2 ee 00 00 00 00 00 00 00 00 49 7a 0f 78 d3 3e 13 65 5f ce 11 9d 9b 2d 36 6e fa b8
```

`u16` at +0 is `0x7368` (`"sh"`), not `2`; the `0xff` region is the GUID, not
`0xff` padding. **So the *length* is right but the trailer's *description* is
wrong** — and the wrong part is a measurement that was presumably taken from the
English disc. The `DMA3D.MMX` cross-reference that `docs/DMA3D.md` §7 lists as
measured is **not in this Italian container's trailer at all** (the string
`DMA3D.MMX` appears at `0x1713d`/`0x171a2b` in the disc's HFS catalog, and at
`0x20d0534` inside the executable's resources, but never inside
`DMA3D.MMM`'s 1940-byte region).

**Textures (`DMA3D.MMX`), region 4232 bytes.** The first `4*N = 2820` bytes are
again not a timestamp column; the region is dominated by 32-byte-padded `.TGA`
names (`dialogBtOk.2.TGA`, `dog.TGA`, `drum.TGA`, `tNew.TGA`, `tQuit.1..4.TGA`,
`shrB*`, `shrD*`). The tail is a column of `0xb5b*` values
(`b5bd8235, b5bd823b, b5bd8241, b5bd8247`, …), one per resource — these look
like offsets into a shared texture atlas rather than timestamps. So
`textures_meta_len`'s `6*N + 2` shape (`4*N` + `2*N` + `2`) is plausible in total
but the intermediate split into "4*N u32 timestamps, then two N-byte arrays read
as `attr` and `flags`" is contradicted by the bytes: the region is names first.

### `docs/DMA3D.md` is wrong on Textures

`docs/DMA3D.md` §2.1 states the Textures region as
`32*N + 4*N + 2*N + 2` (`38*N + 2`), and 26,792 bytes for N=705. The measured
region is **4232 bytes**, i.e. `6*N + 2`. The doc itself flags the history at
§2.1 line 59 ("The `Textures` region was measured **wrong** at first, as
`6*N + 2`") and then carries forward the `38*N + 2` figure in the table — which
is the value now shown to be wrong. **`src/dma3d.rs::textures_meta_len` (`6*N + 2`)
is the one that agrees with the shipped container, and `docs/DMA3D.md` §2.1 must
be corrected to it.** No Lean file carries either value (`grep` for `100`,
`6 * `, `4 * ` across `lean/src/MmfExtract/Dma3d/` and `Dma3d.lean` returns
nothing), so no Lean correction is needed.

### Verdict

**CORRECT for both implementations.** `models_meta_len(N) == 4*N + 100` measures
1940 on `DMA3D.MMM` (N=460) and `textures_meta_len(N) == 6*N + 2` measures 4232
on `DMA3D.MMX` (N=705), each equal to `names_end` → `offsets[0]` on the shipped
files. `src/dma3d.rs` needs no change to either function.

Three **documentation** corrections follow, none of them a code change:

1. `docs/DMA3D.md` §2.1's Textures row (`32*N + 4*N + 2*N + 2`, 26,792 for
   N=705) — replace with `6*N + 2`, 4232. `src/dma3d.rs` is already right.
2. `src/dma3d.rs`'s doc comment on `MODELS_META_TRAILER` (the "u16 2, u16 1, u32
   0, timestamp, `DMA3D.MMX`, 58 bytes `0xff`" description) — the trailer is
   names plus a GUID on this disc.
3. `docs/DMA3D.md` §6's three rects and §7's "the `DMA3D.MMX` reference inside
   the models' trailer" — the shipped Italian `DMA3D.MMF` holds rects
   `(0,0,480,640)`, `(0,27,441,616)`, `(0,0,480,640)`, and no `DMA3D.MMX`
   reference inside `DMA3D.MMM`'s trailer.

---

## Summary of verdicts

| # | Claim | Verdict | Evidence |
| --- | --- | --- | --- |
| 1 | `attr & 3` → 11025/22050/44100, `attr & 4` → 16-bit, `attr & 8` → stereo | **CORRECT** | `FUN_004c5b32` `AND EAX,0x3` @0x4c5b40; `FUN_004c5b46` @0x4c5b54; `FUN_004c5b5d` @0x4c5b6b; `FUN_004c5dd4` `0x2b11/0x5622/0xac44` @0x4c5e06–0x4c5e14 |
| 2 | 256-entry DPCM table @0x532778 / 0x132778, silence `0xf8` | **CORRECT** | 256 `i16` read, 0 differing indices vs `dpcmTable`; `FUN_004c6b80` compares `0xf8` |
| 3 | LZ77 four control forms @0x4c562d, end marker @0x4c56bd | **CORRECT** | full instruction listing `0x4c557a..0x4c56fd` quoted above; `CMP EDI,ECX/JZ` @0x4c56bd |
| 4 | Film metadata order/sizes, rects `{top,left,bottom,right}`, third-game chunk types | **Layout CORRECT, chunk types CORRECT (third game); `0x477940` and second-game `0x00`/`0x01` NOT VERIFIED** | field-by-field parse of shipped `DMA3D.MMF`; `f7 89` kind scan of all three `.flm` payloads |
| 5 | `models_meta_len = 4*N+100`, `textures_meta_len = 6*N+2` | **CORRECT** | 1940 vs `4*460+100`; 4232 vs `6*705+2`, both `names_end`→`offsets[0]` |

**Nothing in `lean/src/MmfExtract/` needs correcting for claims 1, 2, 3 or 5.**
Claim 4 asserts a loader disassembly that could not be produced from any binary in
this workspace; its runtime-observed parts (the layout, the rect order, the third
game's chunk types) were all confirmed instead. The corrections that *are*
required are in the documentation: `docs/DMA3D.md` §2.1's Textures region length
(WRONG — correct value `6*N + 2`, matching `src/dma3d.rs`, not `38*N + 2`),
`src/dma3d.rs`'s `MODELS_META_TRAILER` doc comment, and `docs/DMA3D.md` §6's
rect table + §7's trailer claim.
