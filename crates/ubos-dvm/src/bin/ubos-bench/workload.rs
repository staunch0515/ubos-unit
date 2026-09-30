//! WL-GENERIC-10M generator (UBS-VER-03): 200 classes of 5–40 fields in 3 inheritance
//! levels, objects spread over the classes, and 10% of the objects changed each month.
//!
//! Everything is a pure function of `(seed, object index, month)`, so a dataset is
//! reproduced from its specification and any object can be recomputed on its own.

use serde_json::{Map, Value, json};

/// Number of classes.
pub const CLASSES: u64 = 200;
/// Root classes (level 1) and level-2 classes; the rest are level 3.
const ROOTS: u64 = 20;
const LEVEL2: u64 = 60;
/// First valid date; month `m` starts `m` months later.
const START_YEAR: u64 = 2024;
/// Field types in rotation.
const TYPES: &[&str] = &["text", "integer", "decimal", "date", "boolean"];

/// Dataset specification.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    /// Objects created in the initial load.
    pub objects: u64,
    /// Months of history after the initial load.
    pub months: u64,
    /// Generator seed (fixed per BPA release).
    pub seed: u64,
}

impl Spec {
    /// The specification as JSON for reports.
    pub fn to_json(self) -> Value {
        json!({"workload": "WL-GENERIC", "objects": self.objects, "months": self.months, "seed": self.seed,
               "classes": CLASSES, "monthly_change_percent": 10})
    }
}

/// SplitMix64 finaliser over a combination of inputs.
pub fn mix(a: u64, b: u64, c: u64) -> u64 {
    let mut z = a ^ b.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ c.wrapping_mul(0xD1B5_4A32_D192_ED03);
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn class_name(c: u64) -> String {
    format!("Generic{c:03}")
}

/// Own field count of a class: roots 5–15, levels 2 and 3 up to 12 and 13 more, so every
/// class has 5–40 effective fields.
fn own_fields(seed: u64, c: u64) -> u64 {
    let r = mix(seed, 1, c);
    if c < ROOTS {
        5 + r % 11
    } else if c < ROOTS + LEVEL2 {
        r % 13
    } else {
        r % 14
    }
}

fn parent(seed: u64, c: u64) -> Option<u64> {
    let r = mix(seed, 2, c);
    if c < ROOTS {
        None
    } else if c < ROOTS + LEVEL2 {
        Some(r % ROOTS)
    } else {
        Some(ROOTS + r % LEVEL2)
    }
}

/// Class declarations for `load_genesis`.
pub fn classes(seed: u64) -> Value {
    let mut out = Vec::new();
    for c in 0..CLASSES {
        let fields: Vec<Value> = (0..own_fields(seed, c))
            .map(|i| {
                let t = TYPES[(mix(seed, 3, c * 64 + i) % TYPES.len() as u64) as usize];
                let name = format!("f{c}_{i}");
                json!({"name": name, "type": t, "label": name, "description": name})
            })
            .collect();
        let parent = parent(seed, c).map_or("DataObject".to_string(), class_name);
        out.push(
            json!({"name": class_name(c), "namespace": "bench.generic", "kind": "definition",
            "parent": parent, "label": class_name(c), "description": "WL-GENERIC class",
            "schema_version": 1, "fields": fields}),
        );
    }
    Value::Array(out)
}

/// Object identifier of object `i` (UUID version 7 layout).
pub fn object_id(seed: u64, i: u64) -> String {
    let r = mix(seed, 4, i);
    format!(
        "0190{:04x}-{:04x}-7{:03x}-8{:03x}-{:012x}",
        (i >> 32) & 0xffff,
        (i >> 16) & 0xffff,
        r & 0xfff,
        (r >> 12) & 0xfff,
        ((i & 0xffff) << 32) | ((r >> 24) & 0xffff_ffff)
    )
}

/// Class of object `i`.
pub fn class_of(seed: u64, i: u64) -> String {
    class_name(mix(seed, 5, i) % CLASSES)
}

/// Slug of object `i`.
pub fn slug(i: u64) -> String {
    format!("g-{i}")
}

/// True when object `i` changes in month `m` (1-based), for about 10% of objects.
pub fn changes_in(seed: u64, i: u64, m: u64) -> bool {
    mix(seed, 6 + m, i).is_multiple_of(10)
}

/// First day of month `m` after the start (`m = 0` is the initial load).
pub fn month_date(m: u64) -> String {
    let months = (START_YEAR * 12) + m;
    format!("{:04}-{:02}-01", months / 12, months % 12 + 1)
}

/// A field value of the given type.
fn value(t: &str, r: u64) -> Value {
    match t {
        "text" => Value::String(format!("t{:x}", r % 0xffff_ffff_ffff)),
        "integer" => json!((r % 1_000_000) as i64),
        "decimal" => Value::String(format!("{}.{:02}", r % 100_000, (r >> 20) % 100)),
        "date" => Value::String(format!(
            "20{:02}-{:02}-{:02}",
            10 + r % 15,
            1 + (r >> 8) % 12,
            1 + (r >> 16) % 28
        )),
        _ => Value::Bool(r & 1 == 1),
    }
}

/// Body of object `i` at month `m` given its effective fields `(name, type)`: every field
/// at creation; one to three changed fields in a later month.
pub fn body(
    seed: u64,
    i: u64,
    m: u64,
    fields: &[(String, String)],
    prev: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    match prev {
        None => fields
            .iter()
            .enumerate()
            .map(|(k, (n, t))| (n.clone(), value(t, mix(seed, i, 1000 + k as u64))))
            .collect(),
        Some(p) => {
            let mut b = p.clone();
            let n = 1 + mix(seed, i, 2000 + m) % 3;
            for j in 0..n {
                let k = (mix(seed, i, 3000 + m * 8 + j) % fields.len() as u64) as usize;
                let (name, t) = &fields[k];
                b.insert(name.clone(), value(t, mix(seed, i, 4000 + m * 8 + j)));
            }
            b
        }
    }
}
