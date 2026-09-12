use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use marspeak::{Level, Marspeak};

const SENTENCE: &str = "今天天气真好，我们一起去看看外面的世界";

/// 重复句子拼出长文本，让基准能覆盖「每个字都查表」的稳定负载。
fn corpus(repeat: usize) -> String {
    SENTENCE.repeat(repeat)
}

fn converter(level: Level, intensity: f32) -> Marspeak {
    Marspeak::builder()
        .level(level)
        .intensity(intensity)
        .seed(42)
        .build()
        .expect("参数合法")
}

const LEVELS: [Level; 3] = [Level::Light, Level::Medium, Level::Heavy];

fn encode_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode");
    let cases = [("short", corpus(1)), ("long", corpus(200))];

    for level in LEVELS {
        let mp = converter(level, 0.7);
        for (label, text) in &cases {
            group.throughput(Throughput::Bytes(text.len() as u64));
            group.bench_with_input(BenchmarkId::new(format!("{level:?}"), label), text, |b, t| {
                b.iter(|| mp.encode(black_box(t)));
            });
        }
    }
    group.finish();
}

fn encode_intensity_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode/intensity");
    let text = corpus(200);

    for intensity in [0.0f32, 0.3, 0.6, 1.0] {
        let mp = converter(Level::Heavy, intensity);
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(intensity),
            &text,
            |b, t| {
                b.iter(|| mp.encode(black_box(t)));
            },
        );
    }
    group.finish();
}

fn decode_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode");
    let cases = [("short", corpus(1)), ("long", corpus(200))];

    for level in LEVELS {
        let mp = converter(level, 0.7);
        for (label, text) in &cases {
            let encoded = mp.encode(text);
            group.throughput(Throughput::Bytes(encoded.len() as u64));
            group.bench_with_input(
                BenchmarkId::new(format!("{level:?}"), label),
                &encoded,
                |b, t| {
                    b.iter(|| mp.decode(black_box(t)).unwrap());
                },
            );
        }
    }
    group.finish();
}

fn round_trip_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("round_trip");
    let mp = converter(Level::Medium, 0.7);
    let text = corpus(200);

    group.throughput(Throughput::Bytes(text.len() as u64));
    group.bench_function("medium/long", |b| {
        b.iter(|| {
            let encoded = mp.encode(black_box(&text));
            mp.decode(black_box(&encoded)).unwrap()
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    encode_benchmark,
    encode_intensity_benchmark,
    decode_benchmark,
    round_trip_benchmark
);
criterion_main!(benches);
