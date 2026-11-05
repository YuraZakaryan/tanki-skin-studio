// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Drawing on a real adapter, headless. A machine without one (some CI runners) skips these with
//! a note instead of failing.

use std::sync::Arc;

use tanki_model::{Geometry, read};
use tanki_testkit::three_ds::{hull_3ds, turret_3ds};

use super::*;
use crate::{camera::OrbitCamera, scene::Part};

fn device() -> Option<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
    Some((adapter, device, queue))
}

fn tank(paint: [u8; 3]) -> Scene {
    let part = |bytes: &[u8], offset| Part {
        geometry: Arc::new(Geometry::from_meshes(&read(bytes).unwrap().meshes)),
        material: Material {
            paint,
            ..Material::default()
        },
        offset,
    };
    Scene {
        parts: vec![
            part(&hull_3ds(), [0.0; 3]),
            part(&turret_3ds(), [0.0, 0.0, 110.0]),
        ],
    }
}

fn picture(gpu: &mut Gpu, scene: Option<&GpuScene>, samples: u32) -> Image {
    let mut camera = OrbitCamera::default();
    let bounds = tank([0; 3]).bounds();
    camera.frame(&bounds);
    let target = gpu.target([96, 64], samples);
    let view = View {
        view_proj: camera.view_projection(96.0 / 64.0),
        eye: camera.eye(),
        lights: camera.lights(),
    };
    gpu.draw(scene, &target, &view);
    gpu.read(&target).unwrap()
}

fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * image.width() + x) * 4) as usize;
    image.rgba()[at..at + 4].try_into().unwrap()
}

#[test]
fn the_tank_is_drawn_opaque_in_the_middle_on_a_clear_background() {
    let Some((adapter, device, queue)) = device() else {
        eprintln!("no GPU adapter here: skipped");
        return;
    };
    let mut gpu = Gpu::new(&device, &queue);
    let scene = gpu.upload(&tank([200, 40, 40]));
    for samples in [1, 4] {
        if !Gpu::supports_samples(&adapter, samples) {
            continue;
        }
        let image = picture(&mut gpu, Some(&scene), samples);
        let centre = pixel(&image, 48, 32);
        assert_eq!(centre[3], 255, "the model covers the middle: {centre:?}");
        assert!(
            centre[0] > centre[1] && centre[0] > centre[2],
            "red paint: {centre:?}"
        );
        assert_eq!(pixel(&image, 0, 0), [0, 0, 0, 0], "the corner stays clear");
    }
}

#[test]
fn an_empty_stage_is_fully_transparent() {
    let Some((_, device, queue)) = device() else {
        eprintln!("no GPU adapter here: skipped");
        return;
    };
    let mut gpu = Gpu::new(&device, &queue);
    let image = picture(&mut gpu, None, 1);
    assert!(
        image
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 0, 0, 0])
    );
}

#[test]
fn a_paint_picture_takes_the_place_of_the_paint_colour() {
    let Some((_, device, queue)) = device() else {
        eprintln!("no GPU adapter here: skipped");
        return;
    };
    let mut gpu = Gpu::new(&device, &queue);
    let mut scene = tank([200, 40, 40]);
    let green = Arc::new(Image::new(2, 2, [30, 200, 60, 255].repeat(4)).unwrap());
    for part in &mut scene.parts {
        part.material.paint_picture = Some(green.clone());
    }
    let uploaded = gpu.upload(&scene);
    let centre = pixel(&picture(&mut gpu, Some(&uploaded), 1), 48, 32);
    assert!(
        centre[1] > centre[0],
        "green paint over the red colour: {centre:?}"
    );
}

#[test]
fn details_cover_the_paint_where_they_are_opaque() {
    let Some((_, device, queue)) = device() else {
        eprintln!("no GPU adapter here: skipped");
        return;
    };
    let mut gpu = Gpu::new(&device, &queue);
    let mut scene = tank([200, 40, 40]);
    let blue = Arc::new(Image::new(2, 2, [20, 40, 220, 255].repeat(4)).unwrap());
    for part in &mut scene.parts {
        part.material.details = Some(blue.clone());
    }
    let uploaded = gpu.upload(&scene);
    let centre = pixel(&picture(&mut gpu, Some(&uploaded), 1), 48, 32);
    assert!(
        centre[2] > centre[0],
        "blue details over red paint: {centre:?}"
    );
}

/// The lightmap value that leaves a colour as it is: half brightness, lit up by the garage's exposure
/// (1.6, see the shader).
const NEUTRAL_LIGHTMAP: u8 = (255.0 * 0.5 / 1.6) as u8;

