use cairo::{Context, Format, ImageSurface};
use osdockx::config::Config;
use osdockx::layout::Point;
use osdockx::model::{DockItem, DockModel, WindowIcon};
use osdockx::renderer::{IconCache, Renderer};
use osdockx::theme::Theme;
use std::hint::black_box;
use std::time::{Duration, Instant};

const ITERATIONS: usize = 240;

fn main() {
    let mut config = Config::default().normalized();
    config.dock.icon_size = 64;
    let theme = Theme::from_config(&config.theme);
    let model = fixture_model(15);
    let rest = Renderer::layout_for(&model, &config.dock, &theme, None);
    let center = rest.icons[rest.icons.len() / 2].rect;
    let hover = Point {
        x: center.center_x(),
        y: center.y + center.height * 0.5,
    };

    println!("OSDockX Cairo renderer benchmark: 15 icons, 64 CSS px");
    for scale in [1_i32, 2] {
        run_case("rest/reflections", scale, &model, &config, &theme, |_| None);
        run_case(
            "center-hover/reflections",
            scale,
            &model,
            &config,
            &theme,
            |_| Some(hover),
        );
        let left = rest.icons.first().unwrap().rect.center_x();
        let right = rest.icons.last().unwrap().rect.center_x();
        run_case(
            "sweep/reflections",
            scale,
            &model,
            &config,
            &theme,
            |index| {
                let progress = (index % ITERATIONS) as f64 / (ITERATIONS - 1) as f64;
                Some(Point {
                    x: left + (right - left) * progress,
                    y: hover.y,
                })
            },
        );

        let mut no_reflections = theme.clone();
        no_reflections.reflection_opacity = 0.0;
        run_case(
            "rest/no-reflections",
            scale,
            &model,
            &config,
            &no_reflections,
            |_| None,
        );
        run_case(
            "center-hover/no-reflections",
            scale,
            &model,
            &config,
            &no_reflections,
            |_| Some(hover),
        );
    }
}

fn run_case(
    name: &str,
    scale: i32,
    model: &DockModel,
    config: &Config,
    theme: &Theme,
    hover_for: impl Fn(usize) -> Option<Point>,
) {
    let css_size = Renderer::desired_size(model, &config.dock, theme, None);
    let surface = ImageSurface::create(Format::ARgb32, css_size.0 * scale, css_size.1 * scale)
        .expect("benchmark surface");
    surface.set_device_scale(scale as f64, scale as f64);
    let cr = Context::new(&surface).expect("benchmark context");
    let mut renderer = Renderer::new();
    let mut icons = IconCache::new();

    for index in 0..12 {
        renderer.draw(
            &cr,
            model,
            &config.dock,
            theme,
            hover_for(index),
            &mut icons,
        );
    }

    let mut samples = Vec::with_capacity(ITERATIONS);
    for index in 0..ITERATIONS {
        let started = Instant::now();
        renderer.draw(
            black_box(&cr),
            black_box(model),
            black_box(&config.dock),
            black_box(theme),
            black_box(hover_for(index)),
            black_box(&mut icons),
        );
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    let total: Duration = samples.iter().copied().sum();
    println!(
        "{name:30} {scale}x mean={:8.3}ms p50={:8.3}ms p95={:8.3}ms p99={:8.3}ms",
        total.as_secs_f64() * 1_000.0 / samples.len() as f64,
        percentile(&samples, 0.50).as_secs_f64() * 1_000.0,
        percentile(&samples, 0.95).as_secs_f64() * 1_000.0,
        percentile(&samples, 0.99).as_secs_f64() * 1_000.0,
    );
}

fn percentile(samples: &[Duration], percentile: f64) -> Duration {
    let index = ((samples.len() - 1) as f64 * percentile).round() as usize;
    samples[index]
}

fn fixture_model(count: usize) -> DockModel {
    DockModel {
        items: (0..count)
            .map(|index| DockItem {
                id: format!("fixture-{index}.desktop"),
                name: format!("Fixture {index}"),
                desktop_id: Some(format!("fixture-{index}.desktop")),
                startup_wm_class: None,
                icon_name: None,
                window_icon: Some(fixture_icon(index)),
                pinned: true,
                windows: Vec::new(),
                active: index % 4 == 0,
                urgent: index == 11,
                badge: (index == 7).then_some(3),
            })
            .collect(),
    }
}

fn fixture_icon(seed: usize) -> WindowIcon {
    let size = 16_u32;
    let pixels = (0..size * size)
        .map(|offset| {
            let x = offset % size;
            let y = offset / size;
            let red = ((x * 13 + seed as u32 * 29) & 0xff) as u8;
            let green = ((y * 17 + seed as u32 * 11) & 0xff) as u8;
            let blue = (((x + y) * 9 + seed as u32 * 7) & 0xff) as u8;
            0xff00_0000 | u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue)
        })
        .collect();
    WindowIcon::from_argb(size, size, pixels)
}
