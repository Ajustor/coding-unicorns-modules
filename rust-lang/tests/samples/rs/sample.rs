//! Crate docs.
use std::collections::HashMap;

/// A point.
#[derive(Debug, Clone)]
pub struct Point<T> {
    x: T,
    y: T,
}

impl<T: Copy + Into<f64>> Point<T> {
    pub fn norm(&self) -> f64 {
        let (x, y): (f64, f64) = (self.x.into(), self.y.into());
        (x * x + y * y).sqrt()
    }
}

fn main() {
    let mut seen: HashMap<&str, u32> = HashMap::new();
    for word in "a b a".split(' ') {
        *seen.entry(word).or_insert(0) += 1;
    }
    let raw = r"C:\path";
    println!("{seen:?} {} {}", raw, 0x1F_u8 as char);
    /* block */
    if let Some(n) = seen.get("a") { assert_eq!(*n, 2, "count"); }
}
