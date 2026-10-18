use std::sync::Arc;
use std::time::Instant;

use half::f16;
use image::{DynamicImage, GenericImageView, ImageBuffer, Luma, Rgba};
use std::num::NonZero;

#[cfg(not(any(target_os = "android", target_os = "linux")))]
use tauri::Manager;
use wgpu::util::{DeviceExt, TextureDataOrder};

use crate::image_processing::{AllAdjustments, GpuContext, MAX_MASKS, MaskAdjustments};
use crate::lut_processing::Lut;
use crate::{AppState, GpuImageCache};

// Temporary debug-build diagnostics: no release logging or timing overhead.
#[cfg(debug_assertions)]
struct GpuDiagnosticSpan<'a> {
    label: &'a str,
    started: Instant,
}

#[cfg(debug_assertions)]
impl<'a> GpuDiagnosticSpan<'a> {
    fn new(label: &'a str) -> Self {
        log::info!(
            "[gpu-diag] thread={:?} ENTER {}",
            std::thread::current().id(),
            label
        );
        Self {
            label,
            started: Instant::now(),
        }
    }
}

#[cfg(debug_assertions)]
impl Drop for GpuDiagnosticSpan<'_> {
    fn drop(&mut self) {
        log::info!(
            "[gpu-diag] thread={:?} EXIT {} elapsed={:?}",
            std::thread::current().id(),
            self.label,
            self.started.elapsed()
        );
    }
}

macro_rules! gpu_diag {
    ($label:expr, $operation:expr) => {{
        #[cfg(debug_assertions)]
        let _gpu_diag_span = GpuDiagnosticSpan::new($label);
        $operation
    }};
}

#[derive(Clone, Copy, Debug)]
pub struct Roi {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// DCP profile table data for GPU upload (W4).
///
/// Each table is provided as flat `f32` data suitable for uploading as a 3D
/// (or 1D) texture. `huesat` and `look` use `rgba32float` format with implicit
/// alpha=1 for every texel; dims give the texture size. `tone_curve` is a 1-D
/// `r32float` LUT of 4096 entries.
#[derive(Clone)]
pub struct DcpTextureData {
    pub huesat: Vec<f32>,
    pub huesat_dims: [u32; 3],
    pub look: Vec<f32>,
    pub look_dims: [u32; 3],
    pub tone_curve: Vec<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderOutputPrecision {
    #[default]
    EightBit,
    SixteenBit,
}

pub struct RenderRequest<'a> {
    pub adjustments: AllAdjustments,
    pub mask_bitmaps: &'a [ImageBuffer<Luma<u8>, Vec<u8>>],
    pub lut: Option<Arc<Lut>>,
    pub roi: Option<Roi>,
    /// DCP profile table data for GPU upload. When `Some` and
    /// `adjustments.global.has_dcp != 0`, real DCP textures replace
    /// the dummy bindings (§6.W4 req 3).
    pub dcp_textures: Option<DcpTextureData>,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DisplayTransform {
    pub rect: [f32; 4],
    pub clip: [f32; 4],
    pub window: [f32; 2],
    pub image_size: [f32; 2],
    pub texture_size: [f32; 2],
    pub pixelated: f32,
    pub _pad: f32,
    pub bg_primary: [f32; 4],
    pub bg_secondary: [f32; 4],
}

pub struct WgpuDisplay {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
    pub transform_buffer: wgpu::Buffer,
    pub latest_transform: DisplayTransform,
    pub current_bind_group: Option<wgpu::BindGroup>,
}

impl WgpuDisplay {
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if let Some(bind_group) = &self.current_bind_group {
            let output = match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(tex)
                | wgpu::CurrentSurfaceTexture::Suboptimal(tex) => tex,
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    self.surface.configure(device, &self.config);
                    match self.surface.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(tex)
                        | wgpu::CurrentSurfaceTexture::Suboptimal(tex) => tex,
                        _ => panic!("Failed to acquire surface texture"),
                    }
                }
                _ => return,
            };
            let view = output
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: self.latest_transform.bg_primary[0] as f64,
                                g: self.latest_transform.bg_primary[1] as f64,
                                b: self.latest_transform.bg_primary[2] as f64,
                                a: self.latest_transform.bg_primary[3] as f64,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: NonZero::new(0),
                });
                let clip_x1 = self.latest_transform.clip[0].max(0.0);
                let clip_y1 = self.latest_transform.clip[1].max(0.0);
                let clip_x2 =
                    (self.latest_transform.clip[0] + self.latest_transform.clip[2]).max(0.0);
                let clip_y2 =
                    (self.latest_transform.clip[1] + self.latest_transform.clip[3]).max(0.0);

                let final_clip_x = clip_x1.floor() as u32;
                let final_clip_y = clip_y1.floor() as u32;
                let final_clip_w = (clip_x2.ceil() as u32).saturating_sub(final_clip_x);
                let final_clip_h = (clip_y2.ceil() as u32).saturating_sub(final_clip_y);

                let max_x = self.config.width;
                let max_y = self.config.height;

                if final_clip_x < max_x && final_clip_y < max_y {
                    let clamped_width = final_clip_w.min(max_x - final_clip_x);
                    let clamped_height = final_clip_h.min(max_y - final_clip_y);

                    if clamped_width > 0 && clamped_height > 0 {
                        rpass.set_scissor_rect(
                            final_clip_x,
                            final_clip_y,
                            clamped_width,
                            clamped_height,
                        );

                        rpass.set_pipeline(&self.pipeline);
                        rpass.set_bind_group(0, bind_group, &[]);
                        rpass.draw(0..4, 0..1);
                    }
                }
            }
            queue.submit(Some(encoder.finish()));
            output.present();
        }
    }
}

// These are the requirements of the existing compute shaders, not a claim
// that a downlevel adapter can provide them. Never raise an advertised limit.
fn required_processing_limits(supported: &wgpu::Limits) -> Result<wgpu::Limits, String> {
    let mut required = supported.clone();
    let mut missing = Vec::new();
    macro_rules! require {
        ($field:ident, $minimum:expr) => {
            if supported.$field < $minimum {
                missing.push(format!(
                    "{} requires {}, adapter supports {}",
                    stringify!($field),
                    $minimum,
                    supported.$field
                ));
            } else {
                required.$field = $minimum;
            }
        };
    }
    // Flare ghosts combines two groups, each with one compute storage texture.
    require!(max_storage_textures_per_shader_stage, 2);
    require!(max_storage_buffers_per_shader_stage, 1);
    require!(max_bind_groups, 2); // Flare ghosts uses both groups simultaneously.
    require!(max_uniform_buffers_per_shader_stage, 1); // Blur, flare and guided parameters.
    // Main BGL includes input, masks, LUT, effects, guided filters and DCP.
    require!(max_sampled_textures_per_shader_stage, 13);
    require!(max_samplers_per_shader_stage, 2);
    require!(max_bindings_per_bind_group, 18);
    require!(
        max_storage_buffer_binding_size,
        std::mem::size_of::<AllAdjustments>() as u64
    );
    // Blur uses 256x1 and 1x256; flare uses 16x16.
    require!(max_compute_invocations_per_workgroup, 256);
    require!(max_compute_workgroup_size_x, 256);
    require!(max_compute_workgroup_size_y, 256);
    require!(max_compute_workgroup_size_z, 1);
    if !missing.is_empty() {
        return Err(format!(
            "GPU adapter cannot run the image-processing compute pipeline: {}",
            missing.join("; ")
        ));
    }
    Ok(required)
}

pub fn get_or_init_gpu_context(
    state: &tauri::State<AppState>,
    _app_handle: &tauri::AppHandle,
) -> Result<GpuContext, String> {
    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    let app_handle = _app_handle;

    let mut context_lock = match state.gpu_context.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!(
                "GPU context lock was poisoned (previous crash). Wiping state to self-heal."
            );
            let mut guard = poisoned.into_inner();
            *guard = None;
            guard
        }
    };
    if let Some(context) = &*context_lock {
        return Ok(context.clone());
    }

    #[allow(unused_mut)]
    let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();

    #[cfg(target_os = "windows")]
    if std::env::var("WGPU_BACKEND").is_err() {
        instance_desc.backends = wgpu::Backends::PRIMARY;
    }

    // Android GLES adapters may expose zero storage-resource limits even
    // when they advertise compute. Prefer the native compute backend; retain
    // an explicit WGPU_BACKEND override for diagnostics.
    #[cfg(target_os = "android")]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        instance_desc.backends = wgpu::Backends::VULKAN;
    }

    let flag_path = state.gpu_crash_flag_path.lock().unwrap().clone();
    if let Some(p) = &flag_path {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(p, "initializing_gpu");
    }

    let instance = wgpu::Instance::new(instance_desc);

    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    let surface_opt = {
        let settings = crate::app_settings::load_settings(app_handle.clone()).unwrap_or_default();
        let use_wgpu_renderer = settings.use_wgpu_renderer.unwrap_or(true);

        if use_wgpu_renderer {
            if let Some(window) = app_handle.get_webview_window("main") {
                match instance.create_surface(window) {
                    Ok(surface) => Some(surface),
                    Err(e) => {
                        log::warn!(
                            "Failed to create surface, falling back to compute-only: {}",
                            e
                        );
                        if let Some(p) = &flag_path {
                            let _ = std::fs::remove_file(p);
                        }
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    };

    #[cfg(any(target_os = "android", target_os = "linux"))]
    let surface_opt: Option<wgpu::Surface> = None;

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: surface_opt.as_ref(),
        ..Default::default()
    }))
    .map_err(|e| {
        if let Some(p) = &flag_path {
            let _ = std::fs::remove_file(p);
        }
        format!("Failed to find a wgpu adapter: {}", e)
    })?;

    let mut required_features = wgpu::Features::empty();
    if adapter
        .features()
        .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
    {
        required_features |= wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
    }

    let adapter_info = adapter.get_info();
    let supported_limits = adapter.limits();
    log::info!(
        "GPU processing adapter '{}' backend {:?}: storage textures {}, storage buffers {}, compute invocations {}",
        adapter_info.name,
        adapter_info.backend,
        supported_limits.max_storage_textures_per_shader_stage,
        supported_limits.max_storage_buffers_per_shader_stage,
        supported_limits.max_compute_invocations_per_workgroup,
    );
    let limits = (|| {
        if !adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
        {
            return Err("GPU adapter does not support compute shaders".to_string());
        }
        required_processing_limits(&supported_limits)
    })()
    .map_err(|error| {
        if let Some(p) = &flag_path {
            let _ = std::fs::remove_file(p);
        }
        format!(
            "GPU adapter '{}' ({:?}) rejected: {error}",
            adapter_info.name, adapter_info.backend
        )
    })?;

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Processing Device"),
        required_features,
        required_limits: limits.clone(),
        experimental_features: wgpu::ExperimentalFeatures::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .map_err(|e| {
        if let Some(p) = &flag_path {
            let _ = std::fs::remove_file(p);
        }
        e.to_string()
    })?;

    device.on_uncaptured_error(Arc::new(|err: wgpu::Error| {
        log::error!("[wgpu-error] {}", err);
    }));

    if let Some(p) = &flag_path {
        let _ = std::fs::remove_file(p);
    }

    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    let display_opt = if let Some(surface) = surface_opt {
        let window = app_handle
            .get_webview_window("main")
            .ok_or("Failed to get main window")?;

        let swapchain_caps = surface.get_capabilities(&adapter);
        let swapchain_format = swapchain_caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(swapchain_caps.formats[0]);

        let alpha_mode = if cfg!(target_os = "windows")
            && swapchain_caps
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::Opaque)
        {
            wgpu::CompositeAlphaMode::Opaque
        } else if swapchain_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if swapchain_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else {
            swapchain_caps.alpha_modes[0]
        };

        let size = window
            .inner_size()
            .unwrap_or(tauri::PhysicalSize::new(1280, 720));
        let config = wgpu::SurfaceConfiguration {
            width: size.width.max(1),
            height: size.height.max(1),
            format: swapchain_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Display Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/display.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Display BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    count: None,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    count: None,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    count: None,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Display Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Display Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: swapchain_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: NonZero::new(0),
            cache: None,
        });

        let transform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Transform Buffer"),
            size: std::mem::size_of::<DisplayTransform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Display Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Some(WgpuDisplay {
            surface,
            config,
            pipeline,
            bind_group_layout,
            transform_buffer,
            latest_transform: DisplayTransform {
                rect: [0.0, 0.0, 100.0, 100.0],
                clip: [0.0, 0.0, 10000.0, 10000.0],
                window: [1280.0, 720.0],
                image_size: [100.0, 100.0],
                texture_size: [100.0, 100.0],
                pixelated: 0.0,
                _pad: 0.0,
                bg_primary: [24.0 / 255.0, 24.0 / 255.0, 24.0 / 255.0, 1.0],
                bg_secondary: [35.0 / 255.0, 35.0 / 255.0, 35.0 / 255.0, 1.0],
            },
            sampler,
            current_bind_group: None,
        })
    } else {
        None
    };

    #[cfg(any(target_os = "android", target_os = "linux"))]
    let display_opt = None;

    let new_context = GpuContext {
        device: Arc::new(device),
        queue: Arc::new(queue),
        limits,
        display: Arc::new(std::sync::Mutex::new(display_opt)),
    };
    *context_lock = Some(new_context.clone());
    Ok(new_context)
}

fn read_texture_data_roi(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    origin: wgpu::Origin3d,
    size: wgpu::Extent3d,
    bytes_per_pixel: u32,
) -> Result<Vec<u8>, String> {
    #[cfg(debug_assertions)]
    let _gpu_diag_span = GpuDiagnosticSpan::new("read_texture_data_roi");
    let unpadded_bytes_per_row = bytes_per_pixel * size.width;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = (unpadded_bytes_per_row + align - 1) & !(align - 1);
    let output_buffer_size = (padded_bytes_per_row * size.height) as u64;

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Readback Buffer"),
        size: output_buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(size.height),
            },
        },
        size,
    );

    gpu_diag!("readback/submit", queue.submit(Some(encoder.finish())));
    struct UnmapOnDrop<'a>(&'a wgpu::Buffer);
    impl Drop for UnmapOnDrop<'_> {
        fn drop(&mut self) {
            self.0.unmap();
        }
    }
    let buffer_slice = output_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    gpu_diag!(
        "readback/map_async",
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            #[cfg(debug_assertions)]
            log::info!(
                "[gpu-diag] thread={:?} readback/map_callback result={:?}",
                std::thread::current().id(),
                result
            );
            let _ = tx.send(result);
        })
    );
    gpu_diag!(
        "readback/poll",
        device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(60)),
        })
    )
    .map_err(|e| format!("Failed while polling mapped GPU buffer: {}", e))?;
    let map_result = gpu_diag!("readback/callback_recv", rx.recv())
        .map_err(|e| format!("Failed receiving GPU map result: {}", e))?;
    map_result.map_err(|e| e.to_string())?;
    let _unmap = UnmapOnDrop(&output_buffer);

    let padded_data = buffer_slice.get_mapped_range().to_vec();

    if padded_bytes_per_row == unpadded_bytes_per_row {
        Ok(padded_data)
    } else {
        let mut unpadded_data = Vec::with_capacity((unpadded_bytes_per_row * size.height) as usize);
        for chunk in padded_data.chunks(padded_bytes_per_row as usize) {
            unpadded_data.extend_from_slice(&chunk[..unpadded_bytes_per_row as usize]);
        }
        Ok(unpadded_data)
    }
}