#[test]
fn a_neutral_lightmap_leaves_the_colour_as_it_is() {
    let Some((_, device, queue)) = device() else {
        eprintln!("no GPU adapter here: skipped");
        return;
    };
    let mut gpu = Gpu::new(&device, &queue);
    let plain = gpu.upload(&tank([120, 120, 120]));
    let without = pixel(&picture(&mut gpu, Some(&plain), 1), 48, 32);
    let mut scene = tank([120, 120, 120]);
    let n = NEUTRAL_LIGHTMAP;
    let half = Arc::new(Image::new(1, 1, vec![n, n, n, 255]).unwrap());
    for part in &mut scene.parts {
        part.material.lightmap = Some(half.clone());
    }
    let lit = gpu.upload(&scene);
    let with = pixel(&picture(&mut gpu, Some(&lit), 1), 48, 32);
    assert!(
        (i16::from(with[0]) - i16::from(without[0])).abs() <= 3,
        "{with:?} against {without:?}"
    );
}

// --- Visual checks ------------------------------------------------------------------------------
//
// The picture's properties are checked at points projected from the scene, so they hold on any
// adapter (no golden images, which differ between GPUs). With `TANKI_VISUAL_DIR` set, the pictures
// are also saved there as PNG for a person to look at.

const SIZE: [u32; 2] = [320, 200];

/// A box (Z up) of one paint, and a camera framing it from the home view.
fn boxed(paint: [u8; 3]) -> (Scene, OrbitCamera) {
    use tanki_testkit::three_ds::{Mesh3ds, model_3ds};
    let model = read(&model_3ds(&[Mesh3ds::cuboid(
        "box",
        [-100.0, -150.0, 0.0],
        [100.0, 150.0, 120.0],
    )]))
    .unwrap();
    let scene = Scene {
        parts: vec![Part {
            geometry: Arc::new(Geometry::from_meshes(&model.meshes)),
            material: Material {
                paint,
                ..Material::default()
            },
            offset: [0.0; 3],
        }],
    };
    let mut camera = OrbitCamera::default();
    camera.frame(&scene.bounds());
    (scene, camera)
}

fn draw(gpu: &mut Gpu, scene: &GpuScene, camera: &OrbitCamera, samples: u32) -> Image {
    let target = gpu.target(SIZE, samples);
    let view = View {
        view_proj: camera.view_projection(SIZE[0] as f32 / SIZE[1] as f32),
        eye: camera.eye(),
        lights: camera.lights(),
    };
    gpu.draw(Some(scene), &target, &view);
    gpu.read(&target).unwrap()
}

/// Where a point of the scene lands in the picture.
fn project(camera: &OrbitCamera, point: [f32; 3]) -> (u32, u32) {
    let clip = camera.view_projection(SIZE[0] as f32 / SIZE[1] as f32)
        * glam::Vec3::from_array(point).extend(1.0);
    let ndc = clip.truncate() / clip.w;
    let x = (ndc.x * 0.5 + 0.5) * SIZE[0] as f32;
    let y = (0.5 - ndc.y * 0.5) * SIZE[1] as f32;
    (
        (x.round() as u32).min(SIZE[0] - 1),
        (y.round() as u32).min(SIZE[1] - 1),
    )
}

fn luma(p: [u8; 4]) -> u32 {
    (u32::from(p[0]) * 3 + u32::from(p[1]) * 6 + u32::from(p[2])) / 10
}

