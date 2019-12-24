use std::panic::{set_hook, take_hook};
use std::{io, time};

use snafu::{OptionExt, ResultExt, Snafu};
use ultraviolet as uv;
use winit::{
    event,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::audio::{
    connection::{ConnectionTarget, MasterChannel},
    mixer, playback,
};
use crate::config;
use crate::panic;
use crate::state::{self, State};

// Errors are usually used when the app should quit
#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("Failed to create window: {}", source))]
    WindowCreation { source: winit::error::OsError },

    #[snafu(display("No sufficient graphics card available!"))]
    AdapterSelection,

    #[snafu(display("Failed to create master audio player: {}", source))]
    MasterCreation { source: playback::CreateError },
}

fn load_state(state_file: Option<&str>) -> state::State {
    match state_file {
        Some(path) => match State::from_file(path) {
            Ok((state, warnings)) => {
                for w in warnings {
                    log::warn!("{}", w);
                }
                log::info!("Loaded project from {}", path);
                state
            }
            Err(state::ReadError::OpenError { ref source, .. })
                if source.kind() == io::ErrorKind::NotFound =>
            {
                log::warn!("Project not found, writing default...");
                let state = State::default();
                if let Err(e) = state.write(path) {
                    log::warn!("Failed to write new project: {}", e);
                } else {
                    log::debug!("Created new project at {}", path);
                }
                state
            }
            Err(e) => {
                log::error!("Failed to load project: {}", e);
                State::default()
            }
        },
        None => State::default(),
    }
}

fn preview_transform(window_res: (u32, u32), scope_res: (u32, u32)) -> uv::Mat4 {
    let window_ratio = window_res.0 as f32 / window_res.1 as f32;
    let scope_ratio = scope_res.0 as f32 / scope_res.1 as f32;

    if scope_ratio > window_ratio {
        // letterbox
        uv::Mat4::from_nonuniform_scale(uv::Vec4::new(1.0, window_ratio / scope_ratio, 1.0, 1.0))
    } else {
        // pillarbox
        uv::Mat4::from_nonuniform_scale(uv::Vec4::new(scope_ratio / window_ratio, 1.0, 1.0, 1.0))
    }
}

