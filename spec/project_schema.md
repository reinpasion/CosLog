# CosLog project file format (`.json`)

Normative for both editions: CosLog X (`coslog-x`, Rust) and CosLog Y (`coslog-y`, browser).
Part definitions (pins, parameters, defaults, limits, symbols) live in `parts.json`; this
file defines only what is saved and how it is read. Files written by CircuitLite v1 load unchanged.

## 1. Top level

```json
{
  "app": "CosLog",
  "ver": 1,
  "seed": 24301,
  "c": [ { "t": "dc", "x": 0, "y": 0, "r": 0, "v": "9", "n": "V1" } ],
  "w": [ { "x1": 0, "y1": 40, "x2": 200, "y2": 40 } ],
  "view": { "z": 1, "x": 0, "y": 0 }
}
```

| Key | Type | Required | Rule |
|---|---|---|---|
| `app` | string | no | Written as `"CosLog"`. Readers accept `"CosLog"`, `"CircuitLite"`, or a missing value. Any other value loads with a warning. |
| `ver` | integer | no | Written as `1`. A higher version loads with a warning ("saved by a newer CosLog"). |
| `seed` | integer or string | no | Tolerance seed. Default `0x5EED` (24301). See section 4. |
| `c` | array | yes | Components. Order is significant (section 4). |
| `w` | array | yes | Wires. |
| `view` | object | no | Pan and zoom. Optional; minimal and generated circuits omit it. |

Loading fails only when the text is not JSON, or `c` or `w` is not an array. Everything else degrades with a warning.

## 2. Components (`c[]`)

Saved keys, exactly 16: `t x y r m v f on n no vo u p q s k`.

| Key | Type on write | Accepted on read | Meaning |
|---|---|---|---|
| `t` | string | string | Part code from `parts.json`. |
| `x`, `y` | integer | number (rounded to nearest integer) | Origin on the 20-unit grid. Pin coordinates are origin plus transformed pin offset, so they are integers too. Netlist nodes are matched by exact integer coordinates. |
| `r` | 0..3 | integer (taken modulo 4) | Quarter turns clockwise. The oscilloscope (`os`) is always 0. |
| `m` | 0 or 1 | 0, 1, true, false | Mirror about the Y axis. |
| `on` | 0 or 1 | 0, 1, true, false | Switch, button, SPDT, relay state. |
| `v f p q s k` | string | string or number (numbers are converted to strings) | Parameter slots. Their meaning, unit and default per part are in `parts.json` `params[]`. Values are engineering text such as `"4.7k"`, `"100n"`, `"2meg"`. |
| `n` | string | string | Designator such as `R1`. An empty string means no name (section 5). |
| `no`, `vo` | `[dx, dy]` | two-number array | Offsets of the name label and the value label. |
| `u` | string | string | Unit text override for the value label. |

Transform: mirror X first (`x -> -x`), then rotate `r` times 90 degrees clockwise. Text is never mirrored. Pin labels follow rotation.

Rules:

- **Whitelist.** Only the 16 keys above are read and written. Unknown keys are ignored on load and dropped on save. They are never copied onto the component. All simulation state (`vp ip is av sq gl cnt st bl w ang ta tb ...`) is runtime-only and never saved.
- **Defaults.** A missing key takes the default from `parts.json` (`defaults` for `v` and `f`, `params[].default` otherwise). A missing `m`, `on` or `r` is 0. Writers may omit `m` and `on` when 0, and may omit keys with no value.
- **Unknown part codes.** A component whose `t` is not in `parts.json` is skipped, not loaded, and the loader shows one warning banner such as `Skipped 2 unknown part(s): xyz, abc (made with a newer or different CosLog)`. Wires are kept. The rest of the circuit loads. Skipped components are not written back on save, and the count of skipped parts is reported once. Implementation note for coslog-x: parse each element of `c` as raw JSON first, then convert it, so one bad element cannot fail the whole file.
- **Order.** `c[]` order is part of the format (section 4). The tolerance index is the position in the file as read, counting skipped parts, so opening a file never changes the tolerance of the parts that load. Saving drops skipped parts, which shifts the index of every later part.