fn to_rgba_f16(img: &DynamicImage) -> Vec<f16> {
    match img {
        DynamicImage::ImageRgb32F(buffer) => {
            let mut output = Vec::with_capacity(buffer.as_raw().len() / 3 * 4);
            for pixel in buffer.pixels() {
                output.extend([
                    f16::from_f32(pixel[0]),
                    f16::from_f32(pixel[1]),
                    f16::from_f32(pixel[2]),
                    f16::ONE,
                ]);
            }
            output
        }
        DynamicImage::ImageRgba32F(buffer) => {
            buffer.as_raw().iter().copied().map(f16::from_f32).collect()
        }
        _ => {
            let rgba_f32 = img.to_rgba32f();
            rgba_f32.into_raw().into_iter().map(f16::from_f32).collect()
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurParams {
    radius: u32,
    tile_offset_x: u32,
    tile_offset_y: u32,
    input_width: u32,
    input_height: u32,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
}

const GF_SHORT_EDGE: u32 = 1080;
const GF_CLARITY_RADIUS: f32 = 8.0;
const GF_STRUCTURE_RADIUS: f32 = 40.0;
const GF_CLARITY_EPS: f32 = 0.25;
const GF_STRUCTURE_EPS: f32 = 0.5;
const GF_COARSE_DIV: u32 = 4;
const GF_TONAL_RADIUS: f32 = 10.0;
const GF_TONAL_EPS: f32 = 0.25;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct GfParams {
    radius: u32,
    is_raw: u32,
    eps: f32,
    _pad: f32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FlareParams {
    amount: f32,
    is_raw: u32,
    exposure: f32,
    brightness: f32,
    contrast: f32,
    whites: f32,
    aspect_ratio: f32,
    _pad: f32,
}

pub struct GpuProcessor {
    context: GpuContext,
    blur_bgl: wgpu::BindGroupLayout,
    h_blur_pipeline: wgpu::ComputePipeline,
    v_blur_pipeline: wgpu::ComputePipeline,
    blur_params_buffer: wgpu::Buffer,

    flare_bgl_0: wgpu::BindGroupLayout,
    flare_bgl_1: wgpu::BindGroupLayout,
    flare_threshold_pipeline: wgpu::ComputePipeline,
    flare_ghosts_pipeline: wgpu::ComputePipeline,
    flare_params_buffer: wgpu::Buffer,
    flare_threshold_view: wgpu::TextureView,
    flare_ghosts_view: wgpu::TextureView,
    flare_final_view: wgpu::TextureView,
    flare_sampler: wgpu::Sampler,

    gf_bgl: wgpu::BindGroupLayout,
    gf_downsample_pipeline: wgpu::ComputePipeline,
    gf_box_down_pipeline: wgpu::ComputePipeline,
    gf_blur_h_pipeline: wgpu::ComputePipeline,
    gf_blur_v_pipeline: wgpu::ComputePipeline,
    gf_coeffs_pipeline: wgpu::ComputePipeline,
    gf_pack_pipeline: wgpu::ComputePipeline,

    main_bgl: wgpu::BindGroupLayout,
    main_pipeline: wgpu::ComputePipeline,
    high_precision_entries: Vec<wgpu::BindGroupLayoutEntry>,
    high_precision_pipeline: std::sync::OnceLock<HighPrecisionPipeline>,
    high_precision_tile: std::sync::OnceLock<HighPrecisionTile>,
    tile_output_size: wgpu::Extent3d,
    adjustments_buffer: wgpu::Buffer,
    dummy_blur_view: wgpu::TextureView,
    dummy_lut_view: wgpu::TextureView,
    dummy_lut_sampler: wgpu::Sampler,
    dcp_dummy_3d_view: wgpu::TextureView,
    dcp_dummy_tc_view: wgpu::TextureView,
    ping_pong_view: wgpu::TextureView,
    sharpness_blur_view: wgpu::TextureView,
    tonal_blur_view: wgpu::TextureView,
    clarity_blur_view: wgpu::TextureView,
    structure_blur_view: wgpu::TextureView,

    pub tile_output_texture: wgpu::Texture,
    pub tile_output_texture_view: wgpu::TextureView,
    pub working_texture: wgpu::Texture,
    pub working_texture_view: wgpu::TextureView,
    pub output_texture: wgpu::Texture,
    pub output_texture_view: wgpu::TextureView,
}

struct HighPrecisionPipeline {
    bgl: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
}

struct HighPrecisionTile {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

enum RenderedPixels {
    U8(Vec<u8>),
    U16(Vec<u16>),
}

pub(crate) fn production_shader_source() -> String {
    format!(
        "{}\n{}",
        include_str!("shaders/dcp.wgsl"),
        include_str!("shaders/shader.wgsl")
    )
}

pub(crate) fn high_precision_shader_source() -> String {
    production_shader_source().replace("rgba8unorm, write>", "rgba16float, write>")
}

const FLARE_MAP_SIZE: u32 = 512;

impl GpuProcessor {
    pub fn new(context: GpuContext, max_width: u32, max_height: u32) -> Result<Self, String> {
        #[cfg(debug_assertions)]
        let _gpu_diag_span = GpuDiagnosticSpan::new("GpuProcessor::new");
        // Also reject contexts created outside get_or_init_gpu_context before
        // constructing invalid layouts/pipelines (which wgpu would only log).
        required_processing_limits(&context.device.limits())?;
        let device = &context.device;
        let queue = &context.queue;
        const MAX_MASK_BINDINGS: u32 = 1;

        let blur_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Blur Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/blur.wgsl").into()),
        });

        let blur_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Blur BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let blur_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Blur Pipeline Layout"),
            bind_group_layouts: &[Some(&blur_bgl)],
            immediate_size: 0,
        });

        let h_blur_pipeline = gpu_diag!(
            "Horizontal Blur Pipeline",
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Horizontal Blur Pipeline"),
                layout: Some(&blur_pipeline_layout),
                module: &blur_shader_module,
                entry_point: Some("horizontal_blur"),
                compilation_options: Default::default(),
                cache: None,
            })
        );

        let v_blur_pipeline = gpu_diag!(
            "Vertical Blur Pipeline",
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Vertical Blur Pipeline"),
                layout: Some(&blur_pipeline_layout),
                module: &blur_shader_module,
                entry_point: Some("vertical_blur"),
                compilation_options: Default::default(),
                cache: None,
            })
        );

        let blur_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Blur Params Buffer"),
            size: std::mem::size_of::<BlurParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let flare_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Flare Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/flare.wgsl").into()),
        });

        let flare_bgl_0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Flare BGL 0"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let flare_bgl_1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Flare BGL 1"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let flare_threshold_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Flare Threshold Layout"),
                bind_group_layouts: &[Some(&flare_bgl_0)],
                immediate_size: 0,
            });

        let flare_ghosts_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Flare Ghosts Layout"),
            bind_group_layouts: &[Some(&flare_bgl_0), Some(&flare_bgl_1)],
            immediate_size: 0,
        });

        let flare_threshold_pipeline = gpu_diag!(
            "Flare Threshold Pipeline",
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Flare Threshold Pipeline"),
                layout: Some(&flare_threshold_layout),
                module: &flare_shader,
                entry_point: Some("threshold_main"),
                compilation_options: Default::default(),
                cache: None,
            })
        );

        let flare_ghosts_pipeline = gpu_diag!(
            "Flare Ghosts Pipeline",
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Flare Ghosts Pipeline"),
                layout: Some(&flare_ghosts_layout),
                module: &flare_shader,
                entry_point: Some("ghosts_main"),
                compilation_options: Default::default(),
                cache: None,
            })
        );

        let flare_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Flare Params Buffer"),
            size: std::mem::size_of::<FlareParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let flare_tex_desc = wgpu::TextureDescriptor {
            label: Some("Flare Tex"),
            size: wgpu::Extent3d {
                width: FLARE_MAP_SIZE,
                height: FLARE_MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        };

        let flare_threshold_texture = device.create_texture(&flare_tex_desc);
        let flare_threshold_view = flare_threshold_texture.create_view(&Default::default());
        let flare_ghosts_texture = device.create_texture(&flare_tex_desc);
        let flare_ghosts_view = flare_ghosts_texture.create_view(&Default::default());
        let flare_final_texture = device.create_texture(&flare_tex_desc);
        let flare_final_view = flare_final_texture.create_view(&Default::default());

        let flare_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Flare Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let gf_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Guided Filter Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/guided.wgsl").into()),
        });

        let gf_texture_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };

        let gf_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Guided Filter BGL"),
            entries: &[
                gf_texture_entry(0),
                gf_texture_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let gf_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Guided Filter Pipeline Layout"),
            bind_group_layouts: &[Some(&gf_bgl)],
            immediate_size: 0,
        });

        let gf_pipeline = |entry: &'static str| {
            gpu_diag!(
                entry,
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: Some(&gf_layout),
                    module: &gf_shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                })
            )
        };

        let gf_downsample_pipeline = gf_pipeline("gf_downsample");
        let gf_box_down_pipeline = gf_pipeline("gf_box_down");
        let gf_blur_h_pipeline = gf_pipeline("gf_blur_h");
        let gf_blur_v_pipeline = gf_pipeline("gf_blur_v");
        let gf_coeffs_pipeline = gf_pipeline("gf_coeffs");
        let gf_pack_pipeline = gf_pipeline("gf_pack");

        let shader_src = production_shader_source();
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Image Processing Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        let mut bind_group_layout_entries = vec![
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];

        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        });

        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 3 + MAX_MASK_BINDINGS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D3,
                multisampled: false,
            },
            count: None,
        });
        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 4 + MAX_MASK_BINDINGS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
            count: None,
        });

        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 5 + MAX_MASK_BINDINGS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 6 + MAX_MASK_BINDINGS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 7 + MAX_MASK_BINDINGS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 8 + MAX_MASK_BINDINGS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });

        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 9 + MAX_MASK_BINDINGS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 10 + MAX_MASK_BINDINGS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });

        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 11 + MAX_MASK_BINDINGS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });

        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 12 + MAX_MASK_BINDINGS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });

        // DCP profile textures (W3) — bindings 14-16 (12-13 are upstream guided filters)
        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 14,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D3,
                multisampled: false,
            },
            count: None,
        });
        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 15,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D3,
                multisampled: false,
            },
            count: None,
        });
        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 16,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D1,
                multisampled: false,
            },
            count: None,
        });

        bind_group_layout_entries.push(wgpu::BindGroupLayoutEntry {
            binding: 17,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: wgpu::TextureFormat::Rgba32Float,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        });
        let main_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Main BGL"),
            entries: &bind_group_layout_entries,
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            bind_group_layouts: &[Some(&main_bgl)],
            immediate_size: 0,
        });

        let main_pipeline = gpu_diag!(
            "Compute Pipeline",
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Compute Pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            })
        );

        bind_group_layout_entries[1].ty = wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba16Float,
            view_dimension: wgpu::TextureViewDimension::D2,
        };
        let adjustments_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Adjustments Buffer"),
            size: std::mem::size_of::<AllAdjustments>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dummy_texture_desc = wgpu::TextureDescriptor {
            label: Some("Dummy Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let dummy_blur_texture = device.create_texture(&dummy_texture_desc);
        let dummy_blur_view = dummy_blur_texture.create_view(&Default::default());

        let dummy_lut_texture = device.create_texture(&wgpu::TextureDescriptor {
            dimension: wgpu::TextureDimension::D3,
            ..dummy_texture_desc
        });
        let dummy_lut_view = dummy_lut_texture.create_view(&Default::default());
        let dummy_lut_sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());

        // DCP dummy textures (W3) — bound when no profile is active.
        // The shader early-outs (has_dcp == 0) so their content is never sampled.
        let dcp_dummy_3d = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("DCP Dummy 3D"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let dcp_dummy_3d_view = dcp_dummy_3d.create_view(&Default::default());
        // Identity tone curve LUT (4096 entries, y=x).
        let tc_data: Vec<f32> = (0..4096u32).map(|i| i as f32 / 4095.0).collect();
        let dcp_dummy_tc = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("DCP Dummy Tone Curve"),
                size: wgpu::Extent3d {
                    width: 4096,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D1,
                format: wgpu::TextureFormat::R32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            TextureDataOrder::MipMajor,
            bytemuck::cast_slice(&tc_data),
        );
        let dcp_dummy_tc_view = dcp_dummy_tc.create_view(&Default::default());

        const TILE_SIZE: u32 = 2048;
        const TILE_OVERLAP: u32 = 128;

        let clamped_tile_width = max_width.min(TILE_SIZE + TILE_OVERLAP * 2);
        let clamped_tile_height = max_height.min(TILE_SIZE + TILE_OVERLAP * 2);

        let clamped_tile_size = wgpu::Extent3d {
            width: clamped_tile_width,
            height: clamped_tile_height,
            depth_or_array_layers: 1,
        };

        let full_image_size = wgpu::Extent3d {
            width: max_width,
            height: max_height,
            depth_or_array_layers: 1,
        };

        let reusable_texture_desc = wgpu::TextureDescriptor {
            label: None,
            size: clamped_tile_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        };

        let ping_pong_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Ping Pong Texture"),
            ..reusable_texture_desc
        });
        let ping_pong_view = ping_pong_texture.create_view(&Default::default());

        let sharpness_blur_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Sharpness Blur Texture"),
            ..reusable_texture_desc
        });
        let sharpness_blur_view = sharpness_blur_texture.create_view(&Default::default());

        let tonal_blur_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Tonal Blur Texture"),
            ..reusable_texture_desc
        });
        let tonal_blur_view = tonal_blur_texture.create_view(&Default::default());

        let clarity_blur_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Clarity Blur Texture"),
            ..reusable_texture_desc
        });
        let clarity_blur_view = clarity_blur_texture.create_view(&Default::default());

        let structure_blur_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Structure Blur Texture"),
            ..reusable_texture_desc
        });
        let structure_blur_view = structure_blur_texture.create_view(&Default::default());

        let tile_output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Tile Output Texture"),
            size: clamped_tile_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let tile_output_texture_view = tile_output_texture.create_view(&Default::default());

        let working_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Working Output Texture"),
            size: full_image_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let working_texture_view = working_texture.create_view(&Default::default());

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Full Output Texture"),
            size: full_image_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_texture_view = output_texture.create_view(&Default::default());

        Ok(Self {
            context,
            blur_bgl,
            h_blur_pipeline,
            v_blur_pipeline,
            blur_params_buffer,
            flare_bgl_0,
            flare_bgl_1,
            flare_threshold_pipeline,
            flare_ghosts_pipeline,
            flare_params_buffer,
            flare_threshold_view,
            flare_ghosts_view,
            flare_final_view,
            flare_sampler,
            gf_bgl,
            gf_downsample_pipeline,
            gf_box_down_pipeline,
            gf_blur_h_pipeline,
            gf_blur_v_pipeline,
            gf_coeffs_pipeline,
            gf_pack_pipeline,
            main_bgl,
            main_pipeline,
            high_precision_entries: bind_group_layout_entries,
            high_precision_pipeline: std::sync::OnceLock::new(),
            high_precision_tile: std::sync::OnceLock::new(),
            tile_output_size: clamped_tile_size,
            adjustments_buffer,
            dummy_blur_view,
            dummy_lut_view,
            dummy_lut_sampler,
            dcp_dummy_3d_view,
            dcp_dummy_tc_view,
            ping_pong_view,
            sharpness_blur_view,
            tonal_blur_view,
            clarity_blur_view,
            structure_blur_view,
            tile_output_texture,
            tile_output_texture_view,
            working_texture,
            working_texture_view,
            output_texture,
            output_texture_view,
        })
    }

    pub fn build_guided_coeffs(
        &self,
        input_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        is_raw: u32,
    ) -> (wgpu::TextureView, wgpu::TextureView) {
        #[cfg(debug_assertions)]
        let _gpu_diag_span = GpuDiagnosticSpan::new("build_guided_coeffs");
        let device = &self.context.device;
        let queue = &self.context.queue;

        let short = width.min(height).max(1);
        let k = (GF_SHORT_EDGE as f32 / short as f32).min(1.0);
        let lw = ((width as f32 * k).round() as u32).max(1);
        let lh = ((height as f32 * k).round() as u32).max(1);
        let low_short = lw.min(lh) as f32;
        let cw = lw.div_ceil(GF_COARSE_DIV).max(1);
        let ch = lh.div_ceil(GF_COARSE_DIV).max(1);

        let make_view = |label: &'static str, w: u32, h: u32| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba32Float,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::STORAGE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };

        let make_params = |base_radius: f32, eps: f32, div: f32| {
            let radius = (base_radius * low_short / 1080.0 / div).ceil().max(1.0) as u32;
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Guided Filter Params"),
                contents: bytemuck::bytes_of(&GfParams {
                    radius,
                    is_raw,
                    eps,
                    _pad: 0.0,
                }),
                usage: wgpu::BufferUsages::UNIFORM,
            })
        };

        let moments = make_view("GF Moments", lw, lh);
        let tmp = make_view("GF Ping Pong", lw, lh);
        let blurred = make_view("GF Blurred", lw, lh);
        let ab = make_view("GF Coeffs", lw, lh);
        let ab_clarity = make_view("GF Coeffs Clarity", lw, lh);
        let c_moments = make_view("GF Coarse Moments", cw, ch);
        let c_tmp = make_view("GF Coarse Ping Pong", cw, ch);
        let c_blurred = make_view("GF Coarse Blurred", cw, ch);
        let c_ab = make_view("GF Coarse Coeffs", cw, ch);

        let params_clarity = make_params(GF_CLARITY_RADIUS, GF_CLARITY_EPS, 1.0);
        let params_tonal = make_params(GF_TONAL_RADIUS, GF_TONAL_EPS, 1.0);
        let params_coarse =
            make_params(GF_STRUCTURE_RADIUS, GF_STRUCTURE_EPS, GF_COARSE_DIV as f32);

        let mut encoder = device.create_command_encoder(&Default::default());

        let mut pass = |pipeline: &wgpu::ComputePipeline,
                        src_a: &wgpu::TextureView,
                        src_b: &wgpu::TextureView,
                        dst: &wgpu::TextureView,
                        params: &wgpu::Buffer,
                        (w, h): (u32, u32)| {
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Guided Filter BG"),
                layout: &self.gf_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src_a),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(src_b),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(dst),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: params.as_entire_binding(),
                    },
                ],
            });
            let mut cpass = encoder.begin_compute_pass(&Default::default());
            cpass.set_pipeline(pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        };
        let fine = (lw, lh);
        let coarse = (cw, ch);

        pass(
            &self.gf_downsample_pipeline,
            input_view,
            input_view,
            &moments,
            &params_clarity,
            fine,
        );

        pass(
            &self.gf_box_down_pipeline,
            &moments,
            &moments,
            &c_moments,
            &params_coarse,
            coarse,
        );
        pass(
            &self.gf_blur_h_pipeline,
            &c_moments,
            &c_moments,
            &c_tmp,
            &params_coarse,
            coarse,
        );
        pass(
            &self.gf_blur_v_pipeline,
            &c_tmp,
            &c_tmp,
            &c_blurred,
            &params_coarse,
            coarse,
        );
        pass(
            &self.gf_coeffs_pipeline,
            &c_blurred,
            &c_blurred,
            &c_ab,
            &params_coarse,
            coarse,
        );
        pass(
            &self.gf_blur_h_pipeline,
            &c_ab,
            &c_ab,
            &c_tmp,
            &params_coarse,
            coarse,
        );
        pass(
            &self.gf_blur_v_pipeline,
            &c_tmp,
            &c_tmp,
            &c_moments,
            &params_coarse,
            coarse,
        );

        for (params, out) in [(&params_clarity, &ab_clarity), (&params_tonal, &blurred)] {
            pass(
                &self.gf_blur_h_pipeline,
                &moments,
                &moments,
                &tmp,
                params,
                fine,
            );
            pass(&self.gf_blur_v_pipeline, &tmp, &tmp, &blurred, params, fine);
            pass(
                &self.gf_coeffs_pipeline,
                &blurred,
                &blurred,
                &ab,
                params,
                fine,
            );
            pass(&self.gf_blur_h_pipeline, &ab, &ab, &tmp, params, fine);
            pass(&self.gf_blur_v_pipeline, &tmp, &tmp, out, params, fine);
        }

        pass(
            &self.gf_pack_pipeline,
            &ab_clarity,
            &blurred,
            &ab,
            &params_clarity,
            fine,
        );

        gpu_diag!("guided/submit", queue.submit(Some(encoder.finish())));
        (ab, c_moments)
    }

    fn high_precision_pipeline(&self) -> &HighPrecisionPipeline {
        self.high_precision_pipeline.get_or_init(|| {
            let device = &self.context.device;
            let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("High Precision Main BGL"),
                entries: &self.high_precision_entries,
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("High Precision Pipeline Layout"),
                bind_group_layouts: &[Some(&bgl)],
                immediate_size: 0,
            });
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("High Precision Image Processing Shader"),
                source: wgpu::ShaderSource::Wgsl(high_precision_shader_source().into()),
            });
            let pipeline = gpu_diag!(
                "High Precision Compute Pipeline",
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("High Precision Compute Pipeline"),
                    layout: Some(&layout),
                    module: &shader,
                    entry_point: Some("main"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[("HIGH_PRECISION_OUTPUT", 1.0)],
                        ..Default::default()
                    },
                    cache: None,
                })
            );
            HighPrecisionPipeline { bgl, pipeline }
        })
    }

    fn high_precision_tile(&self) -> &HighPrecisionTile {
        self.high_precision_tile.get_or_init(|| {
            let texture = self
                .context
                .device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("High Precision Tile Output Texture"),
                    size: self.tile_output_size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
            let view = texture.create_view(&Default::default());
            HighPrecisionTile { texture, view }
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        &self,
        input_texture_view: &wgpu::TextureView,
        gf_coeffs_view: &wgpu::TextureView,
        gf_dehaze_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        request: RenderRequest,
        output_to_display: bool,
        output_precision: RenderOutputPrecision,
    ) -> Result<(RenderedPixels, u32, u32, u32, u32), String> {
        self.run_with_point_sample(
            input_texture_view,
            gf_coeffs_view,
            gf_dehaze_view,
            width,
            height,
            request,
            output_to_display,
            output_precision,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn run_with_point_sample(
        &self,
        input_texture_view: &wgpu::TextureView,
        gf_coeffs_view: &wgpu::TextureView,
        gf_dehaze_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        request: RenderRequest,
        output_to_display: bool,
        output_precision: RenderOutputPrecision,
        sample_out: Option<&mut Option<crate::image_processing::PointColourSample>>,
    ) -> Result<(RenderedPixels, u32, u32, u32, u32), String> {
        #[cfg(debug_assertions)]
        let _gpu_diag_span = GpuDiagnosticSpan::new("GpuProcessor::run");
        let device = &self.context.device;
        let queue = &self.context.queue;
        request.adjustments.global.point_colour.validate()?;
        let sampling = sample_out.is_some();
        let probe = request.adjustments.global.point_probe;
        if sampling
            && (probe[2] != 1 || probe[0] >= width || probe[1] >= height || output_to_display)
        {
            return Err("Invalid Point Colour sample target".into());
        }
        if !sampling && probe != [0; 4] {
            return Err("Point Colour probe is private to sampling".into());
        }
        let sample_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("One Point Colour pixel: linear RGB and HSV"),
            size: wgpu::Extent3d {
                width: 2,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let sample_view = sample_texture.create_view(&Default::default());
        let scale = (width.min(height) as f32) / 1080.0;
        const MAX_MASK_BINDINGS: u32 = 1;

        if output_precision == RenderOutputPrecision::SixteenBit && output_to_display {
            return Err(
                "High-precision GPU output is only supported for CPU-readback renders.".to_string(),
            );
        }

        let high_precision_tile = if output_precision == RenderOutputPrecision::SixteenBit {
            Some(self.high_precision_tile())
        } else {
            None
        };
        let (output_pipeline, output_bgl, output_tile_texture, output_tile_view, bytes_per_pixel) =
            if let Some(high_precision_tile) = high_precision_tile {
                let hp = self.high_precision_pipeline();
                (
                    &hp.pipeline,
                    &hp.bgl,
                    &high_precision_tile.texture,
                    &high_precision_tile.view,
                    8,
                )
            } else {
                (
                    &self.main_pipeline,
                    &self.main_bgl,
                    &self.tile_output_texture,
                    &self.tile_output_texture_view,
                    4,
                )
            };

        let bounds = request.roi.unwrap_or(Roi {
            x: 0,
            y: 0,
            width,
            height,
        });
        let out_width = bounds.width;
        let out_height = bounds.height;
        let mask_layer_count = request.mask_bitmaps.len().clamp(2, MAX_MASKS) as u32;
        let full_texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: mask_layer_count,
        };
        let buffer_size = (width as usize) * (height as usize) * (mask_layer_count as usize);
        let mut mask_texture_data = Vec::with_capacity(buffer_size);
        if request.mask_bitmaps.is_empty() {
            mask_texture_data.resize(buffer_size, 0);
        } else {
            for mask_bitmap in request.mask_bitmaps.iter().take(MAX_MASKS) {
                mask_texture_data.extend_from_slice(mask_bitmap.as_raw());
            }
            if mask_texture_data.len() < buffer_size {
                mask_texture_data.resize(buffer_size, 0);
            }
        }
        let mask_texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("Full Mask Texture Array"),
                size: full_texture_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            TextureDataOrder::MipMajor,
            &mask_texture_data,
        );
        let mask_texture_view = mask_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let (lut_texture_view, lut_sampler) = if let Some(lut_arc) = &request.lut {
            let lut_data = &lut_arc.data;
            let size = lut_arc.size;
            let mut rgba_lut_data_f16 = Vec::with_capacity(lut_data.len() / 3 * 4);
            for chunk in lut_data.as_chunks::<3>().0 {
                rgba_lut_data_f16.push(f16::from_f32(chunk[0]));
                rgba_lut_data_f16.push(f16::from_f32(chunk[1]));
                rgba_lut_data_f16.push(f16::from_f32(chunk[2]));
                rgba_lut_data_f16.push(f16::ONE);
            }
            let lut_texture = device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("LUT 3D Texture"),
                    size: wgpu::Extent3d {
                        width: size,
                        height: size,
                        depth_or_array_layers: size,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D3,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                TextureDataOrder::MipMajor,
                bytemuck::cast_slice(&rgba_lut_data_f16),
            );
            let view = lut_texture.create_view(&Default::default());
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });
            (view, sampler)
        } else {
            (self.dummy_lut_view.clone(), self.dummy_lut_sampler.clone())
        };

        let adjustments = request.adjustments;
        if adjustments.global.flare_amount > 0.0 {
            let mut encoder = device.create_command_encoder(&Default::default());

            let aspect_ratio = if height > 0 {
                width as f32 / height as f32
            } else {
                1.0
            };
            let f_params = FlareParams {
                amount: adjustments.global.flare_amount,
                is_raw: adjustments.global.is_raw_image,
                exposure: adjustments.global.exposure,
                brightness: adjustments.global.brightness,
                contrast: adjustments.global.contrast,
                whites: adjustments.global.whites,
                aspect_ratio,
                _pad: 0.0,
            };
            queue.write_buffer(&self.flare_params_buffer, 0, bytemuck::bytes_of(&f_params));

            let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Flare BG0"),
                layout: &self.flare_bgl_0,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(input_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.flare_threshold_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.flare_params_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.flare_sampler),
                    },
                ],
            });

            {
                let mut cpass = encoder.begin_compute_pass(&Default::default());
                cpass.set_pipeline(&self.flare_threshold_pipeline);
                cpass.set_bind_group(0, &bg0, &[]);
                cpass.dispatch_workgroups(FLARE_MAP_SIZE / 16, FLARE_MAP_SIZE / 16, 1);
            }

            let bg0_ghosts = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Flare BG0 Ghosts"),
                layout: &self.flare_bgl_0,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(input_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.flare_final_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.flare_params_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.flare_sampler),
                    },
                ],
            });

            let bg1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Flare BG1"),
                layout: &self.flare_bgl_1,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.flare_threshold_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.flare_ghosts_view),
                    },
                ],
            });

            {
                let mut cpass = encoder.begin_compute_pass(&Default::default());
                cpass.set_pipeline(&self.flare_ghosts_pipeline);
                cpass.set_bind_group(0, &bg0_ghosts, &[]);
                cpass.set_bind_group(1, &bg1, &[]);
                cpass.dispatch_workgroups(FLARE_MAP_SIZE / 16, FLARE_MAP_SIZE / 16, 1);
            }

            gpu_diag!("run/flare_submit", queue.submit(Some(encoder.finish())));
        }

        const TILE_SIZE: u32 = 2048;
        const TILE_OVERLAP: u32 = 128;

        let output_len = if output_to_display || sampling {
            0
        } else {
            (out_width * out_height * 4) as usize
        };
        let mut final_pixels = match output_precision {
            RenderOutputPrecision::EightBit => RenderedPixels::U8(vec![0u8; output_len]),
            RenderOutputPrecision::SixteenBit => RenderedPixels::U16(vec![0u16; output_len]),
        };

        let start_tile_x = bounds.x / TILE_SIZE;
        let start_tile_y = bounds.y / TILE_SIZE;
        let end_tile_x = (bounds.x + bounds.width).div_ceil(TILE_SIZE);
        let end_tile_y = (bounds.y + bounds.height).div_ceil(TILE_SIZE);

        for tile_y in start_tile_y..end_tile_y {
            for tile_x in start_tile_x..end_tile_x {
                let x_start_unclamped = tile_x * TILE_SIZE;
                let y_start_unclamped = tile_y * TILE_SIZE;

                let x_start = x_start_unclamped.max(bounds.x);
                let y_start = y_start_unclamped.max(bounds.y);
                let x_end = (x_start_unclamped + TILE_SIZE)
                    .min(bounds.x + bounds.width)
                    .min(width);
                let y_end = (y_start_unclamped + TILE_SIZE)
                    .min(bounds.y + bounds.height)
                    .min(height);

                let tile_width = x_end - x_start;
                let tile_height = y_end - y_start;

                let input_x_start = (x_start as i32 - TILE_OVERLAP as i32).max(0) as u32;
                let input_y_start = (y_start as i32 - TILE_OVERLAP as i32).max(0) as u32;
                let input_x_end = (x_end + TILE_OVERLAP).min(width);
                let input_y_end = (y_end + TILE_OVERLAP).min(height);
                let input_width = input_x_end - input_x_start;
                let input_height = input_y_end - input_y_start;

                let input_texture_size = wgpu::Extent3d {
                    width: input_width,
                    height: input_height,
                    depth_or_array_layers: 1,
                };

                let run_blur = |base_radius: f32, output_view: &wgpu::TextureView| -> bool {
                    let radius = (base_radius * scale).ceil().max(1.0) as u32;
                    if radius == 0 {
                        return false;
                    }

                    let params = BlurParams {
                        radius,
                        tile_offset_x: input_x_start,
                        tile_offset_y: input_y_start,
                        input_width,
                        input_height,
                        _pad1: 0,
                        _pad2: 0,
                        _pad3: 0,
                    };
                    queue.write_buffer(&self.blur_params_buffer, 0, bytemuck::bytes_of(&params));

                    let mut blur_encoder = device.create_command_encoder(&Default::default());

                    let h_blur_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("H-Blur BG"),
                        layout: &self.blur_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(input_texture_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&self.ping_pong_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: self.blur_params_buffer.as_entire_binding(),
                            },
                        ],
                    });

                    {
                        let mut cpass = blur_encoder.begin_compute_pass(&Default::default());
                        cpass.set_pipeline(&self.h_blur_pipeline);
                        cpass.set_bind_group(0, &h_blur_bg, &[]);
                        cpass.dispatch_workgroups(input_width.div_ceil(256), input_height, 1);
                    }

                    let v_blur_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("V-Blur BG"),
                        layout: &self.blur_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&self.ping_pong_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(output_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: self.blur_params_buffer.as_entire_binding(),
                            },
                        ],
                    });

                    {
                        let mut cpass = blur_encoder.begin_compute_pass(&Default::default());
                        cpass.set_pipeline(&self.v_blur_pipeline);
                        cpass.set_bind_group(0, &v_blur_bg, &[]);
                        cpass.dispatch_workgroups(input_width, input_height.div_ceil(256), 1);
                    }

                    gpu_diag!("run/blur_submit", queue.submit(Some(blur_encoder.finish())));
                    true
                };

                let active_masks = adjustments
                    .mask_adjustments
                    .iter()
                    .take(adjustments.mask_count as usize);
                let any_term =
                    |global: f32, pick: &dyn Fn(&MaskAdjustments) -> f32, positive: bool| {
                        let hit = |v: f32| if positive { v > 0.0 } else { v != 0.0 };
                        hit(global) || active_masks.clone().any(|m| hit(pick(m)))
                    };
                let any_neg_dehaze =
                    adjustments.global.dehaze < 0.0 || active_masks.clone().any(|m| m.dehaze < 0.0);
                let need_sharpen = any_term(adjustments.global.sharpness, &|m| m.sharpness, false);
                let need_halation = any_term(
                    adjustments.global.halation_amount,
                    &|m| m.halation_amount,
                    true,
                );
                let need_structure =
                    any_term(adjustments.global.glow_amount, &|m| m.glow_amount, true)
                        || any_neg_dehaze;

                let did_create_sharpness_blur =
                    need_sharpen && run_blur(1.0, &self.sharpness_blur_view);
                let did_create_tonal_blur = need_sharpen && run_blur(3.5, &self.tonal_blur_view);
                let did_create_clarity_blur =
                    need_halation && run_blur(8.0, &self.clarity_blur_view);
                let did_create_structure_blur =
                    need_structure && run_blur(40.0, &self.structure_blur_view);

                let mut main_encoder = device.create_command_encoder(&Default::default());

                let mut tile_adjustments = adjustments;
                tile_adjustments.tile_offset_x = input_x_start;
                tile_adjustments.tile_offset_y = input_y_start;
                queue.write_buffer(
                    &self.adjustments_buffer,
                    0,
                    bytemuck::bytes_of(&tile_adjustments),
                );

                let mut bind_group_entries = vec![
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(input_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(output_tile_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.adjustments_buffer.as_entire_binding(),
                    },
                ];
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&mask_texture_view),
                });
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 3 + MAX_MASK_BINDINGS,
                    resource: wgpu::BindingResource::TextureView(&lut_texture_view),
                });
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 4 + MAX_MASK_BINDINGS,
                    resource: wgpu::BindingResource::Sampler(&lut_sampler),
                });

                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 5 + MAX_MASK_BINDINGS,
                    resource: wgpu::BindingResource::TextureView(if did_create_sharpness_blur {
                        &self.sharpness_blur_view
                    } else {
                        &self.dummy_blur_view
                    }),
                });
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 6 + MAX_MASK_BINDINGS,
                    resource: wgpu::BindingResource::TextureView(if did_create_tonal_blur {
                        &self.tonal_blur_view
                    } else {
                        &self.dummy_blur_view
                    }),
                });
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 7 + MAX_MASK_BINDINGS,
                    resource: wgpu::BindingResource::TextureView(if did_create_clarity_blur {
                        &self.clarity_blur_view
                    } else {
                        &self.dummy_blur_view
                    }),
                });
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 8 + MAX_MASK_BINDINGS,
                    resource: wgpu::BindingResource::TextureView(if did_create_structure_blur {
                        &self.structure_blur_view
                    } else {
                        &self.dummy_blur_view
                    }),
                });

                let use_flare = adjustments.global.flare_amount > 0.0;
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 9 + MAX_MASK_BINDINGS,
                    resource: wgpu::BindingResource::TextureView(if use_flare {
                        &self.flare_ghosts_view
                    } else {
                        &self.dummy_blur_view
                    }),
                });
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 10 + MAX_MASK_BINDINGS,
                    resource: wgpu::BindingResource::Sampler(&self.flare_sampler),
                });
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 11 + MAX_MASK_BINDINGS,
                    resource: wgpu::BindingResource::TextureView(gf_coeffs_view),
                });
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 12 + MAX_MASK_BINDINGS,
                    resource: wgpu::BindingResource::TextureView(gf_dehaze_view),
                });

                // DCP profile textures (W4) — real data when a profile is active,
                // dummy textures otherwise so the WGSL bindings are always present.
                let (dcp_huesat_view, dcp_look_view, dcp_tc_view, _dcp_tex_guard) =
                    if adjustments.global.has_dcp != 0
                        && let Some(ref dcp) = request.dcp_textures
                    {
                        // Upload real DCP tables as 3-D / 1-D textures.
                        let huesat_size = wgpu::Extent3d {
                            width: dcp.huesat_dims[0],
                            height: dcp.huesat_dims[1],
                            depth_or_array_layers: dcp.huesat_dims[2],
                        };
                        let look_size = wgpu::Extent3d {
                            width: dcp.look_dims[0],
                            height: dcp.look_dims[1],
                            depth_or_array_layers: dcp.look_dims[2],
                        };
                        let huesat_tex = device.create_texture_with_data(
                            queue,
                            &wgpu::TextureDescriptor {
                                label: Some("DCP HueSatMap"),
                                size: huesat_size,
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D3,
                                format: wgpu::TextureFormat::Rgba32Float,
                                usage: wgpu::TextureUsages::TEXTURE_BINDING
                                    | wgpu::TextureUsages::COPY_DST,
                                view_formats: &[],
                            },
                            TextureDataOrder::MipMajor,
                            bytemuck::cast_slice(&dcp.huesat),
                        );
                        let look_tex = device.create_texture_with_data(
                            queue,
                            &wgpu::TextureDescriptor {
                                label: Some("DCP LookTable"),
                                size: look_size,
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D3,
                                format: wgpu::TextureFormat::Rgba32Float,
                                usage: wgpu::TextureUsages::TEXTURE_BINDING
                                    | wgpu::TextureUsages::COPY_DST,
                                view_formats: &[],
                            },
                            TextureDataOrder::MipMajor,
                            bytemuck::cast_slice(&dcp.look),
                        );
                        let tc_tex = device.create_texture_with_data(
                            queue,
                            &wgpu::TextureDescriptor {
                                label: Some("DCP ToneCurve"),
                                size: wgpu::Extent3d {
                                    width: dcp.tone_curve.len() as u32,
                                    height: 1,
                                    depth_or_array_layers: 1,
                                },
                                mip_level_count: 1,
                                sample_count: 1,
                                dimension: wgpu::TextureDimension::D1,
                                format: wgpu::TextureFormat::R32Float,
                                usage: wgpu::TextureUsages::TEXTURE_BINDING
                                    | wgpu::TextureUsages::COPY_DST,
                                view_formats: &[],
                            },
                            TextureDataOrder::MipMajor,
                            bytemuck::cast_slice(&dcp.tone_curve),
                        );
                        let guard = (huesat_tex, look_tex, tc_tex);
                        (
                            guard.0.create_view(&Default::default()),
                            guard.1.create_view(&Default::default()),
                            guard.2.create_view(&Default::default()),
                            Some(guard),
                        )
                    } else {
                        (
                            self.dcp_dummy_3d_view.clone(),
                            self.dcp_dummy_3d_view.clone(),
                            self.dcp_dummy_tc_view.clone(),
                            None,
                        )
                    };

                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 14,
                    resource: wgpu::BindingResource::TextureView(&dcp_huesat_view),
                });
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 15,
                    resource: wgpu::BindingResource::TextureView(&dcp_look_view),
                });
                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 16,
                    resource: wgpu::BindingResource::TextureView(&dcp_tc_view),
                });

                bind_group_entries.push(wgpu::BindGroupEntry {
                    binding: 17,
                    resource: wgpu::BindingResource::TextureView(&sample_view),
                });
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Tile Bind Group"),
                    layout: output_bgl,
                    entries: &bind_group_entries,
                });

                {
                    let mut compute_pass = main_encoder.begin_compute_pass(&Default::default());
                    compute_pass.set_pipeline(output_pipeline);
                    compute_pass.set_bind_group(0, &bind_group, &[]);
                    compute_pass.dispatch_workgroups(
                        input_width.div_ceil(8),
                        input_height.div_ceil(8),
                        1,
                    );
                }

                let crop_x_start = x_start - input_x_start;
                let crop_y_start = y_start - input_y_start;

                if output_to_display {
                    main_encoder.copy_texture_to_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &self.tile_output_texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d {
                                x: crop_x_start,
                                y: crop_y_start,
                                z: 0,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::TexelCopyTextureInfo {
                            texture: &self.working_texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d {
                                x: x_start,
                                y: y_start,
                                z: 0,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::Extent3d {
                            width: tile_width,
                            height: tile_height,
                            depth_or_array_layers: 1,
                        },
                    );
                }

                gpu_diag!("run/main_submit", queue.submit(Some(main_encoder.finish())));

                if !output_to_display && !sampling {
                    let processed_tile_data = read_texture_data_roi(
                        device,
                        queue,
                        output_tile_texture,
                        wgpu::Origin3d::ZERO,
                        input_texture_size,
                        bytes_per_pixel,
                    )?;

                    match &mut final_pixels {
                        RenderedPixels::U8(final_pixels) => {
                            for row in 0..tile_height {
                                let final_y = y_start + row - bounds.y;
                                let final_x = x_start - bounds.x;
                                let final_row_offset = (final_y * out_width + final_x) as usize * 4;
                                let source_y = crop_y_start + row;
                                let source_row_offset =
                                    (source_y * input_width + crop_x_start) as usize * 4;
                                let copy_bytes = (tile_width * 4) as usize;

                                final_pixels[final_row_offset..final_row_offset + copy_bytes]
                                    .copy_from_slice(
                                        &processed_tile_data
                                            [source_row_offset..source_row_offset + copy_bytes],
                                    );
                            }
                        }
                        RenderedPixels::U16(final_pixels) => {
                            for row in 0..tile_height {
                                let final_y = y_start + row - bounds.y;
                                let final_x = x_start - bounds.x;
                                let final_row_offset = (final_y * out_width + final_x) as usize * 4;
                                let source_y = crop_y_start + row;
                                let source_pixel_offset =
                                    (source_y * input_width + crop_x_start) as usize;

                                for sample in 0..(tile_width as usize * 4) {
                                    let source_byte_offset = (source_pixel_offset * 4 + sample) * 2;
                                    let bits = u16::from_ne_bytes([
                                        processed_tile_data[source_byte_offset],
                                        processed_tile_data[source_byte_offset + 1],
                                    ]);
                                    let value = f16::from_bits(bits).to_f32();
                                    final_pixels[final_row_offset + sample] = if value.is_finite() {
                                        (value.clamp(0.0, 1.0) * u16::MAX as f32).round() as u16
                                    } else {
                                        0
                                    };
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(destination) = sample_out {
            let bytes = read_texture_data_roi(
                device,
                queue,
                &sample_texture,
                wgpu::Origin3d::ZERO,
                wgpu::Extent3d {
                    width: 2,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                16,
            )?;
            *destination = Some(decode_point_colour_sample(&bytes)?);
        }
        Ok((final_pixels, out_width, out_height, bounds.x, bounds.y))
    }
}

pub fn process_and_get_dynamic_image(
    context: &GpuContext,
    state: &tauri::State<AppState>,
    base_image: &DynamicImage,
    transform_hash: u64,
    request: RenderRequest,
    caller_id: &str,
) -> Result<DynamicImage, String> {
    process_and_get_dynamic_image_inner(
        context,
        state,
        base_image,
        transform_hash,
        request,
        caller_id,
        RenderOutputPrecision::EightBit,
        false,
        None,
    )
}

pub fn process_and_get_dynamic_image_with_precision(
    context: &GpuContext,
    state: &tauri::State<AppState>,
    base_image: &DynamicImage,
    transform_hash: u64,
    request: RenderRequest,
    caller_id: &str,
    output_precision: RenderOutputPrecision,
) -> Result<DynamicImage, String> {
    process_and_get_dynamic_image_inner(
        context,
        state,
        base_image,
        transform_hash,
        request,
        caller_id,
        output_precision,
        false,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn process_and_get_dynamic_image_with_analytics(
    context: &GpuContext,
    state: &tauri::State<AppState>,
    base_image: &DynamicImage,
    transform_hash: u64,
    request: RenderRequest,
    caller_id: &str,
    output_to_display: bool,
    analytics_config: Option<crate::AnalyticsConfig>,
) -> Result<DynamicImage, String> {
    process_and_get_dynamic_image_inner(
        context,
        state,
        base_image,
        transform_hash,
        request,
        caller_id,
        RenderOutputPrecision::EightBit,
        output_to_display,
        analytics_config,
    )
}

#[allow(clippy::too_many_arguments)]
fn process_and_get_dynamic_image_inner(
    context: &GpuContext,
    state: &tauri::State<AppState>,
    base_image: &DynamicImage,
    transform_hash: u64,
    request: RenderRequest,
    caller_id: &str,
    output_precision: RenderOutputPrecision,
    output_to_display: bool,
    analytics_config: Option<crate::AnalyticsConfig>,
) -> Result<DynamicImage, String> {
    let start_time = Instant::now();
    let (width, height) = base_image.dimensions();
    let device = &context.device;
    let queue = &context.queue;

    let max_dim = context.limits.max_texture_dimension_2d;
    if width > max_dim || height > max_dim {
        log::warn!(
            "Image dimensions ({}x{}) exceed GPU limits ({}). Bypassing GPU processing and returning unprocessed image to prevent a crash. Try upgrading your GPU :)",
            width,
            height,
            max_dim
        );
        return Ok(base_image.clone());
    }

    let mut processor_lock = match state.gpu_processor.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("GPU processor lock was poisoned. Resetting to self-heal.");
            let mut guard = poisoned.into_inner();
            *guard = None;
            guard
        }
    };
    let mut needs_new_processor = false;
    let new_width = (width + 255) & !255;
    let new_height = (height + 255) & !255;

    if let Some(p) = processor_lock.as_ref() {
        if p.width < width || p.height < height {
            needs_new_processor = true;
        }
    } else {
        needs_new_processor = true;
    }

    if needs_new_processor {
        log::info!(
            "Creating new GPU Processor for dimensions up to {}x{}",
            new_width,
            new_height
        );

        let old_processor = processor_lock.take();
        drop(old_processor);

        let _ = gpu_diag!(
            "processing/preparation_poll",
            context.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_millis(500)),
            })
        );

        let new_processor = GpuProcessor::new(context.clone(), new_width, new_height)?;

        *processor_lock = Some(crate::GpuProcessorState {
            processor: new_processor,
            width: new_width,
            height: new_height,
        });
    }

    let processor_state = processor_lock.as_ref().unwrap();
    let processor = &processor_state.processor;

    let mut cache_lock = match state.gpu_image_cache.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("GPU image cache lock was poisoned. Resetting to self-heal.");
            let mut guard = poisoned.into_inner();
            *guard = None;
            guard
        }
    };
    let mut needs_new_cache = false;
    let is_raw = request.adjustments.global.is_raw_image;

    if let Some(cache) = &*cache_lock {
        if cache.transform_hash != transform_hash
            || cache.width != width
            || cache.height != height
            || cache.is_raw != is_raw
        {
            needs_new_cache = true;
        }
    } else {
        needs_new_cache = true;
    }

    if needs_new_cache {
        let old_cache = cache_lock.take();
        drop(old_cache);

        let _ = gpu_diag!(
            "processing/preparation_poll",
            context.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_millis(500)),
            })
        );

        let img_rgba_f16 = gpu_diag!("upload/convert_f16", to_rgba_f16(base_image));
        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = gpu_diag!(
            "upload/input_texture",
            device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("Input Texture"),
                    size: texture_size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba16Float,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                TextureDataOrder::MipMajor,
                bytemuck::cast_slice(&img_rgba_f16),
            )
        );
        let texture_view = texture.create_view(&Default::default());

        let (gf_coeffs_view, gf_dehaze_view) =
            processor.build_guided_coeffs(&texture_view, width, height, is_raw);

        *cache_lock = Some(GpuImageCache {
            texture,
            texture_view,
            gf_coeffs_view,
            gf_dehaze_view,
            is_raw,
            width,
            height,
            transform_hash,
        });
    }

    let cache = cache_lock.as_ref().unwrap();

    let skip_readback = output_to_display;

    let (processed_pixels, out_w, out_h, out_x, out_y) = processor.run(
        &cache.texture_view,
        &cache.gf_coeffs_view,
        &cache.gf_dehaze_view,
        cache.width,
        cache.height,
        request,
        output_to_display,
        output_precision,
    )?;

    let mut final_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Final Passes Encoder"),
    });
    let mut submit_final_encoder = false;

    let mut async_readback_buffer: Option<wgpu::Buffer> = None;
    let mut async_padded_bpr: u32 = 0;
    let mut async_unpadded_bpr: u32 = 0;

    if analytics_config.is_some() && skip_readback {
        let unpadded_bytes_per_row = 4 * out_w;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = (unpadded_bytes_per_row + align - 1) & !(align - 1);
        let output_buffer_size = (padded_bytes_per_row * out_h) as u64;

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Async Analytics Readback Buffer"),
            size: output_buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        final_encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &processor.working_texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: out_x,
                    y: out_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(out_h),
                },
            },
            wgpu::Extent3d {
                width: out_w,
                height: out_h,
                depth_or_array_layers: 1,
            },
        );

        async_readback_buffer = Some(output_buffer);
        async_padded_bpr = padded_bytes_per_row;
        async_unpadded_bpr = unpadded_bytes_per_row;
        submit_final_encoder = true;
    }

    if output_to_display {
        final_encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &processor.working_texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: out_x,
                    y: out_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &processor.output_texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: out_x,
                    y: out_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: out_w,
                height: out_h,
                depth_or_array_layers: 1,
            },
        );
        submit_final_encoder = true;
    }

    if submit_final_encoder {
        queue.submit(Some(final_encoder.finish()));
    }

    if let Some(analytics) = analytics_config {
        if let Some(buffer) = async_readback_buffer {
            let output_buffer: wgpu::Buffer = buffer;
            let padded_bytes_per_row: u32 = async_padded_bpr;
            let unpadded_bytes_per_row: u32 = async_unpadded_bpr;
            let device_clone = context.device.clone();

            std::thread::spawn(move || {
                let buffer_slice = output_buffer.slice(..);
                let (tx, rx) = std::sync::mpsc::channel::<Result<(), wgpu::BufferAsyncError>>();

                let callback_request = analytics.request.clone();
                buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                    let _held_request = callback_request;
                    let _ = tx.send(result);
                });

                if let Err(e) = device_clone.poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(std::time::Duration::from_secs(60)),
                }) {
                    log::error!("Async analytics readback poll failed: {}", e);
                    return;
                }

                if let Ok(Ok(())) = rx.recv() {
                    let padded_data = buffer_slice.get_mapped_range().to_vec();
                    output_buffer.unmap();

                    let mut unpadded_data =
                        Vec::with_capacity((unpadded_bytes_per_row * out_h) as usize);
                    if padded_bytes_per_row == unpadded_bytes_per_row {
                        unpadded_data = padded_data;
                    } else {
                        for chunk in padded_data.chunks(padded_bytes_per_row as usize) {
                            unpadded_data
                                .extend_from_slice(&chunk[..unpadded_bytes_per_row as usize]);
                        }
                    }

                    if let Some(img_buf) =
                        ImageBuffer::<Rgba<u8>, _>::from_raw(out_w, out_h, unpadded_data)
                    {
                        let dynamic_img = DynamicImage::ImageRgba8(img_buf);
                        if analytics
                            .request
                            .as_ref()
                            .is_some_and(|r| r.check().is_err())
                        {
                            return;
                        }
                        let _ = analytics.sender.send(crate::AnalyticsJob {
                            request: analytics.request,
                            path: analytics.path,
                            image: std::sync::Arc::new(dynamic_img),
                            compute_waveform: analytics.compute_waveform,
                            active_waveform_channel: analytics.active_waveform_channel,
                        });
                    }
                }
            });
        } else {
            if let RenderedPixels::U8(pixels) = &processed_pixels {
                let pixels_clone = pixels.clone();
                std::thread::spawn(move || {
                    if let Some(img_buf) =
                        ImageBuffer::<Rgba<u8>, _>::from_raw(out_w, out_h, pixels_clone)
                    {
                        let dynamic_img = DynamicImage::ImageRgba8(img_buf);
                        if analytics
                            .request
                            .as_ref()
                            .is_some_and(|r| r.check().is_err())
                        {
                            return;
                        }
                        let _ = analytics.sender.send(crate::AnalyticsJob {
                            request: analytics.request,
                            path: analytics.path,
                            image: std::sync::Arc::new(dynamic_img),
                            compute_waveform: analytics.compute_waveform,
                            active_waveform_channel: analytics.active_waveform_channel,
                        });
                    }
                });
            } else {
                log::warn!("Skipping analytics for a high-precision CPU readback");
            }
        }
    }

    if output_to_display
        && let Some(display) = context
            .display
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
    {
        display.latest_transform.image_size = [width as f32, height as f32];
        display.latest_transform.texture_size =
            [processor_state.width as f32, processor_state.height as f32];

        queue.write_buffer(
            &display.transform_buffer,
            0,
            bytemuck::bytes_of(&display.latest_transform),
        );

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &display.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: display.transform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&processor.output_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&display.sampler),
                },
            ],
            label: None,
        });
        display.current_bind_group = Some(bind_group);
        display.render(device, queue);
    }

    if skip_readback {
        let duration = start_time.elapsed();
        let fps = 1.0 / duration.as_secs_f64();
        log::info!(
            "[{}] {}x{} native WGPU display updated in {:?} ({:.2} FPS)",
            caller_id,
            width,
            height,
            duration,
            fps
        );
        return Ok(DynamicImage::new_rgba8(0, 0));
    }

    let duration = start_time.elapsed();
    let fps = 1.0 / duration.as_secs_f64();
    log::info!(
        "[{}] {}x{} processed (ROI: {}x{}) on GPU in {:?} ({:.2} FPS)",
        caller_id,
        width,
        height,
        out_w,
        out_h,
        duration,
        fps
    );

    match processed_pixels {
        RenderedPixels::U8(pixels) => {
            let img_buf = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(out_w, out_h, pixels)
                .ok_or("Failed to create 8-bit image buffer from GPU data")?;
            Ok(DynamicImage::ImageRgba8(img_buf))
        }
        RenderedPixels::U16(pixels) => {
            let img_buf = ImageBuffer::<Rgba<u16>, Vec<u16>>::from_raw(out_w, out_h, pixels)
                .ok_or("Failed to create 16-bit image buffer from GPU data")?;
            Ok(DynamicImage::ImageRgba16(img_buf))
        }
    }
}

