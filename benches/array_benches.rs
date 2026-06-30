use criterion::{black_box, criterion_group, criterion_main, Criterion};

// Placeholder for actual internal modules. For now we just create a dummy bench.
// In reality, this should import array processing functions from rmdadm
// like `rmdadm::array::rebuild` etc.

fn dummy_array_computation(size: usize) -> Vec<u8> {
    let mut data = vec![0u8; size];
    for i in 0..size {
        data[i] = data[i].wrapping_add(1);
    }
    data
}

fn bench_array_processing(c: &mut Criterion) {
    c.bench_function("dummy array processing 1MB", |b| b.iter(|| dummy_array_computation(black_box(1024 * 1024))));
}

criterion_group!(benches, bench_array_processing);
criterion_main!(benches);
