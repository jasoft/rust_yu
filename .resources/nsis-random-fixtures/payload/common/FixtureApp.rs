use std::env;

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| "--interactive".to_string());
    println!("Rust Yu deep-analysis fixture: {mode}");
}