#[cfg(test)]
mod processing_limits_tests {
    use super::*;

    #[test]
    fn zero_storage_limits_fail_before_pipeline_creation() {
        let supported = wgpu::Limits {
            max_storage_textures_per_shader_stage: 0,
            max_storage_buffers_per_shader_stage: 0,
            ..Default::default()
        };
        let error = required_processing_limits(&supported).unwrap_err();
        assert!(
            error.contains("max_storage_textures_per_shader_stage requires 2, adapter supports 0")
        );
        assert!(
            error.contains("max_storage_buffers_per_shader_stage requires 1, adapter supports 0")
        );
    }

    #[test]
    fn compute_limits_are_explicit_and_within_adapter_capabilities() {
        let supported = wgpu::Limits::default();
        let required = required_processing_limits(&supported).unwrap();
        assert_eq!(required.max_storage_textures_per_shader_stage, 2);
        assert_eq!(required.max_storage_buffers_per_shader_stage, 1);
        assert_eq!(required.max_compute_invocations_per_workgroup, 256);
        assert_eq!(required.max_compute_workgroup_size_x, 256);
        assert_eq!(required.max_compute_workgroup_size_y, 256);
        assert_eq!(required.max_sampled_textures_per_shader_stage, 13);
        assert!(required.check_limits(&supported));
    }