## 3. Wires (`w[]`)

```json
{ "x1": 0, "y1": 40, "x2": 200, "y2": 40 }
```

- Four integer grid coordinates. No ids and no node numbers; connectivity is derived from coordinates.
- A wire end touches a pin or another wire when the point is the same, or lies on the other wire's segment.
- Extra keys are ignored.

## 4. Tolerance and seed

Tolerance applies only to `r`, `l` and `c`, and only when parameter `p` parses to a number greater than 0 (percent). For the component at index `i` in `c[]` (0-based):

```
value = value * (1 + p/100 * rnd(seed, i))
```

`rnd` is splitmix64, in unsigned 64-bit wrapping arithmetic:

```
z = seed + (i + 1) * 0x9E3779B97F4A7C15
z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
z = (z ^ (z >> 27)) * 0x94D049BB133111EB
z = z ^ (z >> 31)
rnd = (z >> 11) / 2^52 - 1            // range [-1, 1)
```

- **Seed.** Default `0x5EED` = 24301. Write `seed` only when it differs from the default. Read it as an integer or a string (`"24301"` or `"0x5EED"`). Seeds above 2^53 must be written as strings. An unreadable seed falls back to the default with a warning.
- **JS.** Use `BigInt` (or two 32-bit halves); it runs only when the netlist is built. Use `Number(z >> 11n) / 2**52 - 1`.
- **Consequence of index keying.** Adding or deleting a component shifts the index of every later component, which changes their tolerance draws. Moving or editing a component does not. Circuits with 0% tolerance are unaffected.
- **Compatibility.** CircuitLite files have no `seed` and use the default. JS saves made before this change carried no per-part random value, so existing tolerance circuits get new, now reproducible, values.

Reference vectors (computed from the formula above with 64-bit integer arithmetic; both editions must reproduce them to 1e-15):

| seed | i | rnd |
|---|---|---|
| 24301 (`0x5EED`) | 0 | -0.92230253060562961 |
| 24301 | 1 | -0.33439778252114039 |
| 24301 | 2 | -0.27063628724372357 |
| 24301 | 7 | -0.85517423226382672 |
| 24301 | 100 | 0.20850133807570881 |
| 1 | 0 | 0.13312315034456179 |
| 1 | 3 | -0.11128156588845584 |

## 5. Names

A component's display name is `n` when `n` is a non-empty string, otherwise the part code `t`. An empty string `""` counts as missing everywhere (labels, diagnostics, auto-numbering). Writers omit `n` instead of writing `""`. Auto-numbering assigns the first unused `prefix + number` (`R1`, `R2`, ...), using the prefix in `parts.json`; parts with no prefix (ground) get no name.

## 6. View

`view` is optional editor data: `{ "z": zoom, "x": panX, "y": panY }`. It is used only when `z` is a positive number. It never affects simulation. Editions may ignore it.

## 7. Parameter text

Numbers in `v f p q s` use engineering notation: an optional sign, digits with an optional decimal point and exponent, then an optional suffix `p n u µ m k M meg g G` (case-insensitive except that `M` alone and `meg` mean 1e6). Any other trailing letter (such as `V` or `Hz`) is ignored (multiplier 1). Text that does not parse falls back to the part's documented fallback (for example 1e-3 for most values, 1 for transformers, 50 Hz for frequency). Both editions use the same parser, tested with the golden circuits.

## 8. Written form

- Writers emit keys in this order: `app`, `ver`, `seed` (if not default), `c`, `w`, `view`.
- `c` items emit keys in the order `t x y r m v f on n no vo u p q s k`, skipping absent values.
- Parameter slots are always written as strings.
- Indentation is not significant.

## 9. Compatibility checklist

1. A CircuitLite v1 file loads in both editions with no changes.
2. A file with `"on": true` or `"m": false` loads; the next save writes 0 or 1.
3. A file with a numeric `"v": 1000` loads as `"1000"`.
4. A file with an unknown part loads with a warning, and the other parts and wires intact.
5. The same file with tolerance gives identical node voltages in both editions to the golden-test tolerance (1e-4 relative on node voltages).
