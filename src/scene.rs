use std::borrow::Cow;

use bytemuck::Pod;
use crevice::std140::AsStd140;
use wgpu::{
    Device, Queue, TextureFormat, VertexBufferLayout,
    util::{BufferInitDescriptor, DeviceExt},
};

use crate::{
    display::{Display, FRAME_FORMAT},
    load::TSplat,
    pbar::{Progress, ProgressBar},
    utils::{Mat4f, Vec2i, Vec3f},
};

const TEXTURE_WIDTH: u32 = 8192;
const ROWS_PER_CHUNK: u32 = 64;

fn get_padded_wh(count: usize) -> (u32, u32) {
    let w = TEXTURE_WIDTH;
    let num_chunks = (count as f32 / (w * ROWS_PER_CHUNK) as f32).ceil() as u32;
    let h = num_chunks * ROWS_PER_CHUNK;
    (w, h)
}

async fn upload_texture<T, I, F, U>(
    iter: I,
    num_texels: usize,
    format: TextureFormat,
    device: &Device,
    queue: &Queue,
    progress: F,
) -> wgpu::Texture
where
    T: Pod,
    I: Iterator<Item = T>,
    F: Fn(f32) -> U,
    U: Future<Output = ()>,
{
    let (w, h) = get_padded_wh(num_texels);
    let descriptor = wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[format],
    };
    let texture = device.create_texture(&descriptor);

    let zero_texel_iter = std::iter::repeat(T::zeroed());
    let mut padded_iter = iter.chain(zero_texel_iter);

    let mut buffer = Vec::with_capacity((w * ROWS_PER_CHUNK) as usize);
    let num_chunks = h / ROWS_PER_CHUNK;

    for chunk in 0..num_chunks {
        buffer.clear();
        for _ in 0..(w * ROWS_PER_CHUNK) {
            buffer.push(padded_iter.next().unwrap());
        }

        let chunk_extent = wgpu::Extent3d {
            width: w,
            height: ROWS_PER_CHUNK,
            depth_or_array_layers: 1,
        };

        let sub_texture = wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: 0,
                y: chunk * ROWS_PER_CHUNK,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        };

        let data_layout = wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * std::mem::size_of::<T>() as u32),
            rows_per_image: None,
        };

        queue.write_texture(
            sub_texture,
            bytemuck::cast_slice(&buffer),
            data_layout,
            chunk_extent,
        );
        queue.submit([]);

        progress(chunk as f32 / num_chunks as f32).await;
    }

    texture
}

#[derive(AsStd140)]
struct ShaderGlobals {
    fb_size: mint::Vector2<i32>,
    origin: mint::Vector3<f32>,
    num_tris: u32,
    seed: u32,
    vp: mint::ColumnMatrix4<f32>,
    supersample: u32,
}

impl Default for ShaderGlobals {
    fn default() -> Self {
        Self {
            fb_size: [0, 0].into(),
            origin: [0.0, 0.0, 0.0].into(),
            num_tris: 0,
            seed: Default::default(),
            vp: [0.0; 16].into(),
            supersample: 1,
        }
    }
}

pub struct Scene {
    shader_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    texture_bind_group: wgpu::BindGroup,
    uniform_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    num_tris: usize,
    pub t: u32,
}

