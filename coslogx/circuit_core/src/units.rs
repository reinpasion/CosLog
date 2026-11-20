//! SI-suffix parsing (`pv`) and engineering formatting (`fm`), matching the JS.

/// Parse like the JS `pv`: leading number, optional `meg` or one-letter SI suffix.
/// Returns NaN when no number is found (same as JS).
pub fn pv(s: &str) -> f64 {
    let b: Vec<char> = s.trim_start().chars().collect();
    let mut i = 0;
    if i < b.len() && (b[i] == '+' || b[i] == '-') { i += 1; }
    let ds = i;
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == '.') { i += 1; }
    if i == ds { return f64::NAN; }
    // optional exponent: e[+-]?digits
    if i < b.len() && (b[i] == 'e' || b[i] == 'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == '+' || b[j] == '-') { j += 1; }
        let k = j;
        while j < b.len() && b[j].is_ascii_digit() { j += 1; }
        if j > k { i = j; }
    }
    let num: String = b[..i].iter().collect();
    let n: f64 = match num.parse() { Ok(v) => v, Err(_) => return f64::NAN };
    while i < b.len() && b[i].is_whitespace() { i += 1; }
    let rest: String = b[i..].iter().collect();
    let low = rest.to_lowercase();
    let mul = if low.starts_with("meg") { 1e6 } else {
        match rest.chars().next() {
            Some('M') => 1e6,
            Some(c) => match c.to_ascii_lowercase() {
                'p' => 1e-12, 'n' => 1e-9, 'u' | 'µ' => 1e-6,
                'm' => 1e-3, 'k' => 1e3, 'g' => 1e9, _ => 1.0 },
            None => 1.0 }
    };
    n * mul
}

/// 3-significant-digit engineering format, e.g. `fm(0.0047,"A") == "4.7 mA"`.
pub fn fm(x: f64, u: &str) -> String {
    let a = x.abs();
    if a < 1e-12 { return format!("0 {u}"); }
    for (k, p) in [(1e9, "G"), (1e6, "M"), (1e3, "k"), (1.0, ""), (1e-3, "m"), (1e-6, "µ"), (1e-9, "n")] {
        if a >= k * 0.9995 {
            let v: f64 = format!("{:.2e}", x / k).parse().unwrap_or(0.0);
            return format!("{v} {p}{u}");
        }
    }
    format!("0 {u}")
}
