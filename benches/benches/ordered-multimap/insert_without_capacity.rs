use criterion::{criterion_group, Bencher, Criterion};
use multimap::MultiMap;
use ordered_multimap::ListOrderedMultimap;

const ELEMENT_COUNT: usize = 100000;

fn input_iter() -> impl Iterator<Item = (usize, usize)> {
  (0..(ELEMENT_COUNT / 5)).into_iter().enumerate()
}

fn list_ordered_multimap(b: &mut Bencher<'_>) {
  b.iter(|| {
    let mut map = ListOrderedMultimap::new();

    for (i, v) in input_iter() {
      map.insert(i, v);
      map.insert(i, v);
      map.insert(i, v);
      map.insert(i, v);
    }
  });
}

fn multimap(b: &mut Bencher<'_>) {
  b.iter(|| {
    let mut map = MultiMap::new();

    for (i, v) in input_iter() {
      map.insert(i, v);
      map.insert(i, v);
      map.insert(i, v);
      map.insert(i, v);
    }
  });
}

fn benchmark(c: &mut Criterion) {
  let mut group = c.benchmark_group("insert without capacity");

  group.bench_function("ListOrderedMultimap", |b| list_ordered_multimap(b));
  group.bench_function("MultiMap", |b| multimap(b));

  group.finish();
}

criterion_group!(benches, benchmark);
