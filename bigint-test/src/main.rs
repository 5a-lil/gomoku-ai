use num_bigint::BigUint;

use primitive_types::U512;

fn func(mut x: i32) {
    x = 2
}

fn main() {
    let mut x = [1; 200];
    let mut t1 = chrono::Utc::now().timestamp_micros();
    for i in 0..361 {
        x[180] = i;
    }

    let t1 = chrono::Utc::now().timestamp_micros() - t1;
    println!("{t1}")
}
