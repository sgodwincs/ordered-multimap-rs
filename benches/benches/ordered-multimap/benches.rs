#![allow(missing_docs)]
#![allow(unused_results)]

use criterion::criterion_main;

mod insert_with_capacity;
mod insert_without_capacity;
mod iter;
mod remove;

criterion_main!(
  insert_with_capacity::benches,
  insert_without_capacity::benches,
  iter::benches,
  remove::benches
);
