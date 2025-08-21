use crevice::std140::{AsStd140, Std140};
use std::{borrow::Cow, cell::RefCell, fmt::Formatter};

use wgpu::{
    Adapter, BindGroup, Color, Device, Queue, RenderPipeline, Surface, Texture, TextureFormat,
    util::DeviceExt,
};

use crate::{AppState, gui::GuiRenderData, utils::Vec4u};

pub const FRAME_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;
pub const FRAME_FORMAT_FLOAT: TextureFormat = TextureFormat::Rgba32Float;

#[derive(Clone, PartialEq, Eq)]
pub enum RenderResolution {
    Ws360P,
    Ws720P,
    Ws1080P,
    Ws1440P,
    Ws2160P,
    Native(u32, u32),
}

impl RenderResolution {
    fn width(&self) -> u32 {
        match self {
            RenderResolution::Ws360P => 640,
            RenderResolution::Ws720P => 1280,
            RenderResolution::Ws1080P => 1920,
            RenderResolution::Ws1440P => 2560,
            RenderResolution::Ws2160P => 3840,
            RenderResolution::Native(w, _) => *w,
        }
    }

    fn height(&self) -> u32 {
        match self {
            RenderResolution::Ws360P => 360,
            RenderResolution::Ws720P => 720,
            RenderResolution::Ws1080P => 1080,
            RenderResolution::Ws1440P => 1440,
            RenderResolution::Ws2160P => 2160,
            RenderResolution::Native(_, h) => *h,
        }
    }
}

impl std::fmt::Display for RenderResolution {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderResolution::Ws360P => write!(f, "360p"),
            RenderResolution::Ws720P => write!(f, "720p"),
            RenderResolution::Ws1080P => write!(f, "1080p"),
            RenderResolution::Ws1440P => write!(f, "1440p"),
            RenderResolution::Ws2160P => write!(f, "2160p"),
            RenderResolution::Native(w, h) => write!(f, "{}x{}", w, h),
        }
    }
}

pub struct RenderFrame {
    sample_texture: Texture,
    blit_front_texture: Texture,
    blit_back_texture: Texture,
    depth_texture: Texture,
    sample_bind_group_front: BindGroup,
    sample_bind_group_back: BindGroup,
    blit_front_bind_group: BindGroup,
    blit_back_bind_group: BindGroup,
}

pub struct Display {
    surface: Surface<'static>,
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
    downsample_pipeline: RenderPipeline,
    blit_pipeline: RenderPipeline,
    ui_renderer: RefCell<egui_wgpu::Renderer>,
    pub backend: wgpu::Backend,
    uniform_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
}