fn _run(state_file: Option<&str>) -> Result<(), Error> {
    set_hook(panic::dialog(take_hook()));

    // load config
    let config = config::Config::load();
    let mut state = load_state(state_file);

    // create window
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_inner_size((1600, 900).into())
        .with_title("rawrscope")
        .with_resizable(true)
        .build(&event_loop)
        .context(WindowCreation)?;
    let mut window_size = window.inner_size().to_physical(window.hidpi_factor());

    // initialize wgpu adapter and device
    let surface = wgpu::Surface::create(&window);
    let adapter = wgpu::Adapter::request(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance, // maybe do not request high perf
        backends: wgpu::BackendBit::PRIMARY,
    })
    .context(AdapterSelection)?;

    let (device, mut queue) = adapter.request_device(&wgpu::DeviceDescriptor {
        extensions: wgpu::Extensions {
            anisotropic_filtering: false,
        },
        limits: wgpu::Limits::default(),
    });

    // create swapchain
    let mut swap_desc = wgpu::SwapChainDescriptor {
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: window_size.width as u32,
        height: window_size.height as u32,
        present_mode: wgpu::PresentMode::NoVsync,
    };
    let mut swapchain = device.create_swap_chain(&surface, &swap_desc);

    // create and configure master player
    let mut master = playback::Player::new(&config).context(MasterCreation)?;

    let mut mixer_config = mixer::MixerBuilder::new();
    mixer_config.channels(master.channels() as usize);
    mixer_config.target_sample_rate(master.sample_rate());

    for source in state.audio_sources.iter_mut().filter_map(|s| s.as_loaded()) {
        if source
            .connections
            .iter()
            .any(|conn| conn.target.is_master())
        {
            let sample_rate = source.spec().sample_rate;
            mixer_config.source_rate(sample_rate);
        }
    }

    if let Err(e) = master.rebuild_mixer(mixer_config) {
        log::warn!("Failed to rebuild master mixer: {}", e);
    }

    // initialize imgui
    let mut imgui = imgui::Context::create();
    let mut imgui_plat = imgui_winit_support::WinitPlatform::init(&mut imgui);
    imgui_plat.attach_window(
        imgui.io_mut(),
        &window,
        imgui_winit_support::HiDpiMode::Default,
    );
    imgui.set_ini_filename(None);

    let font_size = (13.0 * window.hidpi_factor()) as f32;
    imgui.io_mut().font_global_scale = (1.0 / window.hidpi_factor()) as f32;

    imgui
        .fonts()
        .add_font(&[imgui::FontSource::DefaultFontData {
            config: Some(imgui::FontConfig {
                oversample_h: 1,
                pixel_snap_h: true,
                size_pixels: font_size,
                ..Default::default()
            }),
        }]);

    let mut imgui_renderer =
        imgui_wgpu::Renderer::new_static(&mut imgui, &device, &mut queue, swap_desc.format, None);

    let mut scope_renderer = crate::render::Renderer::new(&device);
    let preview_renderer = crate::render::quad::QuadRenderer::new(
        &device,
        &scope_renderer.texture_view(),
        swap_desc.format,
        preview_transform(
            window
                .inner_size()
                .to_physical(window.hidpi_factor())
                .into(),
            (1920, 1080),
        ),
    );

    let frame_secs = 1.0 / state.appearance.framerate as f32;
    let frame_duration = time::Duration::from_secs_f32(frame_secs);
    let buffer_duration =
        time::Duration::from_secs_f32(config.audio.buffer_ms.unwrap_or(10.0) / 1000.0); // TODO make defualt buffer size more clear to end user
    let mut timer = time::Instant::now() - buffer_duration;
    let window_ms = 50; // TODO remove hardcode

    event_loop.run(move |event, _, control_flow| {
        let sub_builder = master.submission_builder(); // TODO optimize

        imgui_plat.handle_event(imgui.io_mut(), &window, &event);

        match event {
            event::Event::WindowEvent { event, .. } => match event {
                event::WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                event::WindowEvent::Resized(_) => {
                    window_size = window.inner_size().to_physical(window.hidpi_factor());

                    swap_desc.width = window_size.width as u32;
                    swap_desc.height = window_size.height as u32;

                    swapchain = device.create_swap_chain(&surface, &swap_desc);

                    preview_renderer.update_transform(
                        &device,
                        &mut queue,
                        preview_transform(
                            window
                                .inner_size()
                                .to_physical(window.hidpi_factor())
                                .into(),
                            (1920, 1080),
                        ),
                    );
                }
                _ => {}
            },
            event::Event::EventsCleared => {
                *control_flow = ControlFlow::Poll;

                // create encoder early
                let mut encoder: wgpu::CommandEncoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { todo: 0 });

                // update ui
                imgui_plat
                    .prepare_frame(imgui.io_mut(), &window)
                    .expect("Failed to prepare UI rendering"); // TODO do not expect (need to figure out err handling in event loop)

                let im_ui = imgui.frame();
                crate::ui::ui(&mut state, &im_ui);

                let now = time::Instant::now();
                if now > timer {
                    // create audio submission
                    let mut sub = sub_builder.create(frame_secs);

                    let f = state.playback.frame;
                    let framerate = state.appearance.framerate;
                    let mut loaded_sources = state
                        .audio_sources
                        .iter_mut()
                        .filter_map(|s| s.as_loaded())
                        .collect::<Vec<_>>();

                    let sources_exhausted = loaded_sources
                        .iter()
                        .all(|source| f > source.len() / (source.spec().sample_rate / framerate));

                    if !sources_exhausted && state.playback.playing {
                        for source in &mut loaded_sources {
                            let channels = source.spec().channels;
                            let sample_rate = source.spec().sample_rate;

                            let window_len = sample_rate * window_ms / 1000 * u32::from(channels);
                            let window_pos = (sample_rate / framerate) * state.playback.frame;

                            // TODO dont panic
                            let window = source
                                .chunk_at(window_pos, window_len as usize)
                                .unwrap()
                                .iter()
                                .copied()
                                .collect::<Vec<_>>();

                            let chunk_len = sub
                                .length_of_channel(sample_rate)
                                .expect("submission missing sample rate!")
                                * channels as usize;

                            let chunk = &window[0..chunk_len.min(window.len())];

                            for conn in source.connections {
                                let channel_iter = chunk
                                    .iter()
                                    .skip(conn.channel as usize)
                                    .step_by(channels as usize)
                                    .copied();
                                match conn.target {
                                    ConnectionTarget::Master { ref channel } => {
                                        sub.add(
                                            sample_rate,
                                            match channel {
                                                MasterChannel::Left => 0,
                                                MasterChannel::Right => 1,
                                            },
                                            channel_iter,
                                        );
                                    }
                                    _ => log::warn!("scope connections unimplemented"),
                                }
                            }
                        }
                    } else if sources_exhausted && state.playback.playing {
                        state.playback.playing = false;
                    }

                    if let Err(e) = master.submit(sub) {
                        log::error!("Failed to submit audio to master: {}", e);
                    }

                    // render scopes
                    scope_renderer.render(&mut encoder);

                    if state.playback.playing {
                        state.playback.frame += 1;
                    }

                    timer += frame_duration;
                    if now.saturating_duration_since(timer) > buffer_duration {
                        timer = now - buffer_duration;
                    }
                }

                // begin rendering
                let swap_frame = swapchain.get_next_texture();

                // clear screen
                {
                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                            attachment: &swap_frame.view,
                            resolve_target: None,
                            load_op: wgpu::LoadOp::Clear,
                            store_op: wgpu::StoreOp::Store,
                            clear_color: wgpu::Color {
                                r: 0.3,
                                g: 0.3,
                                b: 0.3,
                                a: 1.0,
                            },
                        }],
                        depth_stencil_attachment: None,
                    });
                }

                // copy scopes to screen
                preview_renderer.render(&mut encoder, &swap_frame.view);

                // render ui
                imgui_plat.prepare_render(&im_ui, &window);
                imgui_renderer
                    .render(im_ui.render(), &device, &mut encoder, &swap_frame.view)
                    .expect("Failed to render UI"); // TODO do not expect

                queue.submit(&[encoder.finish()]);
            }
            _ => {}
        }
    });
}

pub fn run(state_file: Option<&str>) {
    if let Err(e) = _run(state_file) {
        log::error!("{}", e)
    }
}
