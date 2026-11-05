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
        light: camera.light(),
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

#[test]
fn a_half_bright_lightmap_leaves_the_colour_as_it_is() {
    let Some((_, device, queue)) = device() else {
        eprintln!("no GPU adapter here: skipped");
        return;
    };
    let mut gpu = Gpu::new(&device, &queue);
    let plain = gpu.upload(&tank([120, 120, 120]));
    let without = pixel(&picture(&mut gpu, Some(&plain), 1), 48, 32);
    let mut scene = tank([120, 120, 120]);
    let half = Arc::new(Image::new(1, 1, vec![128, 128, 128, 255]).unwrap());
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
