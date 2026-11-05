// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The wgpu side of the stage: pipelines, a scene's buffers and textures, and drawing one picture
//! into a texture of its own. It uses the window's device and queue but none of egui's state, so
//! it also runs headless in tests.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use egui_wgpu::wgpu::{self, util::DeviceExt};
use glam::{Mat4, Vec3};

use crate::{
    camera::Lights,
    scene::{Image, Material, Scene},
};

/// The picture's format: sRGB like egui's own textures, so egui shows it with the right colours.
pub const COLOUR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// The model's textures are read as stored (gamma-encoded), the way the client blends them.
const TEXTURE: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
/// How dark the ground shadow is at its centre.
const SHADOW_OPACITY: f32 = 0.5;
/// The shadow reaches this far past the model's footprint, as a share of the footprint.
const SHADOW_REACH: f32 = 1.3;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuVertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

/// Mirrors `Globals` in the shader.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    key: [f32; 4],
    fill: [f32; 4],
    rim: [f32; 4],
    front: [f32; 4],
    shadow: [f32; 4],
    shadow_size: [f32; 4],
}

/// Mirrors `Part` in the shader.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PartUniform {
    offset: [f32; 4],
    paint: [f32; 4],
    flags: [f32; 4],
}

/// Where one picture is seen from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub lights: Lights,
}

struct GpuPart {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    bind_group: wgpu::BindGroup,
}

/// A scene on the GPU.
pub struct GpuScene {
    parts: Vec<GpuPart>,
    /// Centre x, y and ground height of the shadow, then its half extents.
    shadow: [f32; 4],
    shadow_size: [f32; 4],
}

/// The textures one picture is drawn into: the colour egui shows, and when anti-aliased the
/// multisampled colour resolved into it; plus depth.
pub struct Target {
    pub size: [u32; 2],
    pub samples: u32,
    colour: wgpu::Texture,
    pub colour_view: wgpu::TextureView,
    msaa: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
}

struct Pipelines {
    model: wgpu::RenderPipeline,
    shadow: wgpu::RenderPipeline,
}

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    shader: wgpu::ShaderModule,
    part_layout: wgpu::BindGroupLayout,
    model_layout: wgpu::PipelineLayout,
    shadow_layout: wgpu::PipelineLayout,
    /// Built on first use for each sample count.
    pipelines: HashMap<u32, Pipelines>,
    sampler: wgpu::Sampler,
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    /// Stand-ins for a part without details (transparent) or without a lightmap or a paint
    /// picture (white).
    no_details: wgpu::TextureView,
    no_lightmap: wgpu::TextureView,
}

