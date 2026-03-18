use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

use desktop_environment::renderer::{
    DamageTracker, DesktopRenderer, Framebuffer, Layer, SceneGraph, SceneSurface,
};
use desktop_environment::{Rectangle, SurfaceId, WindowState};

fn make_surface(id: SurfaceId, x: i32, y: i32, w: u32, h: u32, layer: Layer) -> SceneSurface {
    SceneSurface {
        id,
        layer,
        geometry: Rectangle {
            x,
            y,
            width: w,
            height: h,
        },
        visible: true,
        opacity: 1.0,
        title: format!("Window {}", id),
        is_active: false,
        window_state: WindowState::Normal,
    }
}

fn bench_render_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("renderer");

    for surface_count in [1, 5, 10, 20] {
        let mut renderer = DesktopRenderer::new(1920, 1080);
        let mut scene = SceneGraph::new();

        for i in 0..surface_count {
            let id = uuid::Uuid::new_v4();
            let surface =
                make_surface(id, (i * 50) % 1600, (i * 40) % 800, 400, 300, Layer::Normal);
            let buf = Framebuffer::new(400, 300, 0xFF336699);
            renderer.submit_buffer(id, buf);
            scene.add_surface(surface);
        }

        group.bench_with_input(
            BenchmarkId::new("render_frame", surface_count),
            &surface_count,
            |b, _| {
                b.iter(|| {
                    renderer.render_frame(black_box(&mut scene));
                });
            },
        );
    }

    group.finish();
}

fn bench_scene_graph_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("scene_graph");

    group.bench_function("add_100_surfaces", |b| {
        b.iter(|| {
            let mut scene = SceneGraph::new();
            for i in 0..100u32 {
                let id = uuid::Uuid::new_v4();
                let layer = match i % 4 {
                    0 => Layer::Background,
                    1 => Layer::Normal,
                    2 => Layer::Floating,
                    _ => Layer::Overlay,
                };
                scene.add_surface(make_surface(id, i as i32 * 10, 0, 200, 150, layer));
            }
            black_box(&scene);
        });
    });

    group.bench_function("z_order_rebuild_50_surfaces", |b| {
        let mut scene = SceneGraph::new();
        for i in 0..50u32 {
            let id = uuid::Uuid::new_v4();
            let layer = match i % 5 {
                0 => Layer::Background,
                1 => Layer::Normal,
                2 => Layer::Floating,
                3 => Layer::Panel,
                _ => Layer::Overlay,
            };
            scene.add_surface(make_surface(id, i as i32 * 10, 0, 200, 150, layer));
        }
        b.iter(|| {
            black_box(scene.surfaces_in_order());
        });
    });

    group.finish();
}

fn bench_framebuffer(c: &mut Criterion) {
    let mut group = c.benchmark_group("framebuffer");

    group.bench_function("create_1080p", |b| {
        b.iter(|| black_box(Framebuffer::new(1920, 1080, 0xFF000000)));
    });

    group.bench_function("clear_1080p", |b| {
        let mut fb = Framebuffer::new(1920, 1080, 0xFF000000);
        b.iter(|| fb.clear(black_box(0xFF222222)));
    });

    group.bench_function("blit_400x300", |b| {
        let mut dst = Framebuffer::new(1920, 1080, 0xFF000000);
        let src = Framebuffer::new(400, 300, 0xFF336699);
        b.iter(|| dst.blit(black_box(&src), black_box(100), black_box(100)));
    });

    group.finish();
}

fn bench_damage_tracker(c: &mut Criterion) {
    let mut group = c.benchmark_group("damage_tracker");

    group.bench_function("add_50_damage_regions", |b| {
        b.iter(|| {
            let mut tracker = DamageTracker::new(1920, 1080);
            tracker.flush();
            for i in 0..50 {
                tracker.add_damage(Rectangle {
                    x: i * 30,
                    y: i * 20,
                    width: 100,
                    height: 80,
                });
            }
            black_box(&tracker);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_render_frame,
    bench_scene_graph_operations,
    bench_framebuffer,
    bench_damage_tracker,
);
criterion_main!(benches);