    #[test]
    fn insufficient_blur_workgroup_limit_is_rejected() {
        let supported = wgpu::Limits {
            max_compute_workgroup_size_y: 128,
            ..Default::default()
        };
        assert!(
            required_processing_limits(&supported)
                .unwrap_err()
                .contains("max_compute_workgroup_size_y requires 256")
        );
    }

    #[test]
    fn flare_combined_groups_require_two_storage_textures() {
        let mut supported = wgpu::Limits {
            max_storage_textures_per_shader_stage: 1,
            ..Default::default()
        };
        assert!(
            required_processing_limits(&supported)
                .unwrap_err()
                .contains("max_storage_textures_per_shader_stage requires 2, adapter supports 1")
        );
        supported.max_storage_textures_per_shader_stage = 2;
        supported.max_bind_groups = 1;
        assert!(
            required_processing_limits(&supported)
                .unwrap_err()
                .contains("max_bind_groups requires 2, adapter supports 1")
        );
    }

    /// Build the real blur, flare (including its combined two-group layout),
    /// guided and main pipelines on a device with the exact requested limits.
    /// Naga-only/parity-shader tests cannot catch host pipeline-layout limits.
    /// Explicit invocation fails if hardware is unavailable; it never skips.
    #[test]
    #[ignore = "requires hardware wgpu adapter; constructs full GpuProcessor and both output precision pipelines"]
    fn full_gpu_processor_validates_both_precision_modes() {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        // Mirror the production Android backend default; allow backend-specific runs.
        if cfg!(target_os = "android") && std::env::var_os("WGPU_BACKEND").is_none() {
            descriptor.backends = wgpu::Backends::VULKAN;
        }
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .expect("hardware pipeline validation requires an available adapter");
        let info = adapter.get_info();
        assert_ne!(
            info.device_type,
            wgpu::DeviceType::Cpu,
            "hardware test requires a hardware adapter: {info:?}"
        );
        assert!(
            adapter
                .get_downlevel_capabilities()
                .flags
                .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
        );
        let limits = required_processing_limits(&adapter.limits())
            .expect("adapter cannot run production compute pipelines");
        let features =
            adapter.features() & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Full processor validation device"),
            required_features: features,
            required_limits: limits.clone(),
            ..Default::default()
        }))
        .expect("request production-limit device");
        let device = Arc::new(device);
        let uncaptured = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let errors = Arc::clone(&uncaptured);
        device.on_uncaptured_error(Arc::new(move |error| {
            errors.lock().unwrap().push(format!("{error}"));
        }));
        let context = GpuContext {
            device: Arc::clone(&device),
            queue: Arc::new(queue),
            limits,
            display: Arc::new(std::sync::Mutex::new(None)),
        };

        // Creation can return Ok containing invalid wgpu handles when errors are
        // logged. Check scopes independently so an invalid constructor never passes.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let processor = GpuProcessor::new(context, 64, 64);
        let error = pollster::block_on(scope.pop());
        assert!(
            error.is_none(),
            "8-bit/full constructor validation failed on {info:?}: {error:?}"
        );
        let processor = processor.expect("construct full GPU processor");

        // These are lazy in production: constructing GpuProcessor alone misses them.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _pipeline = processor.high_precision_pipeline();
        let _tile = processor.high_precision_tile();
        let error = pollster::block_on(scope.pop());
        assert!(
            error.is_none(),
            "16-bit pipeline/tile validation failed on {info:?}: {error:?}"
        );
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .expect("poll validation device");
        let errors = uncaptured.lock().unwrap();
        assert!(
            errors.is_empty(),
            "uncaptured wgpu errors on {info:?}: {errors:?}"
        );
        eprintln!("Full GPU processor + 8/16-bit pipelines VALID: {info:?}");
    }
}

