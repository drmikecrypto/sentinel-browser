use wgpu::{
    Backends, Color, CommandEncoderDescriptor, Device, DeviceDescriptor, Features, Instance, InstanceDescriptor, Limits,
    LoadOp, Operations, PowerPreference, Queue, RenderPassColorAttachment, RenderPassDescriptor, RequestAdapterOptions,
    Surface, SurfaceConfiguration, SurfaceError, TextureUsages, TextureViewDescriptor, include_wgsl,
};
use winit::window::Window;
use std::sync::Arc;
use tracing::info;
use crate::layout::LayoutBox;
// Page content is rendered by WebView2; Spectre only draws the native chrome strip.
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use glyphon::{
    Attrs, Buffer, Cache, Color as TextColor, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Uniforms {
    screen_width: f32,
    screen_height: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct InstanceRaw {
    pos: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
}

impl InstanceRaw {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

pub struct Spectre {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    #[allow(dead_code)]
    window: Arc<Window>,
    render_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    // Text Rendering
    font_system: FontSystem,
    swash_cache: SwashCache,
    #[allow(dead_code)]
    cache: Cache,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,
    viewport: Viewport,
}

impl Spectre {
    pub async fn new(window: Arc<Window>) -> Self {
        info!("Initializing Spectre Rendering Engine...");
        
        let size = window.inner_size();

        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &DeviceDescriptor {
                    label: None,
                    required_features: Features::empty(),
                    required_limits: Limits::default(),
                    memory_hints: Default::default(),
                    experimental_features: Default::default(),
                    trace: Default::default(),
                },
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats.iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        // --- Shader & Pipeline Setup ---

        let shader = device.create_shader_module(include_wgsl!("shader.wgsl"));

        let uniform_size = std::mem::size_of::<Uniforms>() as wgpu::BufferAddress;
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: uniform_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
            label: Some("uniform_bind_group_layout"),
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: Some("uniform_bind_group"),
        });

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[InstanceRaw::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });
        
        // Initial uniform update
        queue.write_buffer(&uniform_buffer, 0, bytemuck::cast_slice(&[Uniforms {
            screen_width: size.width as f32,
            screen_height: size.height as f32,
        }]));

        // --- Text Renderer Setup ---
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(&device);
        let mut text_atlas = TextAtlas::new(&device, &queue, &cache, config.format);
        let text_renderer = TextRenderer::new(&mut text_atlas, &device, wgpu::MultisampleState::default(), None);
        let viewport = Viewport::new(&device, &cache);

        Self {
            surface,
            device,
            queue,
            config,
            size,
            window,
            render_pipeline,
            uniform_buffer,
            uniform_bind_group,
            font_system,
            swash_cache,
            cache,
            text_atlas,
            text_renderer,
            viewport,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            
            // Update Uniforms
            self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[Uniforms {
                screen_width: new_size.width as f32,
                screen_height: new_size.height as f32,
            }]));

            self.viewport.update(&self.queue, Resolution {
                width: new_size.width,
                height: new_size.height,
            });
        }
    }

    pub fn render_chrome(&mut self, chrome: &[LayoutBox]) -> Result<(), SurfaceError> {
        let all_boxes = chrome.to_vec();
        let content_count = 0usize;
        let scroll_y = 0.0f32;

        // --- Prepare Text ---
        let mut text_buffers = Vec::new();
        for (idx, box_item) in all_boxes.iter().enumerate() {
             if let Some(text_content) = &box_item.text {
                 let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(16.0, 20.0));
                buffer.set_size(&mut self.font_system, Some(box_item.width), Some(box_item.height));
                buffer.set_text(&mut self.font_system, text_content, &Attrs::new().family(Family::SansSerif), Shaping::Advanced, None);
                buffer.shape_until_scroll(&mut self.font_system, false);
                text_buffers.push((buffer, (box_item, idx < content_count))); 
            }
       }

        let text_areas = text_buffers.iter().map(|(buffer, (b, is_content))| {
             let is_url_input = b.x == 170.0 && b.y == 45.0 && b.height == 30.0 && b.color.r == 0.05;
             let is_brand = b.y == 0.0 && b.height == 35.0;
             let is_tab = b.y == 5.0 && b.height == 25.0 && b.link.as_deref().map(|l| l.starts_with("tab:")).unwrap_or(false);
             let is_new_tab = b.text.as_deref() == Some("+");
             let is_menu = b.text.as_deref() == Some("=");
             let is_back = b.text.as_deref() == Some("<");
             let is_forward = b.text.as_deref() == Some(">");
             let is_refresh = b.text.as_deref() == Some("R");
             let is_go = b.text.as_deref() == Some("GO");
             let is_shield = b.text.as_ref().map(|t| t.starts_with("Shield")).unwrap_or(false);

             let (padding_x, padding_y) = if is_url_input {
                 (10.0, 5.0)
             } else if is_brand || is_tab {
                 (8.0, 4.0)
             } else if is_new_tab {
                 (10.0, 2.0)
             } else if is_menu || is_back || is_forward || is_refresh {
                 (10.0, 5.0)
             } else if is_go {
                 (28.0, 5.0)
             } else if is_shield {
                 (8.0, 4.0)
             } else {
                 (8.0, 4.0)
             };

             // Determine text color
             let text_color = if is_go || (is_tab && b.color.g > 0.8) {
                 TextColor::rgb(0, 0, 0) // Black on cyan accent
             } else {
                 TextColor::rgb(255, 255, 255)
             };

             let y_shift = if *is_content { scroll_y } else { 0.0 };
             TextArea {
                 buffer,
                 left: b.x + padding_x,
                 top: b.y + padding_y - y_shift,
                 scale: 1.0,
                 bounds: TextBounds {
                     left: b.x as i32,
                     top: (b.y - y_shift) as i32,
                     right: (b.x + b.width) as i32,
                     bottom: (b.y + b.height - y_shift) as i32,
                 },
                 default_color: text_color,
                 custom_glyphs: &[],
             }
        }).collect::<Vec<_>>();

        self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.text_atlas,
            &self.viewport,
            text_areas,
            &mut self.swash_cache,
        ).unwrap();

        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });
        
        // Prepare Instance Buffer
        let instance_data = all_boxes
            .iter()
            .enumerate()
            .map(|(idx, b)| {
                let y_shift = if idx < content_count { scroll_y } else { 0.0 };
                InstanceRaw {
                    pos: [b.x, b.y - y_shift],
                    size: [b.width, b.height],
                    color: [b.color.r, b.color.g, b.color.b, b.color.a],
                }
            })
            .collect::<Vec<_>>();
        
        let instance_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&instance_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: 0.1, 
                            g: 0.1,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, instance_buffer.slice(..));
            
            // Draw 6 vertices (quad) * N instances
            render_pass.draw(0..6, 0..instance_data.len() as u32);
            
            // Render Text
            self.text_renderer.render(&self.text_atlas, &self.viewport, &mut render_pass).unwrap();
        }
        
        // Quiet chrome redraws — no per-frame logging.
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
