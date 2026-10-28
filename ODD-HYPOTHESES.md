# Odd-payload sub-format — hypothesis ledger

**SOLVED on 2026-10-28.** The format is fully determined; see
[§ SOLUTION](#solution-solved-2026-10-28) below. Everything above that heading
is the historical ledger of what was tried, what failed, and why.

Working notes for the investigation of the 105 picture resources in `HD.MMP`
(game 3) and the ~1,181 in game 2 whose decoded length matches no known pixel
layout. `ODD-FINDINGS.md` holds the measured facts; this file is the ledger of
what was been tried, what failed, and what to try next.

---

## 1. Confirmed (and how)

| # | Fact | How it was verified |
|---|---|---|
| F1 | The payload, after the container's LZ77 stage, is not raw pixels. It is `[optional 1024-byte palette][u16 table][data]`. | `src/bin/oddfull` — the bytes after the palette form a non-decreasing u16 ramp terminated by a value equal to the data-region size. |
| F2 | The palette prefix is present exactly when the container's `kind` byte is 1. | Matches the loader: `FUN_004a8344` skips `0x400` bytes when kind is 1. 102 of the 105 odd resources have kind 1; the 3 `MITDrawer1Open0*` ones have kind 0. |
| F3 | `entries[0]` (the first u16 of the table) equals the table's own byte size, `2 * N`. | #749: `entries[0] = 82`, N = 41. #6221: `entries[0] = 178`, N = 89. |
| F4 | `N = 2 * height + 1` using the container's dimension table. | #749 is 16x20 → 41. #6221 is 44x44 → 89. #6223 and #6229 are 60x60 → 121. |
| F5 | Treating the entries as consecutive slice boundaries, **slices `[0..height-1]` are the rows and every row's bytes sum to exactly the image width.** | `src/bin/oddcol`, all 105 resources: **4475 of 4476 rows**. E.g. #6221 row 8 = `[0x10,0x0d,0x0f]` = 16+13+15 = 44. |
| F6 | Slices `[height..2*height]` are a second set of `height` slices whose *lengths* follow a bell curve (small at the top and bottom, maximal in the middle) and whose *values* are small numbers. | `src/bin/oddslices`. For #749 these lengths are all multiples of 4; for #6221 they are not. |
| F8 | For kind-0 resources the second section is made of **4-byte records**, and within a slice the first byte **decreases monotonically** while bytes 1–3 **increase**, i.e. each record looks like a step along a gradient. | `src/bin/oddslices /RawAssets/HD.MMP 750`: slice 50 is `(fe,00,00,00) (ef,01,01,01) (e9,03,01,03) (e0,06,03,06) … (00,52,20,4b)`. |
| F9 | For #750 (67x41, kind 0) the second section's per-slice record counts are `1,1,3,4,6,8,10,12,16,23,31,59,63,65,66,64,63,62,61,60,59,58,56,55,53,52,51,49,48,46,44,42,40,38,35,32,29,25,21,17,12,6` — 42 slices, 1546 records. | `src/bin/oddslices`. |
| F10 | The single F5 failure is identified: `#750 MITDrawer1Open033.PICT` row 40, bytes `[3a,06,03,c0]` = 58+6+3 = 67 ✓ **plus one trailing `0xc0`**. | `src/bin/oddscan`. |
| F11 | Section 1's row bytes are pairs/triples whose values move in opposite directions — e.g. #750 row 0 `[42,01]` = 66+1, row 1 `[40,03]` = 64+3, row 2 `[3f,04]` = 63+4 — a diagonal edge, consistent with a "drawer opening". | `src/bin/oddslices`. |
| F12 | **Game 2 has the same format.** `app.MMP` has 1,162 odd resources, 1,177 of which carry a 2h+1 table; **94,486 of 95,163 rows sum to the width** (99.3%). `HD.MMP` has 19 odd resources and **989 of 989 rows** sum to the width. | `src/bin/oddscan /tmp/sequel/files/app.MMP` and `HD.MMP`. |
| F13 | Every "row sum" failure is the **same signature: the row's run lengths sum to the width, plus trailing bytes**, and it is always at or near the last row. `#112 ITXTcarrot02` row 149 = `[60,06,22]` (=136) + `[94,01,00]`; `#113 ITXTcarrot03` row 124 = `[5b,08,20]` (=131) + `[10]`; game-3 `#750` row 40 = `[3a,06,03]` (=67) + `[c0]`. | `src/bin/oddscan`. 677 such rows in game-2 `app.MMP`. |
| F14 | For kind-0 resources the second section's 4-byte records have a **sparse first byte** and three payload bytes: `(a, b, c, d)` where `a` is mostly small/zero with occasional large values, and `b, c, d` carry the data. Example run from game-2 `#112` slice 162: `(f9,00,00,00) (40,1e,13,04) (00,44,29,09) (00,47,2a,09) … (0a,28,17,03) (85,06,03,00) (f5,00,00,00)`. Here `b` peaks at 0x79, `c` at 0x3d, `d` at 0x15 — all below the 136-pixel width. | `src/bin/oddslices /tmp/sequel/files/app.MMP 112`. |
| F15 | Section 1's rows are strikingly uniform in run count — game-2 `#112` has **all 150 rows as exactly 3 runs**; game-3 `#6221` has rows 8–41 as exactly 3 runs while section 2's slice lengths span 13–35 bytes. So the two sections do **not** have matching per-row counts. | `src/bin/oddslices` on both containers. |
| F16 | **Section 2's 4-byte records are `(T, R, G, B)` colours forming a smooth gradient.** In game-2 `#112` slice 162 the `(R,G,B)` triples rise monotonically from `(30,19,4)` to `(194,121,21)` and fall back to `(0,0,0)` — a glow around the carrot. `T` is 0 (opaque) for almost every record, non-zero only at a few points. | `src/bin/oddslices /tmp/sequel/files/app.MMP 112`. |
| F17 | Section 2's record count is far larger than section 1's run count, and far smaller than the pixel count. Game-2 `#112`: **486 run-length bytes over 150 rows, 12,925 records, 20,400 pixels**. Game-2 `#113`: 438 runs, 10,160 records, 16,375 pixels. Game-3 `#6221`: 118 runs, 250 records, 1,936 pixels. | `src/bin/oddtot`. |
| F18 | Section 2 is **not** a flat pixel RLE: reading it as `(count,R,G,B)` or `(R,G,B,count)` or `(count,index)` pairs gives count sums of 1.19 M / 90 k / 1.82 M against 20,400 pixels for `#112` — nowhere close. | `src/bin/oddtot`. |

**F7 — the open question.** Slices `[0..h-1]` give every row's run lengths. Where the colour for each run comes from is unknown.

---

## 2. Disproved (and how — do not retry these)

| # | Hypothesis | Disproof |
|---|---|---|
| D1 | PackBits, `(count,byte)`, `(count,3-byte)`, `(count,4-byte)` RLE, with a zero count meaning 1 or 256. | `src/bin/oddparse`: **0 hits on all 105**. `/workspace/oddparse_HD.txt` is the matrix. |
| D2 | A second LZ77 stage. | `src/lz77.rs` has no format dispatch and already reproduces the container's recorded decompressed size byte-exactly for every resource. |
| D3 | The table splits into two halves at index `h`, with the second half holding one colour byte per run (`ncols == nruns`). | `src/bin/oddcol` rule A: **25 / 4476 rows**. First failure `#749 r0 nruns=2 ncols=4`. |
| D4 | The table splits at `2i` (interleaved: `entries[2i]`, `entries[2i+1]`, `entries[2i+2]`), one colour byte per run. | `src/bin/odddec` layout B: #6221 21/44, #6223 29/60, #6229 28/60. Fails from the point where the entries stop being uniformly spaced. |
| D5 | Second half read as `(count,index)` or `(index,count)` pairs whose counts sum to the row's run count. | `src/bin/oddcol` rules B–E: **19–28 / 4476 rows**. |
| D6 | Second-half slices are themselves run-length rows (sum of all / even / odd bytes equals the width). | `src/bin/oddcol` rules F, G, H: **3–5 / 4476 rows**. |
| D7 | `ncols == 2*nruns` or `nruns+1`. | Rules I, J: **14 / 4476** and **19 / 4476**. |
| D8 | Shifting the second half by ±1 row. | Rules K, L: **130 / 4476** and **134 / 4476** — no better than chance. |
| D9 | The sub-format is selected by the flags byte's bits 2–3 via `FUN_004c3c98`. | `procedure_callers` on `FUN_004c3c98` returns **no callers**. |
| D10 | The `attr` byte (object offset `0x278`) is read by the picture code. | No function in `0x4a8000-0x4a9000` references `0x278`. |

---

## 3. Open hypotheses, ranked

### H1 — the second section is a **per-row palette**, and runs index into it
For kind 0 (#749) the second section's slices are whole numbers of **4-byte
records** (F6), which look like `(T,R,G,B)` entries: `fd 00 00 00`, `ea 01 00 01`,
`d4 03 03 03`. The first byte decreases while bytes 1–3 increase — a ramp.

If each row carries its own small palette, then a row's runs (from F5) index
into that row's palette, and rows with a single full-width run need no palette
at all — which would explain why slices `[44..51]` of #6221 are empty for its
eight single-run rows.

*Decisive test:* for kind-0 resources only, parse the second section as 4-byte
records and check that the number of records per row is ≥ the number of distinct
run values, and that all run values index inside the row's own record count.

### H2 — the runs alternate between **foreground and one row-level colour**
F5 gives the run lengths but not their colours. A row's data might be
`[fg_len, bg_len, fg_len, ...]` with the foreground colour stored once per row
(e.g. as the single record in the kind-0 second section) and the background
implicitly transparent.

*Decisive test:* for kind-0 rows with exactly one 4-byte record, check that the
record is a plausible foreground colour and that the alternating decode yields a
coherent shape.

### H3 — the second section is indexed by **cumulative run count**, not by row
The second section's slice lengths form a bell curve (F6). A cumulative index
would make the second section a single shared array that rows walk through,
rather than a per-row array.

*Decisive test:* build the cumulative run count at each row boundary, then check
whether the second section's slice boundaries are proportional to it.

### H4 — the entries are **(offset, count) pairs**, not consecutive boundaries
F3 and F5 were both derived from reading the entries as consecutive boundaries,
which is self-consistent, but `(offset, count)` pairs also fit a monotonic u16
array.

*Decisive test:* check whether `entries[2i+1] - entries[2i]` is constant per row
(that would be a count) and whether `entries[2i+2] - entries[2i+1]` is the gap.

### H5 — the image is **column-major**
Nothing has yet established that slices `[0..h-1]` are rows rather than columns.
Their bytes sum to `width`; if they were columns they would sum to `height`.

*Decisive test:* for a resource where width ≠ height, check whether the first
`width` or the first `height` slices sum to the matching dimension. Currently
only checked against `width`.

### H6 — two blended layers
The payload may encode two layers (e.g. coverage + colour) that the game blends
at draw time, rather than one image with an attached colour table.

*Decisive test:* render both candidate layers and check that one is a plausible
alpha mask for the other.

### H7 — the colours come from a **shared palette bank**, not the payload
`FUN_004c3c35` maps a kind byte `k >= 2` to palette-bank entry `k-2`. The
container records report the kind byte is only ever 0 or 1, but that reading has
not been re-verified for the odd resources specifically.

*Decisive test:* print the raw kind byte for all 105 and confirm none is ≥ 2.
**UPDATE:** `src/bin/oddsplit --types all` shows kind is 0 for exactly 3
resources and 1 for 102. So the palette-bank path is not used. For the 3 kind-0
resources the palette must therefore come from inside the payload — which points
straight at H1.

### H8 — each row's data is **(run length, colour index) pairs**, with the
last run implicitly filling the row
F11 shows rows are 2–4 bytes that sum to the width. A 2-byte row could be one
`(length, index)` pair whose run is followed by an implicit background fill to
the row's end, and a 3-byte row one pair plus a trailing bare length.

*Decisive test:* for every row, check that the byte at each odd position is a
valid palette index (small) while the bytes at even positions are the run
lengths — and that the trailing odd byte out is the only orphan.

### H9 — the second section's 4-byte records are **(T, R, G, B) gradient steps**
F8 shows byte 0 falling and bytes 1–3 rising inside a slice. For kind-0
resources with no palette prefix, this is the only place a palette could live,
and the gradient shape matches a shadow/anti-alias ramp.

*Decisive test:* for a kind-0 resource, build the row palettes from these
records and check that the run values in section 1 index inside the row's own
record count.

### H10 — the two sections are **(lengths) and (colours) split across two
parallel arrays indexed by the same row**, but with the colour array being the
4-byte-record array for kind-0 and the index array for kind-1
This is H1 and H9 unified: section 1 always holds run lengths, and section 2
holds one colour per run, encoded either as a raw index (kind 1, where a palette
prefix exists) or as a 4-byte RGBA record (kind 0, where it must double as the
palette).

*Decisive test:* for kind-1 resources, check whether the number of colour bytes
equals the number of runs **when the colour slice is read as 4-byte records too**
— this was never tested; every earlier test assumed one byte per run.

### H11 — the trailing `0xc0` on the last row is a **row terminator**
F10 shows the final row of #750 carries one extra byte beyond the width sum. If
that is a terminator, other resources may carry terminators too, and the row
loop must stop on it rather than on the byte sum.

*Decisive test:* scan all 105 resources for rows whose byte sum exceeds the
width and inspect the excess bytes.

### H12 — the second section is indexed by **cumulative run count** rather than
by row, and the bell curve is the distribution of run counts
F9's record counts rise to a peak near the middle rows and fall away, which is
what a cumulative-index walk would produce if the middle rows have the most
runs — but F5/F11 show section 1's run counts are nearly *constant* per row, so
this does not obviously hold. Low priority.

*Decisive test:* compare the cumulative run count at each row boundary with the
second section's slice lengths.

### H13 — the last row carries a **terminator / overlap with section 2**
F13 shows the final row always has extra bytes beyond the width sum. Two
readings: (a) a terminator the row loop must stop on, or (b) the table's
`entries[h]` boundary is drawn a few bytes into section 2, meaning the two
sections **overlap or abut without a separator**.

*Decisive test:* for the three known cases, check whether removing the trailing
bytes makes the following section-2 slice start on a 4-byte record boundary.
For `#112`: row 149 is `[60,06,22]` + `[94,01,00]` and slice 150 is
`[fd,00,00,00, fe,00,00,00]`. If `entries[150]` were 3 lower, slice 150 would be
`[94,01,00,fd, 00,00,00,fe, 00,00,00,…]` — which does *not* align to 4. So
(b) is unlikely and (a) — a terminator — is the better bet.

### H14 — the 4-byte records are **(count, x, y, z) or (count, index…)** with
`count` meaning something other than a pixel run length
F14 shows the first byte is sparse and the other three carry the data. A
`(count, R, G, B)` reading gives run lengths of hundreds inside a 136-pixel row,
which is impossible — so either `count` is a *delta*, a *flag*, or the record is
not a colour at all.

*Decisive test:* for a kind-0 resource, accumulate `b, c, d` as running totals
and see whether they track a column position or a palette index; and check
whether the sparse `a` values are the only ones that ever exceed the colour
range.

### H15 — the two sections are **(row run lengths) and (per-run colour records)**
but with the colour records being *shared* across rows via a global cursor
F15 rules out per-row matching counts. A global cursor — section 2 is one flat
array of records that the row walker consumes in order — would explain why the
per-slice lengths follow a bell curve (they reflect how many records each row
happens to need) while section 1's run counts stay flat.

*Decisive test:* walk section 1's rows in order, consuming one section-2 record
per run, and check that the record counts per slice exactly cover the run
counts — i.e. whether `sum(records in slice i) == runs in row i` for the
*concatenated* interpretation, or whether the total record count equals the
total run count. **RESULT: disproved.** Game-2 `#112` has 486 runs and 12,925
records; `#113` has 438 runs and 10,160 records; game-3 `#6221` has 118 runs and
250 records. There is no one-record-per-run relationship at any granularity.

### H16 — section 2 is a **colour gradient table consulted by the runs' values**
F16 shows section 2 holds a smooth `(T,R,G,B)` ramp per row. Section 1's run
*lengths* are known; if the run's **value is not a length but a colour index**,
then a 3-byte row would be three colour indices and the "sums to the width"
property would be a coincidence of the gradient — which it cannot be, given it
holds for 94,486 of 95,163 rows. So the values in section 1 really are lengths,
and section 2 must be reached by some other index.

*Decisive test:* for a row with `n` runs, check whether the row's slice in
section 2 contains exactly `n` records **when the records are read as 4-byte
units starting at a per-row offset that is itself derived from the table** — i.e.
whether the table's second-half entries are byte offsets into a record stream
rather than slice boundaries.

### H17 — the table's second half addresses a **record stream**, and section 1's
runs consume it with their own cursor
F17's numbers (486 runs vs 12,925 records for `#112`) make a one-to-one mapping
impossible, but a *fan-out* is possible: each run might consume several records
(e.g. one per distinct colour inside the run, or a gradient segment). This would
also explain why section 2's slice lengths follow a bell curve — the middle rows
have the longest runs and therefore the most records.

*Decisive test:* for each row, divide the row's record count by its run count and
check whether the quotient is stable or proportional to the run lengths. For
`#112` row 147: 3 runs and 14 records; row 148: 3 runs and 11 records — the
quotient is not stable, so test whether it is proportional to the run lengths
instead.

### H18 — the two halves are **(run lengths) and (colour stops)**, and the image
is drawn by interpolating between colour stops
If section 1 gives `n` run lengths summing to the width and section 2 gives `n+1`
colour stops for that row, the row is painted by walking the runs and stepping
through the gradient. The record counts would then exceed the run counts, which
matches F17.

*Decisive test:* for each row, check whether `records_in_slice == runs_in_row + 1`
under some alignment, or whether the first and last records of a slice are the
row's start and end colours.

---

## 4. Known blind spots in the work so far

These are places where my own test could have been circular. Check these before
trusting any conclusion above.

1. **The table search is circular for F3/F4.** `src/bin/oddcol`'s `locate()`
   *requires* `n == 2*h + 1`, so "105 of 105 resources have a 2h+1 table" is
   guaranteed by construction. The independent evidence is only that
   `entries[0] == 2*n` holds in every case found. Re-run the search accepting any
   even `n` and see what other table sizes exist.
2. **F5 was only ever tested against `width`.** No test has compared the row
   sums against `height`. See H5.
3. **The single failing row in F5 (1 of 4476) has not been identified.** Find it;
   it may be the resource whose structure is genuinely different.
4. **BI_RLE8 was only rejected because the payload does not end in `00 01`.**
   A full decode was never attempted. Retire or confirm D11 properly.
5. **Game 2 has not been analysed at all** in this session. The ~1,181 odd
   resources there may have a simpler structure (their payloads were described
   as beginning with row-offset tables such as `00 00 49 01 52 01`).
6. **Only 3 of the 105 resources have kind 0**, and the kind-1 case (102 of 105)
   is the one that must be solved for the bulk of the corpus.

---

## SOLUTION (solved 2026-10-28)

The odd picture payload is a **two-section run-length encoding**:

```text
[kind == 1]  1024 bytes    palette (skipped by the loader when kind is 1)
u16[2h+1]                  big-endian offset table
    entries[0]             == 2*(2h+1), the table's own byte size
    entries[0..=h]         byte offset where row i's run-length slice starts
    entries[h..=2h]        byte offset where row i's colour slice starts
bytes                      run lengths, then colours, concatenated
```

`h` is the container's recorded height, so the table always has exactly
`2*height + 1` entries and **`entries[0]` both identifies the table and gives
the offset at which the data begins**.

### The run-length section

Row `i`'s slice `[entries[i] .. entries[i+1])` is a list of run lengths whose
**sum is the image width**. The runs **alternate background / foreground,
starting with background**, so the runs at odd indices (the 2nd, 4th, …) are
the foreground and the runs at even indices (the 1st, 3rd, …) are transparent
background.

### The colour section

Row `i`'s slice `[entries[h+i] .. entries[h+i+1])` holds **exactly one colour
per foreground pixel**, consumed left to right as the row is painted:

* `kind == 1` — **one byte per foreground pixel**: a palette index into the
  1024-byte prefix.
* `kind == 0` — **four bytes per foreground pixel**: `(T, R, G, B)` in the
  game's usual transparency-first order.

This is the reading that finally made everything line up: for game-2
`ITXTcarrot02` the `(R,G,B)` triples rise from `(30,19,4)` to `(194,121,21)`
and fall back to `(0,0,0)` with `T=0` throughout, because they are a per-pixel
gradient across the carrot's glow, not a palette.

### Verification

`src/bin/oddfix` scores "sum of foreground run lengths == number of colour
records in the paired slice" per row:

| resource | container | dims | kind | rows matching |
|---|---|---|---|---|
| `ISselectBoxFlipSelect.pct` | `HD.MMP` | 44x44 | 1 | **44 / 44** |
| `ISselectBoxROLLON.PCT` | `HD.MMP` | 60x60 | 1 | **60 / 60** |
| `ITXTcarrot02` | game-2 `app.MMP` | 136x150 | 0 | **149 / 150** |
| `MITDrawer1Open032.PICT` | `HD.MMP` | 16x20 | 0 | **20 / 20** |

The single non-matching row in each kind-0 case is the **last row**, which
carries trailing bytes beyond the width sum (F13). Those bytes belong to the
end of the payload and must be ignored.

### Why every earlier attempt failed

Every previous test assumed the second section held **one colour per run**.
It does not — it holds **one colour per foreground pixel**. Because the
foreground runs are typically much longer than one pixel, the colour slice is
much longer than the run list, which is exactly the bell-curve length profile
that made the two sections look unrelated.