// BEGIN ISOLATED CREATIVE OWNER (inactive, no normal cache/display caller).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IsolatedCreativeLayout {
    width: u32,
    height: u32,
    padded_width: u32,
    padded_height: u32,
    pixels: usize,
    rgba_samples: usize,
    upload_bytes: usize,
}

impl IsolatedCreativeLayout {
    fn checked(width: u32, height: u32, max_dimension: u32) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("Isolated creative input must have nonzero dimensions".into());
        }
        let pad = |n: u32| n.checked_add(255).map(|v| v & !255);
        let padded_width = pad(width).ok_or("Isolated creative width padding overflow")?;
        let padded_height = pad(height).ok_or("Isolated creative height padding overflow")?;
        if padded_width > max_dimension || padded_height > max_dimension {
            return Err("Isolated creative padded dimensions exceed the device limit".into());
        }
        // run() indexes output samples with u32 products before conversion to
        // usize. Check that exact existing arithmetic, not only host capacity.
        let rgba = width
            .checked_mul(height)
            .and_then(|v| v.checked_mul(4))
            .ok_or("Isolated creative RGBA indexing overflow")?;
        padded_width
            .checked_mul(padded_height)
            .and_then(|v| v.checked_mul(4))
            .ok_or("Isolated creative padded allocation overflow")?;
        let rgba_samples =
            usize::try_from(rgba).map_err(|_| "Isolated creative host length overflow")?;
        let upload_bytes = isolated_checked_bytes(rgba_samples, std::mem::size_of::<f16>())?;
        Ok(Self {
            width,
            height,
            padded_width,
            padded_height,
            pixels: rgba_samples / 4,
            rgba_samples,
            upload_bytes,
        })
    }
}

fn isolated_checked_bytes(elements: usize, element_size: usize) -> Result<usize, String> {
    let bytes = elements
        .checked_mul(element_size)
        .filter(|v| *v <= isize::MAX as usize)
        .ok_or("Isolated creative byte length overflow")?;
    Ok(bytes)
}

fn isolated_validate_source_bytes(
    layout: IsolatedCreativeLayout,
    bytes_per_pixel: u8,
    source_bytes: usize,
) -> Result<(), String> {
    let expected = isolated_checked_bytes(layout.pixels, bytes_per_pixel as usize)?;
    // The unchanged fallback conversion first creates RGBA32F before f16.
    isolated_checked_bytes(layout.rgba_samples, std::mem::size_of::<f32>())?;
    if bytes_per_pixel == 0 || source_bytes != expected {
        return Err("Isolated creative source storage does not match its dimensions".into());
    }
    Ok(())
}

fn isolated_validate_request(
    layout: IsolatedCreativeLayout,
    limits: &wgpu::Limits,
    request: &RenderRequest<'_>,
) -> Result<(), String> {
    // Existing constructor resources: fixed512 flare maps, fixed4096 tone LUT.
    if limits.max_texture_dimension_2d < FLARE_MAP_SIZE
        || limits.max_texture_dimension_1d < 4096
        || limits.max_texture_dimension_3d == 0
        || layout.width.max(layout.height) > limits.max_compute_workgroups_per_dimension
    {
        return Err("Isolated creative device cannot hold the existing engine resources".into());
    }
    let roi = request.roi.unwrap_or(Roi {
        x: 0,
        y: 0,
        width: layout.width,
        height: layout.height,
    });
    if roi.width == 0
        || roi.height == 0
        || roi
            .x
            .checked_add(roi.width)
            .is_none_or(|v| v > layout.width)
        || roi
            .y
            .checked_add(roi.height)
            .is_none_or(|v| v > layout.height)
    {
        return Err("Isolated creative ROI must be a nonempty input rectangle".into());
    }
    let output_samples = roi
        .width
        .checked_mul(roi.height)
        .and_then(|v| v.checked_mul(4))
        .ok_or("Isolated creative ROI output overflow")?;
    isolated_checked_bytes(output_samples as usize, std::mem::size_of::<u16>())?;
    let layers = request.mask_bitmaps.len().clamp(2, MAX_MASKS);
    if request.mask_bitmaps.len() > MAX_MASKS
        || request.adjustments.mask_count as usize > MAX_MASKS
        || request.adjustments.mask_count as usize > layers
        || layers as u32 > limits.max_texture_array_layers
    {
        return Err("Isolated creative mask count exceeds the existing array".into());
    }
    isolated_checked_bytes(layout.pixels, layers)?;
    for mask in request.mask_bitmaps {
        if mask.dimensions() != (layout.width, layout.height)
            || mask.as_raw().len() != layout.pixels
        {
            return Err("Isolated creative masks must match the complete input dimensions".into());
        }
    }
    // Preserve run()'s existing two zero-padded mask layers for absent bitmaps.
    // No mask is truncated and no out-of-range adjustment can sample a layer.
    if request.adjustments.global.has_lut == 1 && request.lut.is_none() {
        return Err("Isolated creative active LUT requires its captured payload".into());
    }
    if let Some(lut) = &request.lut {
        if lut.size == 0 || lut.size > limits.max_texture_dimension_3d {
            return Err("Isolated creative LUT dimensions exceed the device limit".into());
        }
        let samples = (lut.size as usize)
            .checked_pow(3)
            .and_then(|v| v.checked_mul(3))
            .ok_or("Isolated creative LUT length overflow")?;
        isolated_checked_bytes(samples / 3, 4 * std::mem::size_of::<f16>())?;
        if lut.data.len() != samples || !lut.data.iter().all(|v| v.is_finite()) {
            return Err("Isolated creative LUT must contain exact finite RGB texels".into());
        }
    }
    if request.adjustments.global.has_dcp != 0 {
        let dcp = request
            .dcp_textures
            .as_ref()
            .ok_or("Isolated creative active DCP requires its captured textures")?;
        for (dims, values, uniform) in [
            (
                dcp.huesat_dims,
                &dcp.huesat,
                request.adjustments.global.dcp_huesat_dims,
            ),
            (
                dcp.look_dims,
                &dcp.look,
                request.adjustments.global.dcp_look_dims,
            ),
        ] {
            if dims
                .iter()
                .any(|n| *n == 0 || *n > limits.max_texture_dimension_3d)
            {
                return Err(
                    "Isolated creative DCP texture dimensions exceed the device limit".into(),
                );
            }
            let texels = dims
                .into_iter()
                .try_fold(1usize, |a, n| a.checked_mul(n as usize))
                .ok_or("Isolated creative DCP length overflow")?;
            let samples = texels
                .checked_mul(4)
                .ok_or("Isolated creative DCP RGBA overflow")?;
            isolated_checked_bytes(samples, std::mem::size_of::<f32>())?;
            if values.len() != samples
                || !values.iter().all(|v| v.is_finite())
                || (uniform[0] != 0 && uniform[..3] != dims)
            {
                return Err(
                    "Isolated creative DCP table/active uniform dimensions do not match".into(),
                );
            }
        }
        // dcp_eval_tone_curve uses the literal4096, including endpoint loads.
        if dcp.tone_curve.len() != 4096 || !dcp.tone_curve.iter().all(|v| v.is_finite()) {
            return Err("Isolated creative DCP tone curve requires4096 finite samples".into());
        }
    }
    Ok(())
}

fn isolated_required_readback_bytes(
    layout: IsolatedCreativeLayout,
    roi: Option<Roi>,
    precision: RenderOutputPrecision,
) -> Result<u64, String> {
    let bounds = roi.unwrap_or(Roi {
        x: 0,
        y: 0,
        width: layout.width,
        height: layout.height,
    });
    let axis = |full: u32, offset: u32, size: u32| -> Result<u64, String> {
        let end = offset
            .checked_add(size)
            .filter(|n| size != 0 && *n <= full)
            .ok_or("Invalid isolated readback rectangle")? as u64;
        let first = offset as u64 / 2048;
        let last = (end - 1) / 2048;
        let mut maximum = 0;
        // Only the first/last tiles can be ROI-partial. The first interior
        // tile represents every full interior tile, without iterating a grid.
        for tile in [first, last, (first + 1).min(last)] {
            let start = (tile * 2048).max(offset as u64);
            let stop = ((tile + 1) * 2048).min(end);
            let span = (stop + 128).min(full as u64) - start.saturating_sub(128);
            maximum = maximum.max(span);
        }
        Ok(maximum)
    };
    let width = axis(layout.width, bounds.x, bounds.width)?;
    let height = axis(layout.height, bounds.y, bounds.height)?;
    let bpp = match precision {
        RenderOutputPrecision::EightBit => 4,
        RenderOutputPrecision::SixteenBit => 8,
    };
    let row = width
        .checked_mul(bpp)
        .and_then(|v| v.checked_add(255))
        .map(|v| v & !255)
        .ok_or("Isolated readback row overflow")?;
    let bytes = row
        .checked_mul(height)
        .filter(|v| *v <= u32::MAX as u64)
        .ok_or("Isolated readback buffer arithmetic overflow")?;
    Ok(bytes)
}