fn save(name: &str, image: &Image) {
    let Some(dir) = std::env::var_os("TANKI_VISUAL_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let _ = std::fs::create_dir_all(&dir);
    if let Some(png) =
        image::RgbaImage::from_raw(image.width(), image.height(), image.rgba().to_vec())
    {
        let _ = png.save(dir.join(format!("{name}.png")));
    }
}

#[test]
fn the_top_catches_the_light_and_the_sides_are_told_apart() {
    let Some((_, device, queue)) = device() else {
        eprintln!("no GPU adapter here: skipped");
        return;
    };
    let mut gpu = Gpu::new(&device, &queue);
    let (scene, camera) = boxed([150, 150, 150]);
    let uploaded = gpu.upload(&scene);
    let image = draw(&mut gpu, &uploaded, &camera, 1);
    save("lighting-home", &image);
    // The home view looks from -X, +Y: the top, the -X side and the +Y side are seen.
    let at = |point| {
        let (x, y) = project(&camera, point);
        luma(pixel(&image, x, y))
    };
    let (top, left, front) = (
        at([0.0, 0.0, 120.0]),
        at([-100.0, 0.0, 60.0]),
        at([0.0, 150.0, 60.0]),
    );
    for (name, value) in [("top", top), ("left", left), ("front", front)] {
        assert!(
            (30..250).contains(&value),
            "{name} is neither black nor blown out: {value}"
        );
    }
    assert!(
        top > left.min(front),
        "the top catches the light: {top} {left} {front}"
    );
    assert!(
        left.abs_diff(front) >= 6,
        "two sides are told apart by the light: {left} {front}"
    );
}

#[test]
fn the_ground_shadow_sits_at_the_model_and_fades_out_away_from_it() {
    let Some((_, device, queue)) = device() else {
        eprintln!("no GPU adapter here: skipped");
        return;
    };
    let mut gpu = Gpu::new(&device, &queue);
    let (scene, camera) = boxed([150, 150, 150]);
    let uploaded = gpu.upload(&scene);
    let image = draw(&mut gpu, &uploaded, &camera, 1);
    let (nx, ny) = project(&camera, [-104.0, 0.0, 0.0]);
    let near = pixel(&image, nx, ny);
    assert!(near[3] > 20, "dark right at the model: {near:?}");
    assert!(near[0] < 8, "the shadow is black, premultiplied: {near:?}");
    let (fx, fy) = project(&camera, [-190.0, 0.0, 0.0]);
    let far = pixel(&image, fx, fy);
    assert!(
        far[3] < near[3] / 2,
        "fainter away from it: {far:?} against {near:?}"
    );
    assert_eq!(
        pixel(&image, 0, 0),
        [0, 0, 0, 0],
        "the background stays clear"
    );
}

#[test]
fn bright_paint_rolls_off_toward_white_and_keeps_its_hue() {
    let Some((_, device, queue)) = device() else {
        eprintln!("no GPU adapter here: skipped");
        return;
    };
    let mut gpu = Gpu::new(&device, &queue);
    // A lightmap twice as bright as the neutral one: the red is pushed past white.
    let (mut scene, camera) = boxed([230, 60, 40]);
    let n = NEUTRAL_LIGHTMAP * 2;
    scene.parts[0].material.lightmap =
        Some(Arc::new(Image::new(1, 1, vec![n, n, n, 255]).unwrap()));
    let uploaded = gpu.upload(&scene);
    let image = draw(&mut gpu, &uploaded, &camera, 1);
    save("tone-bright", &image);
    let (tx, ty) = project(&camera, [0.0, 0.0, 120.0]);
    let top = pixel(&image, tx, ty);
    assert!(
        top[0] > top[1] + 60 && top[0] > top[2] + 60,
        "still red, not washed out to white: {top:?}"
    );
}

#[test]
fn anti_aliasing_softens_the_edges_against_the_background() {
    let Some((adapter, device, queue)) = device() else {
        eprintln!("no GPU adapter here: skipped");
        return;
    };
    if !Gpu::supports_samples(&adapter, 4) {
        eprintln!("no 4x anti-aliasing here: skipped");
        return;
    }
    let mut gpu = Gpu::new(&device, &queue);
    let (scene, camera) = boxed([150, 150, 150]);
    let uploaded = gpu.upload(&scene);
    // Soft edge pixels: partly covered by the model, so partly opaque and not the black shadow.
    let soft = |image: &Image| {
        image
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0 && p[3] < 250 && p[0].max(p[1]).max(p[2]) > 24)
            .count()
    };
    let hard = draw(&mut gpu, &uploaded, &camera, 1);
    let smooth = draw(&mut gpu, &uploaded, &camera, 4);
    save("edges-1x", &hard);
    save("edges-4x", &smooth);
    assert!(
        soft(&smooth) > soft(&hard) + 20,
        "{} against {}",
        soft(&smooth),
        soft(&hard)
    );
}

#[test]
fn the_same_view_draws_the_same_picture_and_home_comes_back_to_it() {
    let Some((_, device, queue)) = device() else {
        eprintln!("no GPU adapter here: skipped");
        return;
    };
    let mut gpu = Gpu::new(&device, &queue);
    let (scene, mut camera) = boxed([150, 150, 150]);
    let uploaded = gpu.upload(&scene);
    let first = draw(&mut gpu, &uploaded, &camera, 1);
    assert_eq!(
        first,
        draw(&mut gpu, &uploaded, &camera, 1),
        "deterministic"
    );
    camera.nudge(1.2, 0.4);
    let turned = draw(&mut gpu, &uploaded, &camera, 1);
    save("turned", &turned);
    assert_ne!(first, turned, "turning changes the picture");
    camera.go_home();
    while camera.update(1.0 / 60.0) {}
    assert_eq!(
        first,
        draw(&mut gpu, &uploaded, &camera, 1),
        "home is the first view"
    );
}
