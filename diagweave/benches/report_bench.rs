use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use diagweave::prelude::{Compact, Pretty, Report};
use diagweave::report::{AttachmentValue, CauseCollectOptions};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::hint::black_box;

#[cfg(feature = "json")]
use diagweave::render::Json;
#[cfg(feature = "json")]
use diagweave::render::ReportRenderOptions;

#[derive(Debug, Clone, Copy)]
enum BenchError {
    Root,
}

impl Display for BenchError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Root => write!(f, "bench root error"),
        }
    }
}

impl Error for BenchError {}

fn make_report(
    context_count: usize,
    note_count: usize,
    payload_count: usize,
    source_count: usize,
) -> Report<BenchError> {
    let mut report = Report::new(BenchError::Root)
        .with_error_code("BENCH.ROOT")
        .with_category("benchmark")
        .with_retryable(false);

    for idx in 0..context_count {
        report = report.with_context(format!("ctx_{idx}"), idx as u64);
    }

    for idx in 0..note_count {
        report = report.with_note(format!("note_{idx}"));
    }

    for idx in 0..payload_count {
        report = report.with_payload(
            format!("payload_{idx}"),
            AttachmentValue::Array(vec![
                AttachmentValue::Unsigned(idx as u64),
                AttachmentValue::from("diagweave"),
            ]),
            Some("application/x.bench"),
        );
    }

    for idx in 0..source_count {
        report =
            report.with_diagnostic_source_error(std::io::Error::other(format!("source_{idx}")));
    }

    report
}

fn bench_report_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("report_build");
    for size in [0usize, 4, 16, 64] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::new("contexts", size), &size, |b, &size| {
            b.iter(|| {
                let mut report = Report::new(BenchError::Root);
                for idx in 0..size {
                    report = report.with_context(format!("ctx_{idx}"), idx as u64);
                }
                black_box(report);
            })
        });
    }

    for size in [0usize, 2, 8, 32] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::new("mixed_attachments", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let report = make_report(size, size, size, 0);
                    black_box(report);
                })
            },
        );
    }
    group.finish();
}

fn bench_ir_and_render(c: &mut Criterion) {
    let small = make_report(2, 1, 1, 1);
    let medium = make_report(8, 4, 4, 4);
    let large = make_report(32, 16, 16, 16);

    let mut group = c.benchmark_group("report_transform_and_render");

    for (name, report) in [("small", &small), ("medium", &medium), ("large", &large)] {
        group.bench_function(BenchmarkId::new("to_diagnostic_ir", name), |b| {
            b.iter(|| {
                let ir = report.to_diagnostic_ir();
                black_box(ir.context_count + ir.attachment_count);
            })
        });

        group.bench_function(BenchmarkId::new("render_compact", name), |b| {
            b.iter(|| {
                black_box(report.render(Compact).to_string());
            })
        });

        group.bench_function(BenchmarkId::new("render_pretty", name), |b| {
            b.iter(|| {
                black_box(report.render(Pretty::default()).to_string());
            })
        });

        #[cfg(feature = "json")]
        group.bench_function(BenchmarkId::new("render_json_compact", name), |b| {
            b.iter(|| {
                black_box(report.render(Json::default()).to_string());
            })
        });

        #[cfg(feature = "json")]
        group.bench_function(BenchmarkId::new("render_json_pretty", name), |b| {
            b.iter(|| {
                let options = ReportRenderOptions {
                    json_pretty: true,
                    ..ReportRenderOptions::default()
                };
                black_box(report.render(Json::new(options)).to_string());
            })
        });
    }

    group.finish();
}

fn bench_source_traversal(c: &mut Criterion) {
    let report = make_report(4, 2, 2, 64);
    let mut group = c.benchmark_group("source_traversal");

    for max_depth in [1usize, 4, 16, 64] {
        group.bench_with_input(
            BenchmarkId::new("iter_origin_sources_depth", max_depth),
            &max_depth,
            |b, &max_depth| {
                b.iter(|| {
                    let mut iter = report.iter_origin_sources_ext(CauseCollectOptions {
                        max_depth,
                        detect_cycle: true,
                    });
                    let count = iter.by_ref().count();
                    black_box((count, iter.state().truncated));
                })
            },
        );
    }

    for max_depth in [1usize, 4, 16, 64] {
        group.bench_with_input(
            BenchmarkId::new("iter_diagnostic_sources_depth", max_depth),
            &max_depth,
            |b, &max_depth| {
                b.iter(|| {
                    let mut iter = report.iter_diagnostic_sources_ext(CauseCollectOptions {
                        max_depth,
                        detect_cycle: true,
                    });
                    let count = iter.by_ref().count();
                    black_box((count, iter.state().truncated));
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_report_build,
    bench_ir_and_render,
    bench_source_traversal
);
criterion_main!(benches);