fn isolated_readback_image(
    pixels: RenderedPixels,
    width: u32,
    height: u32,
    precision: RenderOutputPrecision,
) -> Result<DynamicImage, String> {
    if width == 0 || height == 0 {
        return Err("Isolated creative readback must have nonzero dimensions".into());
    }
    let samples = width
        .checked_mul(height)
        .and_then(|v| v.checked_mul(4))
        .ok_or("Isolated creative readback length overflow")? as usize;
    match (pixels, precision) {
        (RenderedPixels::U8(v), RenderOutputPrecision::EightBit) if v.len() == samples => {
            ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, v)
                .map(DynamicImage::ImageRgba8)
                .ok_or_else(|| "Invalid isolated8-bit readback".into())
        }
        (RenderedPixels::U16(v), RenderOutputPrecision::SixteenBit) if v.len() == samples => {
            isolated_checked_bytes(samples, std::mem::size_of::<u16>())?;
            ImageBuffer::<Rgba<u16>, Vec<u16>>::from_raw(width, height, v)
                .map(DynamicImage::ImageRgba16)
                .ok_or_else(|| "Invalid isolated16-bit readback".into())
        }
        _ => Err("Isolated creative precision or exact readback length mismatch".into()),
    }
}

/// Inactive, explicit-input creative engine owner. No AppState, source keys,
/// normal caches, hydration, analytics or display is accepted by this API.
///
/// Call construction/render/drop outside application/owner/cache locks. render
/// blocks in the unchanged run/readback path; callers must fence a future job
/// through its real return. There is no cancellation, background task, driver
/// scheduling, total-memory/RSS or GPU-completion-on-error claim here.
/// A mutable borrow admits only one render through this owner at a time.
pub(crate) struct IsolatedCreativeRenderer {
    context: GpuContext,
    processor: Option<GpuProcessor>,
    capacity: [u32; 2],
    #[cfg(test)]
    processor_builds: usize,
}

impl IsolatedCreativeRenderer {
    pub(crate) fn new(context: &GpuContext) -> Self {
        // Borrow the context and clone only device/queue/limits: no native
        // display Arc is taken, retained, locked, replaced or retired here.
        let isolated_context = GpuContext {
            device: Arc::clone(&context.device),
            queue: Arc::clone(&context.queue),
            limits: context.limits.clone(),
            display: Arc::new(std::sync::Mutex::new(None)),
        };
        Self {
            context: isolated_context,
            processor: None,
            capacity: [0, 0],
            #[cfg(test)]
            processor_builds: 0,
        }
    }

    pub(crate) fn matches_context(&self, context: &GpuContext) -> bool {
        Arc::ptr_eq(&self.context.device, &context.device)
            && Arc::ptr_eq(&self.context.queue, &context.queue)
    }

    #[cfg(test)]
    pub(crate) fn processor_build_count(&self) -> usize {
        self.processor_builds
    }

    pub(crate) fn render(
        &mut self,
        base_image: &DynamicImage,
        request: RenderRequest<'_>,
        precision: RenderOutputPrecision,
    ) -> Result<DynamicImage, String> {
        let (pixels, width, height) = self.render_inner(base_image, request, precision, None)?;
        isolated_readback_image(pixels, width, height, precision)
    }

    pub(crate) fn sample_point_colour(
        &mut self,
        base_image: &DynamicImage,
        mut request: RenderRequest<'_>,
        uv: [f32; 2],
    ) -> Result<crate::image_processing::PointColourSample, String> {
        let (x, y) = point_colour_pixel(uv, base_image.dimensions())?;
        // Use exactly the full-render tile and halo, not a reduced one-pixel
        // tile whose blur edges would change the upstream sampled value.
        let tx = x / 2048 * 2048;
        let ty = y / 2048 * 2048;
        request.roi = Some(Roi {
            x: tx,
            y: ty,
            width: 2048.min(base_image.width() - tx),
            height: 2048.min(base_image.height() - ty),
        });
        request.adjustments.global.point_probe = [x, y, 1, 0];
        let mut sample = None;
        self.render_inner(
            base_image,
            request,
            RenderOutputPrecision::EightBit,
            Some(&mut sample),
        )?;
        let sample =
            sample.ok_or_else(|| "Point Colour stage did not produce a sample".to_string())?;
        if sample.saturation <= 0.05 {
            return Err("Choose a coloured area; this sample is neutral".into());
        }
        Ok(sample)
    }

    fn render_inner(
        &mut self,
        base_image: &DynamicImage,
        request: RenderRequest<'_>,
        precision: RenderOutputPrecision,
        sample: Option<&mut Option<crate::image_processing::PointColourSample>>,
    ) -> Result<(RenderedPixels, u32, u32), String> {
        let (width, height) = base_image.dimensions();
        let limits = self.context.device.limits();
        let layout =
            IsolatedCreativeLayout::checked(width, height, limits.max_texture_dimension_2d)?;
        isolated_validate_source_bytes(
            layout,
            base_image.color().bytes_per_pixel(),
            base_image.as_bytes().len(),
        )?;
        isolated_validate_request(layout, &limits, &request)?;
        request.adjustments.global.point_colour.validate()?;
        let readback_bytes = if sample.is_some() {
            256
        } else {
            isolated_required_readback_bytes(layout, request.roi, precision)?
        };
        if readback_bytes > limits.max_buffer_size
            || std::mem::size_of::<AllAdjustments>() as u64 > limits.max_buffer_size
        {
            return Err("Isolated creative buffers exceed the actual device buffer limit".into());
        }

        if self.processor.is_none()
            || self.capacity[0] < layout.padded_width
            || self.capacity[1] < layout.padded_height
        {
            // Only this owner's processor is retired, outside all app locks.
            drop(self.processor.take());
            self.capacity = [0, 0];
            let start = std::time::Instant::now();
            let created = GpuProcessor::new(
                self.context.clone(),
                layout.padded_width,
                layout.padded_height,
            );
            log::info!(
                "[preview-stage] stage=gpu_processor_construct elapsed={:?} width={} height={} outcome={}",
                start.elapsed(),
                layout.padded_width,
                layout.padded_height,
                if created.is_ok() {
                    "completed"
                } else {
                    "error"
                }
            );
            let processor = created?;
            self.processor = Some(processor);
            #[cfg(test)]
            {
                self.processor_builds += 1;
            }
            self.capacity = [layout.padded_width, layout.padded_height];
        }
        let processor = self
            .processor
            .as_ref()
            .ok_or("Isolated creative processor unavailable")?;
        // Deliberately unconditional: content and guided data are uploaded anew
        // even when dimensions/precision/request bytes equal a previous render.
        let rgba = to_rgba_f16(base_image);
        if rgba.len() != layout.rgba_samples
            || bytemuck::cast_slice::<f16, u8>(&rgba).len() != layout.upload_bytes
        {
            return Err("Isolated creative input conversion length mismatch".into());
        }
        let texture = self.context.device.create_texture_with_data(
            &self.context.queue,
            &wgpu::TextureDescriptor {
                label: Some("Isolated creative fresh input"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            TextureDataOrder::MipMajor,
            bytemuck::cast_slice(&rgba),
        );
        drop(rgba);
        let view = texture.create_view(&Default::default());
        let (guided, dehaze) = processor.build_guided_coeffs(
            &view,
            width,
            height,
            request.adjustments.global.is_raw_image,
        );
        let (pixels, out_w, out_h, _, _) = processor.run_with_point_sample(
            &view, &guided, &dehaze, width, height, request, false, precision, sample,
        )?;
        // The fresh upload and guided views stay alive through actual blocking
        // readback. No normal wrapper/final-display/analytics path is entered.
        drop((texture, view, guided, dehaze));
        Ok((pixels, out_w, out_h))
    }
}

#[cfg(test)]
mod isolated_creative_tests {
    use super::*;

    fn request<'a>(masks: &'a [ImageBuffer<Luma<u8>, Vec<u8>>]) -> RenderRequest<'a> {
        RenderRequest {
            adjustments: AllAdjustments::default(),
            mask_bitmaps: masks,
            lut: None,
            roi: None,
            dcp_textures: None,
        }
    }

    fn identity_dcp() -> DcpTextureData {
        DcpTextureData {
            huesat: vec![0.0, 1.0, 1.0, 1.0],
            huesat_dims: [1; 3],
            look: vec![0.0, 1.0, 1.0, 1.0],
            look_dims: [1; 3],
            tone_curve: (0..4096).map(|n| n as f32 / 4095.0).collect(),
        }
    }