impl Gpu {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Gpu {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stage"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gpu/stage.wgsl").into()),
        });
        let uniform = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stage globals"),
            entries: &[uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let part_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stage part"),
            entries: &[
                uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                texture(1),
                texture(2),
                texture(4),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let model_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stage model"),
            bind_group_layouts: &[Some(&globals_layout), Some(&part_layout)],
            immediate_size: 0,
        });
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stage shadow"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("stage surface"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let no_details = texture_view(device, queue, [1, 1], 1, &[0, 0, 0, 0]);
        let no_lightmap = texture_view(device, queue, [1, 1], 1, &[255; 4]);
        Gpu {
            device: device.clone(),
            queue: queue.clone(),
            shader,
            part_layout,
            model_layout,
            shadow_layout,
            pipelines: HashMap::new(),
            sampler,
            globals,
            globals_group,
            no_details,
            no_lightmap,
        }
    }

    /// Whether the adapter can anti-alias the stage's picture with `samples` samples.
    pub fn supports_samples(adapter: &wgpu::Adapter, samples: u32) -> bool {
        samples == 1
            || [COLOUR, DEPTH].iter().all(|format| {
                adapter
                    .get_texture_format_features(*format)
                    .flags
                    .sample_count_supported(samples)
            })
    }

    /// Put a scene on the GPU.
    pub fn upload(&self, scene: &Scene) -> GpuScene {
        let parts = scene
            .parts
            .iter()
            .filter(|part| !part.geometry.indices.is_empty())
            .map(|part| {
                let vertices: Vec<GpuVertex> = part
                    .geometry
                    .vertices
                    .iter()
                    .map(|v| GpuVertex {
                        position: v.position,
                        normal: v.normal,
                        uv: v.uv,
                    })
                    .collect();
                let vertices = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("stage vertices"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                let indices = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("stage indices"),
                        contents: bytemuck::cast_slice(&part.geometry.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });
                GpuPart {
                    vertices,
                    indices,
                    count: part.geometry.indices.len() as u32,
                    bind_group: self.material(&part.material, part.offset),
                }
            })
            .collect();
        let bounds = scene.bounds();
        let centre = bounds.center();
        let size = bounds.size();
        GpuScene {
            parts,
            shadow: [
                centre[0],
                centre[1],
                bounds.min[2] - bounds.radius() * 0.002,
                SHADOW_OPACITY,
            ],
            // Wider than the model, so the shadow fades out beyond its edges.
            shadow_size: [
                size[0] * 0.5 * SHADOW_REACH + 1.0,
                size[1] * 0.5 * SHADOW_REACH + 1.0,
                0.0,
                0.0,
            ],
        }
    }

    fn material(&self, material: &Material, offset: [f32; 3]) -> wgpu::BindGroup {
        let [r, g, b] = material.paint.map(|c| f32::from(c) / 255.0);
        let uniform = PartUniform {
            offset: [offset[0], offset[1], offset[2], 0.0],
            paint: [r, g, b, 1.0],
            flags: [
                f32::from(u8::from(material.details.is_some())),
                f32::from(u8::from(material.lightmap.is_some())),
                f32::from(u8::from(material.paint_picture.is_some())),
                0.0,
            ],
        };
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("stage part"),
                contents: bytemuck::bytes_of(&uniform),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let details = material
            .details
            .as_ref()
            .map(|image| upload_image(&self.device, &self.queue, image));
        let lightmap = material
            .lightmap
            .as_ref()
            .map(|image| upload_image(&self.device, &self.queue, image));
        let paint = material
            .paint_picture
            .as_ref()
            .map(|image| upload_image(&self.device, &self.queue, image));
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage part"),
            layout: &self.part_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        details.as_ref().unwrap_or(&self.no_details),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        lightmap.as_ref().unwrap_or(&self.no_lightmap),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        paint.as_ref().unwrap_or(&self.no_lightmap),
                    ),
                },
            ],
        })
    }

    /// New textures to draw a `size` picture into with `samples` anti-aliasing samples.
    pub fn target(&self, size: [u32; 2], samples: u32) -> Target {
        let extent = wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        };
        let texture = |label, format, samples, usage| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let colour = texture(
            "stage picture",
            COLOUR,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        );
        let msaa = (samples > 1).then(|| {
            texture(
                "stage picture (multisampled)",
                COLOUR,
                samples,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            )
            .create_view(&Default::default())
        });
        let depth = texture(
            "stage depth",
            DEPTH,
            samples,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
        .create_view(&Default::default());
        Target {
            size: [extent.width, extent.height],
            samples,
            colour_view: colour.create_view(&Default::default()),
            colour,
            msaa,
            depth,
        }
    }

    /// Draw `scene` seen from `view` into `target`: clear (transparent, so the page behind shows),
    /// the ground shadow, then the model.
    pub fn draw(&mut self, scene: Option<&GpuScene>, target: &Target, view: &View) {
        let globals = Globals {
            view_proj: view.view_proj.to_cols_array_2d(),
            eye: view.eye.extend(1.0).to_array(),
            key: view.lights.key.extend(0.0).to_array(),
            fill: view.lights.fill.extend(0.0).to_array(),
            rim: view.lights.rim.extend(0.0).to_array(),
            front: view.lights.front.extend(0.0).to_array(),
            shadow: scene.map_or([0.0; 4], |s| s.shadow),
            shadow_size: scene.map_or([0.0; 4], |s| s.shadow_size),
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        self.pipelines(target.samples);
        let Some(pipelines) = self.pipelines.get(&target.samples) else {
            return;
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("stage"),
            });
        {
            let (view, resolve_target) = match &target.msaa {
                Some(msaa) => (msaa, Some(&target.colour_view)),
                None => (&target.colour_view, None),
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("stage"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            if let Some(scene) = scene {
                pass.set_bind_group(0, &self.globals_group, &[]);
                pass.set_pipeline(&pipelines.shadow);
                pass.draw(0..6, 0..1);
                pass.set_pipeline(&pipelines.model);
                for part in &scene.parts {
                    pass.set_bind_group(1, &part.bind_group, &[]);
                    pass.set_vertex_buffer(0, part.vertices.slice(..));
                    pass.set_index_buffer(part.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..part.count, 0, 0..1);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// Read a drawn picture back (for saving it), its alpha made straight again.
    pub fn read(&self, target: &Target) -> Option<Image> {
        let [width, height] = target.size;
        let row = width as u64 * 4;
        let align = u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let padded = row.div_ceil(align) * align;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stage readback"),
            size: padded * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("stage readback"),
            });
        encoder.copy_texture_to_buffer(
            target.colour.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded as u32),
                    rows_per_image: Some(height),
                },
            },
            target.colour.size(),
        );
        self.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let mapped = slice.get_mapped_range().ok()?;
        let mut rgba = Vec::with_capacity((row * u64::from(height)) as usize);
        for line in mapped.chunks(padded as usize).take(height as usize) {
            for pixel in line.get(..row as usize)?.as_chunks::<4>().0 {
                let alpha = pixel[3];
                let straight = |c: u8| {
                    if alpha == 0 {
                        0
                    } else {
                        ((u32::from(c) * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255)
                            as u8
                    }
                };
                rgba.extend([
                    straight(pixel[0]),
                    straight(pixel[1]),
                    straight(pixel[2]),
                    alpha,
                ]);
            }
        }
        drop(mapped);
        buffer.unmap();
        Image::new(width, height, rgba)
    }

    fn pipelines(&mut self, samples: u32) {
        if self.pipelines.contains_key(&samples) {
            return;
        }
        let multisample = wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        };
        let depth = |write| wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(write),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };
        let model = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("stage model"),
                layout: Some(&self.model_layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_model"),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GpuVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3,
                            1 => Float32x3,
                            2 => Float32x2
                        ],
                    })],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs_model"),
                    targets: &[Some(COLOUR.into())],
                    compilation_options: Default::default(),
                }),
                // Both sides: exports are not always consistently wound.
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(depth(true)),
                multisample,
                multiview_mask: None,
                cache: None,
            });
        let shadow = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("stage shadow"),
                layout: Some(&self.shadow_layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_shadow"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs_shadow"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOUR,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(depth(false)),
                multisample,
                multiview_mask: None,
                cache: None,
            });
        self.pipelines.insert(samples, Pipelines { model, shadow });
    }
}

/// An image as a sampled texture with its full mip chain.
fn upload_image(device: &wgpu::Device, queue: &wgpu::Queue, image: &Image) -> wgpu::TextureView {
    let chain = image.mip_chain();
    let data: Vec<u8> = chain.iter().flat_map(|mip| mip.rgba()).copied().collect();
    texture_view(
        device,
        queue,
        [image.width(), image.height()],
        chain.len() as u32,
        &data,
    )
}

/// A sampled texture of `size` with `mips` levels, `data` holding every level in turn.
fn texture_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: [u32; 2],
    mips: u32,
    data: &[u8],
) -> wgpu::TextureView {
    device
        .create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("stage texture"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: TEXTURE,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            data,
        )
        .create_view(&Default::default())
}

#[cfg(test)]
mod tests;
