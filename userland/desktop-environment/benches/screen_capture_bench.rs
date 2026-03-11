// SPDX-License-Identifier: GPL-3.0
//! Screen capture benchmarks for the AGNOS desktop environment.
//!
//! Measures PNG/BMP/raw encoding throughput and pixel buffer operations at
//! common display resolutions (720p, 1080p, 4K).

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

use desktop_environment::{
    CaptureFormat, CaptureTarget, Compositor, ScreenCaptureManager,
};
use desktop_environment::renderer::Framebuffer;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolution presets: (name, width, height).
const RESOLUTIONS: &[(&str, u32, u32)] = &[
    ("720p", 1280, 720),
    ("1080p", 1920, 1080),
    ("4K", 3840, 2160),
];

fn make_compositor_and_manager(width: u32, height: u32) -> (Compositor, ScreenCaptureManager) {
    let compositor = Compositor::with_resolution(width, height);
    let manager = ScreenCaptureManager::new();
    (compositor, manager)
}

// ---------------------------------------------------------------------------
// 1. Full-screen capture + encoding at various resolutions and formats
// ---------------------------------------------------------------------------

fn bench_capture_png(c: &mut Criterion) {
    let mut group = c.benchmark_group("screen_capture/png_encode");

    for &(name, w, h) in RESOLUTIONS {
        let (compositor, manager) = make_compositor_and_manager(w, h);
        // Warm-up: ensure the renderer has a valid front buffer
        compositor.render();

        group.bench_with_input(BenchmarkId::new("full_screen", name), &(), |b, _| {
            b.iter(|| {
                black_box(
                    manager
                        .capture(&compositor, CaptureTarget::FullScreen, CaptureFormat::Png, None)
                        .unwrap(),
                );
            });
        });
    }
    group.finish();
}

fn bench_capture_bmp(c: &mut Criterion) {
    let mut group = c.benchmark_group("screen_capture/bmp_encode");

    for &(name, w, h) in RESOLUTIONS {
        let (compositor, manager) = make_compositor_and_manager(w, h);
        compositor.render();

        group.bench_with_input(BenchmarkId::new("full_screen", name), &(), |b, _| {
            b.iter(|| {
                black_box(
                    manager
                        .capture(&compositor, CaptureTarget::FullScreen, CaptureFormat::Bmp, None)
                        .unwrap(),
                );
            });
        });
    }
    group.finish();
}

fn bench_capture_raw(c: &mut Criterion) {
    let mut group = c.benchmark_group("screen_capture/raw_encode");

    for &(name, w, h) in RESOLUTIONS {
        let (compositor, manager) = make_compositor_and_manager(w, h);
        compositor.render();

        group.bench_with_input(BenchmarkId::new("full_screen", name), &(), |b, _| {
            b.iter(|| {
                black_box(
                    manager
                        .capture(
                            &compositor,
                            CaptureTarget::FullScreen,
                            CaptureFormat::RawArgb,
                            None,
                        )
                        .unwrap(),
                );
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 2. Region capture at different sizes (from the 1080p compositor)
// ---------------------------------------------------------------------------

fn bench_capture_region_sizes(c: &mut Criterion) {
    let (compositor, manager) = make_compositor_and_manager(1920, 1080);
    compositor.render();

    let regions: &[(&str, i32, i32, u32, u32)] = &[
        ("256x256", 0, 0, 256, 256),
        ("640x480", 0, 0, 640, 480),
        ("1280x720", 0, 0, 1280, 720),
        ("1920x1080", 0, 0, 1920, 1080),
    ];

    let mut group = c.benchmark_group("screen_capture/region_png");
    for &(name, x, y, w, h) in regions {
        group.bench_with_input(BenchmarkId::new("capture", name), &(), |b, _| {
            b.iter(|| {
                black_box(
                    manager
                        .capture(
                            &compositor,
                            CaptureTarget::Region {
                                x,
                                y,
                                width: w,
                                height: h,
                            },
                            CaptureFormat::Png,
                            None,
                        )
                        .unwrap(),
                );
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 3. Pixel buffer copy / conversion (Framebuffer operations)
// ---------------------------------------------------------------------------

fn bench_framebuffer_copy(c: &mut Criterion) {
    let mut group = c.benchmark_group("screen_capture/pixel_buffer");

    for &(name, w, h) in RESOLUTIONS {
        let fb = Framebuffer::new(w, h, 0xFF336699);

        group.bench_with_input(BenchmarkId::new("clone", name), &(), |b, _| {
            b.iter(|| {
                black_box(fb.pixels.clone());
            });
        });
    }
    group.finish();
}

fn bench_framebuffer_argb_to_rgba(c: &mut Criterion) {
    let mut group = c.benchmark_group("screen_capture/argb_to_rgba");

    for &(name, w, h) in RESOLUTIONS {
        let pixel_count = (w * h) as usize;
        let argb_pixels: Vec<u32> = vec![0xFF_AA_BB_CC; pixel_count];

        group.bench_with_input(BenchmarkId::new("convert", name), &(), |b, _| {
            b.iter(|| {
                let mut rgba = Vec::with_capacity(pixel_count * 4);
                for &px in &argb_pixels {
                    let r = ((px >> 16) & 0xFF) as u8;
                    let g = ((px >> 8) & 0xFF) as u8;
                    let b_val = (px & 0xFF) as u8;
                    let a = ((px >> 24) & 0xFF) as u8;
                    rgba.push(r);
                    rgba.push(g);
                    rgba.push(b_val);
                    rgba.push(a);
                }
                black_box(rgba);
            });
        });
    }
    group.finish();
}

fn bench_framebuffer_blit(c: &mut Criterion) {
    let mut group = c.benchmark_group("screen_capture/blit");

    let blit_sizes: &[(&str, u32, u32)] = &[
        ("256x256", 256, 256),
        ("640x480", 640, 480),
        ("1280x720", 1280, 720),
    ];

    for &(name, sw, sh) in blit_sizes {
        let mut dst = Framebuffer::new(1920, 1080, 0xFF000000);
        let src = Framebuffer::new(sw, sh, 0xFF336699);

        group.bench_with_input(BenchmarkId::new("to_1080p", name), &(), |b, _| {
            b.iter(|| {
                dst.blit(black_box(&src), black_box(0), black_box(0));
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

criterion_group!(
    screen_capture_benches,
    bench_capture_png,
    bench_capture_bmp,
    bench_capture_raw,
    bench_capture_region_sizes,
    bench_framebuffer_copy,
    bench_framebuffer_argb_to_rgba,
    bench_framebuffer_blit,
);
criterion_main!(screen_capture_benches);
