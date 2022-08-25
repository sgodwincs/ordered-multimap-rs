use criterion::{criterion_group, Bencher, Criterion};
use multimap::MultiMap;
use ordered_multimap::ListOrderedMultimap;

const ELEMENT_COUNT: usize = 100000;

fn input_iter() -> impl Iterator<Item = (usize, usize)> {
  (0..ELEMENT_COUNT)
    .into_iter()
    .enumerate()
    .flat_map(|(k, v)| [(k, v), (k, v), (k, v), (k, v), (k, v)])
}

fn list_ordered_multimap(b: &mut Bencher<'_>) {
  let mut map = ListOrderedMultimap::new();
  map.extend(input_iter());
  let mut sum = 0;

  b.iter(|| {
    for i in map.iter() {
      sum += i.1;
    }
  });
}

fn multimap(b: &mut Bencher<'_>) {
  let mut map = MultiMap::new();
  map.extend(input_iter());
  let mut sum = 0;

  b.iter(|| {
    for i in map.iter() {
      sum += i.1;
    }
  });
}

fn benchmark(c: &mut Criterion) {
  let mut group = c.benchmark_group("iterate");

  group.bench_function("ListOrderedMultimap", |b| list_ordered_multimap(b));
  group.bench_function("MultiMap", |b| multimap(b));

  group.finish();
}

criterion_group!(benches, benchmark);