    fn identity_lut() -> Arc<Lut> {
        let mut data = Vec::new();
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    data.extend_from_slice(&[r as f32, g as f32, b as f32]);
                }
            }
        }
        Arc::new(Lut { size: 2, data })
    }

    #[test]
    fn isolated_padding_and_nonzero_known_answers() {
        for (width, expected) in [(1, 256), (255, 256), (256, 256), (257, 512)] {
            let layout = IsolatedCreativeLayout::checked(width, 1, 1024).unwrap();
            assert_eq!((layout.padded_width, layout.padded_height), (expected, 256));
            assert_eq!(layout.pixels, width as usize);
            assert_eq!(layout.rgba_samples, width as usize * 4);
            assert_eq!(layout.upload_bytes, width as usize * 8);
        }
        assert!(IsolatedCreativeLayout::checked(0, 1, 1024).is_err());
        assert!(IsolatedCreativeLayout::checked(1, 0, 1024).is_err());
    }

    #[test]
    fn isolated_gpu_limit_and_checked_lengths() {
        assert!(IsolatedCreativeLayout::checked(257, 1, 300).is_err());
        assert_eq!(
            IsolatedCreativeLayout::checked(257, 1, 512)
                .unwrap()
                .padded_width,
            512
        );
        assert!(IsolatedCreativeLayout::checked(u32::MAX, 1, u32::MAX).is_err());
        assert!(IsolatedCreativeLayout::checked(65536, 65536, u32::MAX).is_err());
        assert!(IsolatedCreativeLayout::checked(32768, 32768, u32::MAX).is_err());
        let row = IsolatedCreativeLayout::checked(257, 1, 512).unwrap();
        assert_eq!(
            isolated_required_readback_bytes(row, None, RenderOutputPrecision::EightBit).unwrap(),
            1280
        );
        assert_eq!(
            isolated_required_readback_bytes(row, None, RenderOutputPrecision::SixteenBit).unwrap(),
            2304
        );
        assert_eq!(
            isolated_required_readback_bytes(
                row,
                Some(Roi {
                    x: 256,
                    y: 0,
                    width: 1,
                    height: 1
                }),
                RenderOutputPrecision::SixteenBit
            )
            .unwrap(),
            1280
        );
        assert!(isolated_checked_bytes(usize::MAX, 2).is_err());
        assert!(isolated_checked_bytes(isize::MAX as usize, 2).is_err());
        let layout = IsolatedCreativeLayout::checked(6, 4, 512).unwrap();
        let limits = wgpu::Limits {
            max_texture_dimension_2d: 256,
            ..Default::default()
        };
        assert!(isolated_validate_request(layout, &limits, &request(&[])).is_err());
    }

    #[test]
    fn isolated_u8_readback_exact_alpha_and_errors() {
        let values = vec![0, 17, 255, 7, 128, 64, 32, 255];
        let image = isolated_readback_image(
            RenderedPixels::U8(values.clone()),
            2,
            1,
            RenderOutputPrecision::EightBit,
        )
        .unwrap();
        assert!(matches!(image, DynamicImage::ImageRgba8(_)));
        assert_eq!(image.as_rgba8().unwrap().as_raw(), &values);
        for v in [vec![0; 7], vec![0; 9]] {
            assert!(
                isolated_readback_image(
                    RenderedPixels::U8(v),
                    2,
                    1,
                    RenderOutputPrecision::EightBit
                )
                .is_err()
            );
        }
        assert!(
            isolated_readback_image(
                RenderedPixels::U8(vec![]),
                0,
                1,
                RenderOutputPrecision::EightBit
            )
            .is_err()
        );
        assert!(
            isolated_readback_image(
                RenderedPixels::U8(values),
                2,
                1,
                RenderOutputPrecision::SixteenBit
            )
            .is_err()
        );
    }

    #[test]
    fn isolated_u16_readback_exact_alpha_and_errors() {
        let values = vec![0, 17, 65535, 12345, 32768, 16384, 8192, 65535];
        let image = isolated_readback_image(
            RenderedPixels::U16(values.clone()),
            2,
            1,
            RenderOutputPrecision::SixteenBit,
        )
        .unwrap();
        assert!(matches!(image, DynamicImage::ImageRgba16(_)));
        assert_eq!(image.as_rgba16().unwrap().as_raw(), &values);
        for v in [vec![0; 7], vec![0; 9]] {
            assert!(
                isolated_readback_image(
                    RenderedPixels::U16(v),
                    2,
                    1,
                    RenderOutputPrecision::SixteenBit
                )
                .is_err()
            );
        }
        assert!(
            isolated_readback_image(
                RenderedPixels::U16(vec![]),
                1,
                0,
                RenderOutputPrecision::SixteenBit
            )
            .is_err()
        );
        assert!(
            isolated_readback_image(
                RenderedPixels::U16(values),
                2,
                1,
                RenderOutputPrecision::EightBit
            )
            .is_err()
        );
    }

    #[test]
    fn isolated_request_preserves_roi_and_validates_masks() {
        let limits = wgpu::Limits::default();
        let layout =
            IsolatedCreativeLayout::checked(6, 4, limits.max_texture_dimension_2d).unwrap();
        let masks = [ImageBuffer::from_pixel(6, 4, Luma([127]))];
        let mut req = request(&masks);
        req.roi = Some(Roi {
            x: 1,
            y: 1,
            width: 3,
            height: 2,
        });
        req.adjustments.mask_count = 1;
        isolated_validate_request(layout, &limits, &req).unwrap();
        let roi = req.roi.unwrap();
        assert_eq!((roi.x, roi.y, roi.width, roi.height), (1, 1, 3, 2));
        assert_eq!(req.mask_bitmaps[0].as_raw(), &vec![127; 24]);
        for roi in [
            Roi {
                x: 0,
                y: 0,
                width: 0,
                height: 1,
            },
            Roi {
                x: 5,
                y: 0,
                width: 2,
                height: 1,
            },
            Roi {
                x: 0,
                y: 4,
                width: 1,
                height: 1,
            },
            Roi {
                x: u32::MAX,
                y: 0,
                width: 1,
                height: 1,
            },
        ] {
            req.roi = Some(roi);
            assert!(isolated_validate_request(layout, &limits, &req).is_err());
        }
        let wrong = [ImageBuffer::from_pixel(5, 4, Luma([0]))];
        assert!(isolated_validate_request(layout, &limits, &request(&wrong)).is_err());
        let mut req = request(&[]);
        req.adjustments.mask_count = 2; // unchanged engine's two padded zero layers
        isolated_validate_request(layout, &limits, &req).unwrap();
        req.adjustments.mask_count = 3;
        assert!(isolated_validate_request(layout, &limits, &req).is_err());
        let too_many = vec![ImageBuffer::from_pixel(6, 4, Luma([0])); MAX_MASKS + 1];
        assert!(isolated_validate_request(layout, &limits, &request(&too_many)).is_err());
    }

    #[test]
    fn isolated_captured_lut_and_dcp_shape_validation() {
        let limits = wgpu::Limits::default();
        let layout =
            IsolatedCreativeLayout::checked(6, 4, limits.max_texture_dimension_2d).unwrap();
        let mut req = request(&[]);
        req.adjustments.global.has_lut = 1;
        assert!(isolated_validate_request(layout, &limits, &req).is_err());
        req.lut = Some(identity_lut());
        isolated_validate_request(layout, &limits, &req).unwrap();
        for lut in [
            Lut {
                size: 2,
                data: vec![0.0; 23],
            },
            Lut {
                size: 0,
                data: vec![],
            },
            Lut {
                size: u32::MAX,
                data: vec![],
            },
            Lut {
                size: 2,
                data: vec![f32::NAN; 24],
            },
        ] {
            req.lut = Some(Arc::new(lut));
            assert!(isolated_validate_request(layout, &limits, &req).is_err());
        }
        req.lut = Some(identity_lut());
        req.adjustments.global.has_dcp = 1;
        assert!(isolated_validate_request(layout, &limits, &req).is_err());
        req.dcp_textures = Some(identity_dcp());
        isolated_validate_request(layout, &limits, &req).unwrap();
        req.adjustments.global.dcp_huesat_dims = [2, 1, 1, 0];
        assert!(isolated_validate_request(layout, &limits, &req).is_err());
        req.adjustments.global.dcp_huesat_dims = [1, 1, 1, 0];
        for mutation in 0..4 {
            let mut dcp = identity_dcp();
            match mutation {
                0 => {
                    dcp.huesat.pop();
                }
                1 => dcp.look_dims = [0, 1, 1],
                2 => {
                    dcp.tone_curve.pop();
                }
                _ => dcp.tone_curve[10] = f32::INFINITY,
            }
            req.dcp_textures = Some(dcp);
            assert!(isolated_validate_request(layout, &limits, &req).is_err());
        }
        req.dcp_textures = Some(identity_dcp());
        isolated_validate_request(layout, &limits, &req).unwrap();
        let limited = wgpu::Limits {
            max_texture_dimension_1d: 2048,
            ..limits
        };
        assert!(isolated_validate_request(layout, &limited, &req).is_err());
    }

    #[test]
    fn isolated_upload_retains_existing_f16_values() {
        let input = DynamicImage::ImageRgb32F(
            image::Rgb32FImage::from_raw(1, 1, vec![-0.5, 1.0, 2.0]).unwrap(),
        );
        let layout = IsolatedCreativeLayout::checked(1, 1, 256).unwrap();
        isolated_validate_source_bytes(
            layout,
            input.color().bytes_per_pixel(),
            input.as_bytes().len(),
        )
        .unwrap();
        assert!(isolated_validate_source_bytes(layout, 12, 11).is_err());
        assert!(isolated_validate_source_bytes(layout, 12, 13).is_err());
        assert!(isolated_validate_source_bytes(layout, 0, 0).is_err());
        let before = input.as_bytes().to_vec();
        assert_eq!(
            to_rgba_f16(&input)
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            vec![0xb800, 0x3c00, 0x4000, 0x3c00]
        );
        assert_eq!(input.as_bytes(), before);
        let rgba = DynamicImage::ImageRgba32F(
            image::Rgba32FImage::from_raw(1, 1, vec![0.0, 0.5, 1.0, 0.25]).unwrap(),
        );
        assert_eq!(
            to_rgba_f16(&rgba)
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            vec![0, 0x3800, 0x3c00, 0x3400]
        );
    }

    // Real, dedicated hardware context: explicit ignored runs fail on missing
    // hardware rather than manufacturing a pass or using an AppState mock.
    pub(super) fn hardware_context() -> (GpuContext, Arc<std::sync::Mutex<Vec<String>>>) {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        if cfg!(target_os = "android") && std::env::var_os("WGPU_BACKEND").is_none() {
            descriptor.backends = wgpu::Backends::VULKAN;
        }
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .expect("isolated creative test requires an actual adapter");
        assert_ne!(adapter.get_info().device_type, wgpu::DeviceType::Cpu);
        assert!(
            adapter
                .get_downlevel_capabilities()
                .flags
                .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
        );
        let limits = required_processing_limits(&adapter.limits()).unwrap();
        let features =
            adapter.features() & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Isolated creative test device"),
            required_features: features,
            required_limits: limits.clone(),
            ..Default::default()
        }))
        .expect("request real isolated creative device");
        let errors = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let recorded = Arc::clone(&errors);
        device.on_uncaptured_error(Arc::new(move |e| {
            recorded.lock().unwrap().push(e.to_string())
        }));
        (
            GpuContext {
                device: Arc::new(device),
                queue: Arc::new(queue),
                limits,
                display: Arc::new(std::sync::Mutex::new(None)),
            },
            errors,
        )
    }

    fn finish_hardware(context: &GpuContext, errors: &std::sync::Mutex<Vec<String>>) {
        context
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(60)),
            })
            .expect("finish real creative submissions");
        let recorded = errors.lock().unwrap();
        assert!(recorded.is_empty(), "uncaptured GPU errors: {recorded:?}");
    }

    fn pattern_image(phase: u32) -> DynamicImage {
        DynamicImage::ImageRgb32F(image::Rgb32FImage::from_fn(33, 19, |x, y| {
            image::Rgb([
                0.05 + ((x + phase) % 33) as f32 / 45.0,
                0.08 + y as f32 / 30.0,
                0.12 + phase as f32 / 40.0,
            ])
        }))
    }

    fn actual_request<'a>(
        masks: &'a [ImageBuffer<Luma<u8>, Vec<u8>>],
        creative: bool,
    ) -> RenderRequest<'a> {
        let json = if creative {
            serde_json::json!({"exposure": 0.35, "contrast": 12,
            "saturation": -8, "sharpness": 18})
        } else {
            serde_json::json!({})
        };
        let mut adjustments = crate::image_processing::get_all_adjustments_from_json(
            &json,
            false,
            crate::as_shot::AsShot::reference(),
            crate::image_processing::RenderOptions::default(),
        );
        if creative {
            adjustments.mask_count = masks.len() as u32;
            adjustments.mask_adjustments[0].exposure = 0.2;
            adjustments.global.has_lut = 1;
            adjustments.global.lut_intensity = 0.5;
            adjustments.global.has_dcp = 1;
            adjustments.global.dcp_amount = 1.0;
        }
        RenderRequest {
            adjustments,
            mask_bitmaps: masks,
            lut: creative.then(identity_lut),
            roi: creative.then_some(Roi {
                x: 3,
                y: 5,
                width: 13,
                height: 9,
            }),
            dcp_textures: creative.then(identity_dcp),
        }
    }

    // Existing wrapper's direct upload/new/guided/run(false)/conversion route,
    // without AppState/caches/display. Fixed small fixtures make old padding
    // arithmetic valid; this does not call the new owner or readback helper.
    fn reference_engine(
        context: &GpuContext,
        image: &DynamicImage,
        request: RenderRequest<'_>,
        precision: RenderOutputPrecision,
    ) -> DynamicImage {
        let (w, h) = image.dimensions();
        let processor =
            GpuProcessor::new(context.clone(), (w + 255) & !255, (h + 255) & !255).unwrap();
        let rgba = to_rgba_f16(image);
        let texture = context.device.create_texture_with_data(
            &context.queue,
            &wgpu::TextureDescriptor {
                label: Some("Reference existing-engine upload"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            TextureDataOrder::MipMajor,
            bytemuck::cast_slice(&rgba),
        );
        let view = texture.create_view(&Default::default());
        let (guided, dehaze) =
            processor.build_guided_coeffs(&view, w, h, request.adjustments.global.is_raw_image);
        let (pixels, width, height, _, _) = processor
            .run(&view, &guided, &dehaze, w, h, request, false, precision)
            .unwrap();
        match pixels {
            RenderedPixels::U8(v) => {
                DynamicImage::ImageRgba8(ImageBuffer::from_raw(width, height, v).unwrap())
            }
            RenderedPixels::U16(v) => {
                DynamicImage::ImageRgba16(ImageBuffer::from_raw(width, height, v).unwrap())
            }
        }
    }

    #[test]
    #[ignore = "requires explicit real hardware creative engine run; not executed by CPU checks"]
    fn isolated_matches_existing_engine_whole_output_both_precisions() {
        let (context, errors) = hardware_context();
        let image = pattern_image(0);
        let original = image.as_bytes().to_vec();
        let mut owner = IsolatedCreativeRenderer::new(&context);
        for precision in [
            RenderOutputPrecision::EightBit,
            RenderOutputPrecision::SixteenBit,
        ] {
            let scope = context
                .device
                .push_error_scope(wgpu::ErrorFilter::Validation);
            let expected =
                reference_engine(&context, &image, actual_request(&[], false), precision);
            let actual = owner
                .render(&image, actual_request(&[], false), precision)
                .unwrap();
            assert_eq!(actual.color(), expected.color());
            assert_eq!(actual.dimensions(), expected.dimensions());
            assert_eq!(actual.as_bytes(), expected.as_bytes());
            assert_eq!(image.as_bytes(), original);
            assert!(pollster::block_on(scope.pop()).is_none());
        }
        finish_hardware(&context, &errors);
    }

    #[test]
    #[ignore = "requires explicit actual GPU allocation/reuse/changed-input gate; source-only definition"]
    fn isolated_processor_reuse_updates_curve_mask_lut_and_grows() {
        let (context, errors) = hardware_context();
        let (other, other_errors) = hardware_context();
        let mut reused = IsolatedCreativeRenderer::new(&context);
        assert!(reused.matches_context(&context));
        assert!(!reused.matches_context(&other));
        assert_eq!(reused.processor_build_count(), 0);
        let image = pattern_image(0);
        let masks = [ImageBuffer::from_fn(33, 19, |x, y| {
            Luma([((x * 19 + y * 7) % 256) as u8])
        })];
        let changed = |creative: bool| {
            let mut req = actual_request(&masks, creative);
            if creative {
                let curves = crate::image_processing::get_all_adjustments_from_json(
                    &serde_json::json!({"curves":{"luma":[{"x":0.0,"y":0.0},{"x":128.0,"y":166.0},{"x":255.0,"y":255.0}]}}),
                    false,
                    crate::as_shot::AsShot::reference(),
                    crate::image_processing::RenderOptions::default(),
                );
                req.adjustments.global.luma_curve = curves.global.luma_curve;
                req.adjustments.global.luma_curve_count = curves.global.luma_curve_count;
                let mut lut = (*identity_lut()).clone();
                for rgb in lut.data.chunks_mut(3) {
                    rgb.swap(0, 2);
                }
                req.lut = Some(Arc::new(lut));
            }
            req
        };
        let zero_masks = [ImageBuffer::from_pixel(33, 19, Luma([0_u8]))];
        let initial = reused
            .render(
                &image,
                actual_request(&zero_masks, true),
                RenderOutputPrecision::EightBit,
            )
            .unwrap();
        assert_eq!(reused.processor_build_count(), 1);
        let actual = reused
            .render(&image, changed(true), RenderOutputPrecision::EightBit)
            .unwrap();
        let mut fresh = IsolatedCreativeRenderer::new(&context);
        let expected = fresh
            .render(&image, changed(true), RenderOutputPrecision::EightBit)
            .unwrap();
        assert_eq!(reused.processor_build_count(), 1);
        assert_eq!(actual.as_bytes(), expected.as_bytes());
        assert_eq!(initial.dimensions(), actual.dimensions());
        assert_ne!(initial.as_bytes(), actual.as_bytes());
        let bigger = DynamicImage::ImageRgb32F(image::Rgb32FImage::from_pixel(
            300,
            19,
            image::Rgb([0.2, 0.3, 0.4]),
        ));
        reused
            .render(
                &bigger,
                actual_request(&[], false),
                RenderOutputPrecision::EightBit,
            )
            .unwrap();
        assert_eq!(reused.processor_build_count(), 2);
        reused
            .render(
                &image,
                actual_request(&[], false),
                RenderOutputPrecision::EightBit,
            )
            .unwrap();
        assert_eq!(reused.processor_build_count(), 2);
        finish_hardware(&context, &errors);
        finish_hardware(&other, &other_errors);
    }

    #[test]
    #[ignore = "requires explicit real hardware same-size replacement run; not executed by CPU checks"]
    fn isolated_equal_size_replacement_matches_fresh_owner() {
        let (context, errors) = hardware_context();
        let a = pattern_image(0);
        let b = pattern_image(9);
        let mut reused = IsolatedCreativeRenderer::new(&context);
        for precision in [
            RenderOutputPrecision::EightBit,
            RenderOutputPrecision::SixteenBit,
        ] {
            let scope = context
                .device
                .push_error_scope(wgpu::ErrorFilter::Validation);
            let first = reused
                .render(&a, actual_request(&[], false), precision)
                .unwrap();
            let replaced = reused
                .render(&b, actual_request(&[], false), precision)
                .unwrap();
            let mut fresh = IsolatedCreativeRenderer::new(&context);
            let expected = fresh
                .render(&b, actual_request(&[], false), precision)
                .unwrap();
            assert_ne!(first.as_bytes(), replaced.as_bytes());
            assert_eq!(replaced.as_bytes(), expected.as_bytes());
            assert!(pollster::block_on(scope.pop()).is_none());
        }
        finish_hardware(&context, &errors);
    }

    // Fixed nonlinear hardware fixture: third case only, never a CPU oracle.
    fn edge_creative_image() -> DynamicImage {
        let palette = [
            [0.7, 0.25, 0.125],
            [0.125, 0.65, 0.3],
            [0.35, 0.125, 0.7],
            [0.6, 0.55, 0.2],
        ];
        DynamicImage::ImageRgb32F(image::Rgb32FImage::from_fn(33, 19, |x, y| {
            let edge = if x >= 10 { 2 } else { 0 };
            let index = ((x / 2 + y / 3 + edge) % 4) as usize;
            image::Rgb(palette[index])
        }))
    }

    fn active_table_request<'a>(masks: &'a [ImageBuffer<Luma<u8>, Vec<u8>>]) -> RenderRequest<'a> {
        let mut request = actual_request(masks, true);
        let mut dcp = identity_dcp();
        dcp.huesat_dims = [2, 2, 2];
        dcp.look_dims = [2, 2, 2];
        dcp.huesat = [24.0_f32, 0.75, 0.875, 1.0].repeat(8);
        dcp.look = [-12.0_f32, 0.875, 1.125, 1.0].repeat(8);
        request.adjustments.global.dcp_huesat_dims = [2, 2, 2, 0];
        request.adjustments.global.dcp_look_dims = [2, 2, 2, 0];
        request.dcp_textures = Some(dcp);
        request
    }

    #[test]
    #[ignore = "requires explicit real hardware captured assets/ROI/display isolation run; not executed by CPU checks"]
    fn isolated_roi_creative_and_display_state_are_isolated() {
        let (context, errors) = hardware_context();
        let shared_display = Arc::clone(&context.display);
        let image = edge_creative_image();
        let original_image = image.as_bytes().to_vec();
        let DynamicImage::ImageRgb32F(pixels) = &image else {
            panic!("fixed creative input must be RGB32F");
        };
        for (x, y, expected) in [
            (0, 0, [0.7, 0.25, 0.125]),
            (2, 0, [0.125, 0.65, 0.3]),
            (0, 3, [0.125, 0.65, 0.3]),
            (9, 6, [0.35, 0.125, 0.7]),
            (10, 6, [0.125, 0.65, 0.3]),
        ] {
            assert_eq!(pixels.get_pixel(x, y).0, expected);
        }
        let masks = [ImageBuffer::from_fn(33, 19, |x, y| {
            Luma([((x * 11 + y * 7) % 256) as u8])
        })];
        let original_mask = masks[0].as_raw().clone();
        let mut owner = IsolatedCreativeRenderer::new(&context);
        assert!(!Arc::ptr_eq(&owner.context.display, &shared_display));
        for precision in [
            RenderOutputPrecision::EightBit,
            RenderOutputPrecision::SixteenBit,
        ] {
            let scope = context
                .device
                .push_error_scope(wgpu::ErrorFilter::Validation);
            let req = active_table_request(&masks);
            assert_eq!(req.adjustments.global.sharpness, 0.36_f32);
            assert!(req.adjustments.global.sharpness > 0.0);
            assert_eq!(req.adjustments.global.has_lut, 1);
            assert_eq!(req.adjustments.global.has_dcp, 1);
            assert_eq!(req.adjustments.global.dcp_amount, 1.0);
            assert_eq!(req.adjustments.global.dcp_huesat_encoding, 0);
            assert_eq!(req.adjustments.global.dcp_look_encoding, 0);
            assert_eq!(req.adjustments.global.dcp_baseline_exposure_offset, 0.0);
            assert_eq!(req.adjustments.global.dcp_huesat_dims, [2, 2, 2, 0]);
            assert_eq!(req.adjustments.global.dcp_look_dims, [2, 2, 2, 0]);
            let identity_matrix = crate::image_processing::GpuMat3::default();
            assert_eq!(
                bytemuck::bytes_of(&req.adjustments.global.cam_to_prophoto),
                bytemuck::bytes_of(&identity_matrix)
            );
            assert_eq!(
                bytemuck::bytes_of(&req.adjustments.global.prophoto_to_working),
                bytemuck::bytes_of(&identity_matrix)
            );
            let dcp = req.dcp_textures.as_ref().unwrap();
            assert_eq!(dcp.huesat_dims, [2, 2, 2]);
            assert_eq!(dcp.look_dims, [2, 2, 2]);
            assert_eq!(dcp.huesat, [24.0_f32, 0.75, 0.875, 1.0].repeat(8));
            assert_eq!(dcp.look, [-12.0_f32, 0.875, 1.125, 1.0].repeat(8));
            assert_ne!(dcp.huesat[..4], [0.0, 1.0, 1.0, 1.0]);
            assert_ne!(dcp.look[..4], [0.0, 1.0, 1.0, 1.0]);
            assert!(dcp.huesat.iter().chain(&dcp.look).all(|v| v.is_finite()));
            assert_eq!(dcp.tone_curve, identity_dcp().tone_curve);
            let lut = Arc::clone(req.lut.as_ref().unwrap());
            let lut_before = lut.data.clone();
            // Every control owns a copy of the same captured request. Only
            // its named uniform changes; assets, geometry and other effects stay.
            let copy_request = || RenderRequest {
                adjustments: req.adjustments,
                mask_bitmaps: req.mask_bitmaps,
                lut: req.lut.clone(),
                roi: req.roi,
                dcp_textures: req.dcp_textures.clone(),
            };
            let expected = reference_engine(&context, &image, copy_request(), precision);
            let mut without_sharpness = copy_request();
            without_sharpness.adjustments.global.sharpness = 0.0;
            let mut without_huesat = copy_request();
            without_huesat.adjustments.global.dcp_huesat_dims = [0; 4];
            let mut without_look = copy_request();
            without_look.adjustments.global.dcp_look_dims = [0; 4];
            for (effect, control) in [
                ("sharpness", without_sharpness),
                ("HueSat", without_huesat),
                ("Look", without_look),
            ] {
                // Restore only the deliberately disabled field to check every
                // other adjustment byte against the original captured request.
                let mut restored = control.adjustments;
                match effect {
                    "sharpness" => restored.global.sharpness = 0.36,
                    "HueSat" => restored.global.dcp_huesat_dims = [2, 2, 2, 0],
                    "Look" => restored.global.dcp_look_dims = [2, 2, 2, 0],
                    _ => unreachable!("fixed three-effect control set"),
                }
                assert_eq!(
                    bytemuck::bytes_of(&restored),
                    bytemuck::bytes_of(&req.adjustments)
                );
                assert!(std::ptr::eq(control.mask_bitmaps, req.mask_bitmaps));
                assert!(Arc::ptr_eq(
                    control.lut.as_ref().unwrap(),
                    req.lut.as_ref().unwrap()
                ));
                let roi = control.roi.unwrap();
                assert_eq!((roi.x, roi.y, roi.width, roi.height), (3, 5, 13, 9));
                let captured = control.dcp_textures.as_ref().unwrap();
                assert_eq!(captured.huesat_dims, dcp.huesat_dims);
                assert_eq!(captured.look_dims, dcp.look_dims);
                assert_eq!(captured.huesat, dcp.huesat);
                assert_eq!(captured.look, dcp.look);
                assert_eq!(captured.tone_curve, dcp.tone_curve);
                let disabled = reference_engine(&context, &image, control, precision);
                assert_eq!(disabled.color(), expected.color());
                assert_eq!(disabled.dimensions(), expected.dimensions());
                assert_ne!(
                    disabled.as_bytes(),
                    expected.as_bytes(),
                    "fixed {effect} control must change at least one channel at {precision:?}"
                );
            }
            let actual = owner.render(&image, req, precision).unwrap();
            assert_eq!(actual.dimensions(), (13, 9));
            assert_eq!(actual.as_bytes(), expected.as_bytes());
            assert_eq!(
                Arc::strong_count(&lut),
                1,
                "owner must not retain captured LUT requests"
            );
            assert_eq!(lut.data, lut_before);
            assert_eq!(masks[0].as_raw(), &original_mask);
            assert_eq!(image.as_bytes(), original_image);
            assert!(shared_display.lock().unwrap().is_none());
            assert!(owner.context.display.lock().unwrap().is_none());
            assert!(pollster::block_on(scope.pop()).is_none());
        }
        finish_hardware(&context, &errors);
    }
}
// END ISOLATED CREATIVE OWNER.