impl Scene {
    pub async fn new(tsplat: TSplat, display: &Display, pbar: ProgressBar) -> Result<Self, String> {
        let num_tris = tsplat.points.len();

        let TSplat {
            points,
            alpha_sigma,
            sh,
        } = tsplat;

        let sampler = display.device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        pbar.update_status("uploading vertices to gpu".to_string())
            .await;

        let vertex_buffer = display.device.create_buffer_init(&BufferInitDescriptor {
            label: None,
            usage: wgpu::BufferUsages::VERTEX,
            contents: bytemuck::cast_slice(points.as_slice()),
        });

        pbar.update_status("uploading triangle colors to gpu".to_string())
            .await;

        let alpha_sigma_texture = upload_texture(
            alpha_sigma.into_iter(),
            num_tris,
            TextureFormat::Rg16Float,
            &display.device,
            &display.queue,
            |progress| pbar.update_progress(0.8 + 0.02 * progress),
        )
        .await;

        let sh_texture = upload_texture(
            sh.into_iter(),
            num_tris,
            TextureFormat::Rgba16Float,
            &display.device,
            &display.queue,
            |progress| pbar.update_progress(0.82 + 0.18 * progress),
        )
        .await;

        pbar.update_status("compiling shaders".to_string()).await;

        let shader = display
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                    "shaders/splatting.wgsl"
                ))),
            });

        let vertex_buffer_layout = VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            }],
        };

        let texture_bind_group_layout =
            display
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: None,
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                                view_dimension: wgpu::TextureViewDimension::D2,
                            },
                            count: None,
                        },
                    ],
                });
        let texture_bind_group = display
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(
                            &alpha_sigma_texture
                                .create_view(&wgpu::TextureViewDescriptor::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(
                            &sh_texture.create_view(&wgpu::TextureViewDescriptor::default()),
                        ),
                    },
                ],
            });

        let globals = ShaderGlobals::default();
        let uniform_buffer = display
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: globals.as_std140().as_bytes(),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let uniform_bind_group_layout =
            display
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: None,
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: Some(
                                (ShaderGlobals::std140_size_static() as u64)
                                    .try_into()
                                    .unwrap(),
                            ),
                        },
                        count: None,
                    }],
                });
        let uniform_bind_group = display
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &uniform_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                }],
            });

        let shader_pipeline_layout =
            display
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("shader_pipeline_layout"),
                    bind_group_layouts: &[&texture_bind_group_layout, &uniform_bind_group_layout],
                    push_constant_ranges: &[],
                });
        let shader_pipeline =
            display
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("shader_pipeline"),
                    layout: Some(&shader_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[vertex_buffer_layout],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: FRAME_FORMAT,
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: TextureFormat::Depth32Float,
                        depth_write_enabled: true,
                        depth_compare: wgpu::CompareFunction::Less,
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multiview: None,
                    cache: None,
                    multisample: Default::default(),
                });

        Ok(Self {
            shader_pipeline,
            vertex_buffer,
            texture_bind_group,
            uniform_bind_group,
            uniform_buffer,
            num_tris,
            t: 0,
        })
    }

    pub fn draw(
        &self,
        queue: &Queue,
        render_pass: &mut wgpu::RenderPass,
        width: i32,
        height: i32,
        supersample: u32,
        azimuth: f32,
        elevation: f32,
        zoom: f32,
    ) {
        let up = Vec3f::new(-0.0039, -0.8796, -0.4756);
        let center = Vec3f::new(0.0549, 0.3402, 0.2599) - up;
        let vx = Vec3f::new(1.0, 0.0, 0.0).cross(&up).normalize();
        let vy = up.cross(&vx).normalize();

        let r = zoom;
        let origin = center
            + r * (elevation.cos() * (azimuth.cos() * vx + azimuth.sin() * vy)
                + elevation.sin() * up);

        let view = Mat4f::look_at_rh(&origin.into(), &center.into(), &up);

        let aspect = width as f32 / height as f32;
        let proj = Mat4f::new_perspective(aspect, 0.85, 0.01, 100.0);
        let vp = proj * view;

        let globals = ShaderGlobals {
            fb_size: Vec2i::new(width, height).into(),
            origin: origin.into(),
            num_tris: self.num_tris as u32,
            seed: self.t,
            vp: vp.into(),
            supersample,
            ..ShaderGlobals::default()
        };
        queue.write_buffer(&self.uniform_buffer, 0, globals.as_std140().as_bytes());
        render_pass.set_pipeline(&self.shader_pipeline);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_bind_group(0, &self.texture_bind_group, &[]);
        render_pass.set_bind_group(1, &self.uniform_bind_group, &[]);
        render_pass.draw(0..(self.num_tris * 3) as u32, 0..1);
    }
}