impl Display {
    pub async fn from_canvas(canvas: &web_sys::HtmlCanvasElement) -> Self {
        crate::utils::yield_async(10).await;

        let descriptor = wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            flags: wgpu::InstanceFlags::VALIDATION,
            ..Default::default()
        };
        let instance = wgpu::util::new_instance_with_webgpu_detection(&descriptor).await;
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .unwrap();
        let adapter_options = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        };
        let adapter = instance
            .request_adapter(&adapter_options)
            .await
            .expect("adapter supports WebGPU or WebGL");

        let limits = adapter.limits();
        web_sys::console::log_1(&format!("Adapter limits: {:?}", limits).into());
        let info = adapter.get_info();
        web_sys::console::log_1(&format!("Adapter info: {:?}", info).into());
        let backend = info.backend;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .expect("adapter supports WebGPU");

        let mut surface_config = surface
            .get_default_config(&adapter, 512, 512)
            .expect("adapter supports config");
        surface_config.format = surface_config.format.remove_srgb_suffix();
        web_sys::console::log_1(&format!("Surface config: {:?}", surface_config).into());
        surface.configure(&device, &surface_config);

        let ui_renderer = RefCell::new(egui_wgpu::Renderer::new(
            &device,
            surface_config.format,
            None,
            1,
            false,
        ));

        let downsample_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "shaders/downsample.wgsl"
            ))),
        });
        let downsample_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });

        let supersample_vec = Vec4u::new(5, 0, 0, 0);
        let supersample_vec: mint::Vector4<u32> = supersample_vec.into();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: supersample_vec.as_std140().as_bytes(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(
                            (mint::Vector4::<u32>::std140_size_static() as u64)
                                .try_into()
                                .unwrap(),
                        ),
                    },
                    count: None,
                }],
            });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
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

        let downsample_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("downsample_pipeline_layout"),
                bind_group_layouts: &[&downsample_bind_group_layout, &uniform_bind_group_layout],
                push_constant_ranges: &[],
            });
        let downsample_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("downsample_pipeline"),
            layout: Some(&downsample_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &downsample_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &downsample_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FRAME_FORMAT_FLOAT,
                    blend: None, //Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multiview: None,
            cache: None,
            multisample: Default::default(),
        });

        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/blit.wgsl"))),
        });
        let blit_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("blit_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });
        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&blit_bind_group_layout],
            push_constant_ranges: &[],
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit_pipeline"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(surface_config.format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multiview: None,
            cache: None,
            multisample: Default::default(),
        });

        Self {
            surface,
            adapter,
            device,
            queue,
            downsample_pipeline,
            blit_pipeline,
            ui_renderer,
            backend,
            uniform_bind_group,
            uniform_buffer,
        }
    }

    pub fn create_render_frame(
        &self,
        resolution: &RenderResolution,
        supersample: u32,
    ) -> RenderFrame {
        let width = resolution.width();
        let height = resolution.height();

        let sample_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: width * supersample,
                height: height * supersample,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FRAME_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[FRAME_FORMAT],
        });
        let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: width * supersample,
                height: height * supersample,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let blit_front_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FRAME_FORMAT_FLOAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[FRAME_FORMAT_FLOAT],
        });
        let blit_back_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FRAME_FORMAT_FLOAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[FRAME_FORMAT_FLOAT],
        });

        let mut blit_view_desc = wgpu::TextureViewDescriptor::default();
        blit_view_desc.format = Some(FRAME_FORMAT_FLOAT);

        let blit_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let blit_front_view = blit_front_texture.create_view(&blit_view_desc);
        let blit_front_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit_front_bind_group"),
            layout: &self.blit_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&blit_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&blit_front_view),
                },
            ],
        });
        let blit_back_view = blit_back_texture.create_view(&blit_view_desc);
        let blit_back_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit_back_bind_group"),
            layout: &self.blit_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&blit_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&blit_back_view),
                },
            ],
        });

        let mut sample_view_desc = wgpu::TextureViewDescriptor::default();
        sample_view_desc.format = Some(FRAME_FORMAT);

        let sample_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let sample_view = sample_texture.create_view(&sample_view_desc);
        let sample_bind_group_front = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sample_bind_group_front"),
            layout: &self.downsample_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sample_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&sample_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&blit_back_view),
                },
            ],
        });
        let sample_bind_group_back = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sample_bind_group_back"),
            layout: &self.downsample_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sample_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&sample_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&blit_front_view),
                },
            ],
        });

        RenderFrame {
            sample_texture,
            blit_front_texture,
            blit_back_texture,
            depth_texture,
            sample_bind_group_front,
            sample_bind_group_back,
            blit_front_bind_group,
            blit_back_bind_group,
        }
    }

    pub fn render(
        &self,
        gui_render_data: GuiRenderData,
        state: &mut AppState,
        canvas_width: u32,
        canvas_height: u32,
        subframe_count: u32,
        stale_camera: bool,
    ) {
        if let Some(scene) = &state.scene {
            let sample_view =
                state
                    .render_frame
                    .sample_texture
                    .create_view(&wgpu::TextureViewDescriptor {
                        format: Some(FRAME_FORMAT),
                        ..Default::default()
                    });
            let depth_view =
                state
                    .render_frame
                    .depth_texture
                    .create_view(&wgpu::TextureViewDescriptor {
                        format: Some(wgpu::TextureFormat::Depth32Float),
                        ..Default::default()
                    });

            for _i in 0..subframe_count {
                std::mem::swap(
                    &mut state.render_frame.blit_front_texture,
                    &mut state.render_frame.blit_back_texture,
                );
                std::mem::swap(
                    &mut state.render_frame.blit_front_bind_group,
                    &mut state.render_frame.blit_back_bind_group,
                );
                std::mem::swap(
                    &mut state.render_frame.sample_bind_group_front,
                    &mut state.render_frame.sample_bind_group_back,
                );
                let mut encoder = self
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                let mut splat_render_pass =
                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &sample_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(Color {
                                    r: 0.0,
                                    g: 0.0,
                                    b: 0.0,
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &depth_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                scene.draw(
                    &self.queue,
                    &mut splat_render_pass,
                    (state.render_resolution.width() * state.supersample) as i32,
                    (state.render_resolution.height() * state.supersample) as i32,
                    state.supersample,
                    state.azimuth,
                    state.elevation,
                    state.zoom,
                );

                std::mem::drop(splat_render_pass);

                let blit_view = state.render_frame.blit_front_texture.create_view(
                    &wgpu::TextureViewDescriptor {
                        format: Some(FRAME_FORMAT_FLOAT),
                        ..Default::default()
                    },
                );

                let mut downsample_render_pass =
                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &blit_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                let supersample_vec = Vec4u::new(state.supersample, stale_camera as u32, 0, 0);
                let supersample_vec: mint::Vector4<u32> = supersample_vec.into();
                self.queue.write_buffer(
                    &self.uniform_buffer,
                    0,
                    supersample_vec.as_std140().as_bytes(),
                );
                downsample_render_pass.set_pipeline(&self.downsample_pipeline);
                downsample_render_pass.set_bind_group(
                    0,
                    &state.render_frame.sample_bind_group_front,
                    &[],
                );
                downsample_render_pass.set_bind_group(1, &self.uniform_bind_group, &[]);
                downsample_render_pass.draw(0..4, 0..1);

                std::mem::drop(downsample_render_pass);

                self.queue.submit(Some(encoder.finish()));
            }
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let GuiRenderData {
            textures_delta,
            shapes,
            pixels_per_point,
        } = gui_render_data;

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [canvas_width, canvas_height],
            pixels_per_point,
        };

        let clipped_primitives = state
            .gui_state
            .egui_ctx()
            .tessellate(shapes, pixels_per_point);

        let mut ui_renderer = self.ui_renderer.borrow_mut();

        for tex_id in &textures_delta.free {
            ui_renderer.free_texture(tex_id);
        }
        for (tex_id, delta) in &textures_delta.set {
            ui_renderer.update_texture(&self.device, &self.queue, *tex_id, delta);
        }

        let surface_texture = {
            let texture_result = self.surface.get_current_texture();
            let needs_configure = if let Ok(surface_texture) = &texture_result {
                if surface_texture.texture.width() != canvas_width
                    || surface_texture.texture.height() != canvas_height
                {
                    true
                } else {
                    false
                }
            } else {
                true
            };
            if needs_configure {
                std::mem::drop(texture_result);
                let mut config = self
                    .surface
                    .get_default_config(&self.adapter, canvas_width, canvas_height)
                    .expect("adapter supports config");
                config.format = config.format.remove_srgb_suffix();
                self.surface.configure(&self.device, &config);
                self.surface.get_current_texture().unwrap()
            } else {
                texture_result.unwrap()
            }
        };

        ui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.01,
                        g: 0.01,
                        b: 0.01,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        if state.scene.is_some() {
            let frame_width = state.render_resolution.width();
            let frame_height = state.render_resolution.height();

            let frame_aspect = frame_width as f32 / frame_height as f32;
            let canvas_aspect = canvas_width as f32 / canvas_height as f32;

            if frame_aspect > canvas_aspect {
                let box_width = canvas_width as f32;
                let box_height = canvas_width as f32 / frame_aspect;
                let border = (canvas_height as f32 - box_height) / 2.0;
                render_pass.set_viewport(0.0, border, box_width, box_height, 0.0, 1.0);
            } else {
                let box_width = canvas_height as f32 * frame_aspect;
                let box_height = canvas_height as f32;
                let border = (canvas_width as f32 - box_width) / 2.0;
                render_pass.set_viewport(border, 0.0, box_width, box_height, 0.0, 1.0);
            }
            render_pass.set_pipeline(&self.blit_pipeline);
            render_pass.set_bind_group(0, &state.render_frame.blit_front_bind_group, &[]);
            render_pass.draw(0..4, 0..1);
        }

        ui_renderer.render(
            &mut render_pass.forget_lifetime(),
            &clipped_primitives,
            &screen_descriptor,
        );

        self.queue.submit(Some(encoder.finish()));

        surface_texture.present();
    }
}