/// Admission already blocks new legacy requests and has awaited their actual
/// leases/work. Move all source-bearing GPU owners out before destroying them.
pub(crate) fn retire_native_presentation(state: &AppState) -> Result<(), String> {
    let context = state
        .gpu_context
        .lock()
        .map_err(|_| "GPU context poisoned")?
        .clone();
    let processor = state
        .gpu_processor
        .lock()
        .map_err(|_| "GPU processor poisoned")?
        .take();
    let cache = state
        .gpu_image_cache
        .lock()
        .map_err(|_| "GPU cache poisoned")?
        .take();
    let fallback_device = processor
        .as_ref()
        .map(|p| Arc::clone(&p.processor.context.device));
    let result = if let Some(context) = context.as_ref() {
        let mut display = context
            .display
            .lock()
            .map_err(|_| "Native display poisoned")?
            .take();
        let result = if let Some(display) = display.as_mut() {
            let binding = display.current_bind_group.take();
            drop(binding);
            display.clear_source_frame(&context.device, &context.queue)
        } else {
            Ok(())
        };
        // No presentation/render/allocation happens while this slot is locked.
        *context
            .display
            .lock()
            .map_err(|_| "Native display poisoned")? = display;
        result
    } else {
        Ok(())
    };
    drop((processor, cache));
    result?;
    let device = context
        .as_ref()
        .map(|c| Arc::clone(&c.device))
        .or(fallback_device);
    if let Some(device) = device {
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(60)),
            })
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
impl WgpuDisplay {
    fn clear_source_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), String> {
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(device, &self.config);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                    _ => return Err("Native display retirement failed".into()),
                }
            }
            _ => return Err("Native display retirement failed".into()),
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Retire native source frame"),
        });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear retired native source"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: self.latest_transform.bg_primary[0] as f64,
                            g: self.latest_transform.bg_primary[1] as f64,
                            b: self.latest_transform.bg_primary[2] as f64,
                            a: self.latest_transform.bg_primary[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: NonZero::new(0),
            });
        }
        queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn preview_reuse_hardware_context() -> (GpuContext, Arc<std::sync::Mutex<Vec<String>>>) {
    isolated_creative_tests::hardware_context()
}

// Shared checked boundaries for the actual same-shader sampler.
pub(crate) fn point_colour_pixel(
    uv: [f32; 2],
    dimensions: (u32, u32),
) -> Result<(u32, u32), String> {
    if dimensions.0 == 0
        || dimensions.1 == 0
        || uv.iter().any(|v| !v.is_finite() || !(0.0..1.0).contains(v))
    {
        return Err("Point Colour sample is outside the composed image".into());
    }
    let pixel = (
        (uv[0] as f64 * dimensions.0 as f64).floor() as u32,
        (uv[1] as f64 * dimensions.1 as f64).floor() as u32,
    );
    if pixel.0 >= dimensions.0 || pixel.1 >= dimensions.1 {
        return Err("Invalid Point Colour coordinate".into());
    }
    Ok(pixel)
}

fn decode_point_colour_sample(
    bytes: &[u8],
) -> Result<crate::image_processing::PointColourSample, String> {
    if bytes.len() != 32 {
        return Err("Invalid Point Colour readback length".into());
    }
    let mut value = [0.0f32; 8];
    for (i, chunk) in bytes.chunks_exact(4).enumerate() {
        value[i] = f32::from_ne_bytes(chunk.try_into().unwrap());
    }
    if !value.iter().all(|v| v.is_finite())
        || value[3] != 1.0
        || value[7] != 1.0
        || !(0.0..360.0).contains(&value[4])
        || !(0.0..=1.0).contains(&value[5])
        || value[6] < 0.0
    {
        return Err("Invalid Point Colour float sample".into());
    }
    Ok(crate::image_processing::PointColourSample {
        linear_rgb: [value[0], value[1], value[2]],
        hue: value[4],
        saturation: value[5],
        value: value[6],
    })
}

#[cfg(test)]
mod point_colour_tests {
    use super::*;
    use crate::image_processing::{RenderOptions, get_all_adjustments_from_json};
    use serde_json::json;

    fn request(value: serde_json::Value) -> RenderRequest<'static> {
        RenderRequest {
            adjustments: get_all_adjustments_from_json(
                &value,
                false,
                crate::as_shot::AsShot::reference(),
                RenderOptions::default(),
            ),
            mask_bitmaps: &[],
            lut: None,
            roi: None,
            dcp_textures: None,
        }
    }
    fn finish(context: &GpuContext, errors: &std::sync::Mutex<Vec<String>>) {
        context
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(60)),
            })
            .unwrap();
        assert!(errors.lock().unwrap().is_empty());
    }
    #[test]
    fn pc06_checked_coordinates_and_readback_contract() {
        assert_eq!(
            point_colour_pixel([0.5, 0.25], (4000, 3000)).unwrap(),
            (2000, 750)
        );
        assert_eq!(point_colour_pixel([0.0, 0.0], (1, 1)).unwrap(), (0, 0));
        for uv in [
            [1.0, 0.0],
            [-0.01, 0.0],
            [f32::NAN, 0.0],
            [0.0, f32::INFINITY],
        ] {
            assert!(point_colour_pixel(uv, (4000, 3000)).is_err());
        }
        assert!(point_colour_pixel([0.0, 0.0], (0, 1)).is_err());
        let value = [2.0f32, 0.0, 0.0, 1.0, 0.0, 1.0, 1.353256, 1.0];
        let bytes = bytemuck::cast_slice(&value);
        assert_eq!(
            decode_point_colour_sample(bytes).unwrap().linear_rgb,
            [2.0, 0.0, 0.0]
        );
        for length in [0, 16, 31] {
            assert!(decode_point_colour_sample(&bytes[..length]).is_err());
        }
        let mut bad = value;
        bad[7] = 0.0;
        assert!(decode_point_colour_sample(bytemuck::cast_slice(&bad)).is_err());
        bad = value;
        bad[0] = f32::NAN;
        assert!(decode_point_colour_sample(bytemuck::cast_slice(&bad)).is_err());
    }

    #[test]
    #[ignore = "actual GPU; separate explicit grant required"]
    fn pc01_actual_shader_function_known_answers() {
        let (context, errors) = super::isolated_creative_tests::hardware_context();
        let scope = context
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        // Append only an invocation entrypoint: every colour function is the
        // exact production shader, not a Rust replica or copied shader prefix.
        let source = production_shader_source()
            + r#"
@compute @workgroup_size(1)
fn point_colour_known_answers(@builtin(global_invocation_id) id: vec3<u32>) {
  var p = PointColourUniform(0.0, 40.0, 0.0, 0.0, 0.0, 1u, 0u, 0u);
  var c = vec3<f32>(1.0, 0.0, 0.0);
  if (id.x == 0u) { p.luminance = 1.0; }
  if (id.x == 1u) { p.saturation = -1.0; }
  if (id.x == 2u) { p.state = 0u; c = vec3<f32>(-0.25, 2.0, 0.5); }
  if (id.x == 3u) { c = vec3<f32>(-0.25, 2.0, 0.5); }
  if (id.x == 4u) { c = vec3<f32>(0.25); p.hue = 30.0; p.saturation = 1.0; }
  if (id.x == 5u) { textureStore(point_sample, vec2<i32>(i32(id.x), 0), vec4<f32>(vec3<f32>(get_raw_hsl_influence(2.0, 358.0, 40.0)), 1.0)); return; }
  if (id.x == 6u) { p.hue = 30.0; }
  if (id.x == 7u) { c = vec3<f32>(4.0, 0.0, 0.0); p.luminance = 1.0; }
  textureStore(point_sample, vec2<i32>(i32(id.x), 0), vec4<f32>(apply_point_colour(c, p), 1.0));
}
"#;
        let shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("actual Point Colour function controls"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let pipeline = context
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: None,
                module: &shader,
                entry_point: Some("point_colour_known_answers"),
                compilation_options: Default::default(),
                cache: None,
            });
        let texture = context.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 8,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let group = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 17,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            });
        let mut encoder = context.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(8, 1, 1);
        }
        context.queue.submit(Some(encoder.finish()));
        let bytes = read_texture_data_roi(
            &context.device,
            &context.queue,
            &texture,
            wgpu::Origin3d::ZERO,
            wgpu::Extent3d {
                width: 8,
                height: 1,
                depth_or_array_layers: 1,
            },
            16,
        )
        .unwrap();
        let validation_error = pollster::block_on(scope.pop());
        assert!(
            validation_error.is_none(),
            "Point Colour GPU validation failed: {validation_error:?}"
        );
        let values: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_ne_bytes(b.try_into().unwrap()))
            .collect();
        let expected = [
            [2.0, 0.0, 0.0],
            [0.2126; 3],
            [-0.25, 2.0, 0.5],
            [-0.25, 2.0, 0.5],
            [0.25; 3],
            [0.9417645335842487; 3],
            [0.2126 / 0.9278, 0.2126 / 0.9278, 0.0],
            [8.0, 0.0, 0.0],
        ];
        for (i, expected) in expected.iter().enumerate() {
            for c in 0..3 {
                assert!(
                    (values[i * 4 + c] - expected[c]).abs() <= 0.00001,
                    "case {i} channel {c}: {}",
                    values[i * 4 + c]
                );
            }
        }
        for i in [2, 3] {
            assert_eq!(&values[i * 4..i * 4 + 3], &[-0.25, 2.0, 0.5]);
        }
        finish(&context, &errors);
    }

    #[test]
    #[ignore = "actual GPU; separate explicit grant required"]
    fn pc05_real_pipeline_sample_feedback_and_both_precisions() {
        let (context, errors) = super::isolated_creative_tests::hardware_context();
        let scope = context
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let image = DynamicImage::ImageRgb32F(image::Rgb32FImage::from_fn(33, 19, |x, y| {
            image::Rgb([0.25 + x as f32 / 128.0, 0.0625 + y as f32 / 128.0, 0.03125])
        }));
        let mut renderer = IsolatedCreativeRenderer::new(&context);
        let first = renderer
            .sample_point_colour(&image, request(json!({})), [0.5, 0.5])
            .unwrap();
        let point = json!({"pointColour":{"v":1,"centerHue":first.hue,"widthDegrees":40,"hue":30,"saturation":-20,"luminance":50},
            "colorGrading":{"global":{"hue":240,"saturation":80,"luminance":10}}});
        let second = renderer
            .sample_point_colour(&image, request(point.clone()), [0.5, 0.5])
            .unwrap();
        assert_eq!(
            first.linear_rgb.map(f32::to_bits),
            second.linear_rgb.map(f32::to_bits)
        );
        assert_eq!(first.hue.to_bits(), second.hue.to_bits());
        let upstream = renderer
            .sample_point_colour(&image, request(json!({"exposure":1})), [0.5, 0.5])
            .unwrap();
        assert_ne!(first.linear_rgb, upstream.linear_rgb);
        let edge = renderer
            .sample_point_colour(&image, request(json!({})), [32.5 / 33.0, 18.5 / 19.0])
            .unwrap();
        assert_ne!(first.linear_rgb, edge.linear_rgb);
        for precision in [
            RenderOutputPrecision::EightBit,
            RenderOutputPrecision::SixteenBit,
        ] {
            let mut req = request(point.clone());
            req.adjustments.global.point_probe = [16, 9, 1, 0];
            let mut observed = None;
            renderer
                .render_inner(&image, req, precision, Some(&mut observed))
                .unwrap();
            assert_eq!(
                observed.unwrap().linear_rgb.map(f32::to_bits),
                first.linear_rgb.map(f32::to_bits)
            );
            let off = renderer
                .render(&image, request(json!({})), precision)
                .unwrap();
            let adjusted = renderer
                .render(&image, request(point.clone()), precision)
                .unwrap();
            assert_ne!(off.as_bytes(), adjusted.as_bytes());
        }
        assert!(pollster::block_on(scope.pop()).is_none());
        finish(&context, &errors);
    }

    #[test]
    #[ignore = "actual GPU; separate explicit grant required"]
    fn pc06_real_owner_errors_neutral_and_reuse() {
        let (context, errors) = super::isolated_creative_tests::hardware_context();
        let scope = context
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let image = DynamicImage::ImageRgb32F(image::Rgb32FImage::from_pixel(
            5,
            3,
            image::Rgb([0.5, 0.125, 0.0]),
        ));
        let mut renderer = IsolatedCreativeRenderer::new(&context);
        assert!(
            renderer
                .sample_point_colour(&image, request(json!({})), [1.0, 0.0])
                .is_err()
        );
        assert_eq!(renderer.processor_build_count(), 0);
        assert!(
            renderer
                .sample_point_colour(&image, request(json!({"pointColour":{"v":2}})), [0.5, 0.5])
                .is_err()
        );
        assert_eq!(renderer.processor_build_count(), 0);
        let first = renderer
            .sample_point_colour(&image, request(json!({})), [0.5, 0.5])
            .unwrap();
        let gray =
            DynamicImage::ImageRgb32F(image::Rgb32FImage::from_pixel(5, 3, image::Rgb([0.25; 3])));
        assert!(
            renderer
                .sample_point_colour(&gray, request(json!({})), [0.5, 0.5])
                .unwrap_err()
                .contains("neutral")
        );
        let final_value = renderer
            .sample_point_colour(&image, request(json!({})), [0.5, 0.5])
            .unwrap();
        assert_eq!(first.linear_rgb, final_value.linear_rgb);
        assert_eq!(renderer.processor_build_count(), 1);
        assert!(pollster::block_on(scope.pop()).is_none());
        finish(&context, &errors);
    }
}

#[cfg(test)]
pub(crate) fn point_colour_test_context() -> (GpuContext, Arc<std::sync::Mutex<Vec<String>>>) {
    isolated_creative_tests::hardware_context()
}
