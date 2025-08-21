mod display;
mod gui;
mod load;
mod pbar;
mod scene;
mod utils;

use std::{cell::RefCell, sync::Arc};

use futures::AsyncRead;
use pbar::Progress;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use winit::{
    application::ApplicationHandler,
    event_loop::EventLoop,
    platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys, WindowExtWebSys},
    window::Window,
};

struct AppState {
    scene: Option<scene::Scene>,
    render_resolution: display::RenderResolution,
    supersample: u32,
    render_frame: display::RenderFrame,
    gui_state: egui_winit::State,
    last_frame_time: Option<f64>,
    subframe_count: u32,
    avg_frame_time: f64,
    panel_open: bool,
    chooser_open: bool,
    known_scenes: Vec<(&'static str, &'static str)>,
    file_hovered: bool,
    loading: bool,
    paused: bool,
    progress_bar: pbar::ProgressBar,
    error_message: Option<String>,
    azimuth: f32,
    elevation: f32,
    prev_mouse_pos: (f32, f32),
    mouse_dragging: bool,
    zoom: f32,
    stale_camera: bool,
}

impl AppState {
    fn begin_loading(&mut self) {
        self.file_hovered = false;
        self.chooser_open = false;
        self.loading = true;
        self.progress_bar.update_progress_sync(0.0);
        self.progress_bar
            .update_status_sync("fetching scene data".to_string());
    }
}

struct App {
    window: Arc<Window>,
    display: display::Display,
    state: RefCell<AppState>,
}

impl App {
    async fn new(window: Window, canvas: &HtmlCanvasElement) -> Self {
        let window = Arc::new(window);

        let display = display::Display::from_canvas(canvas).await;

        let egui_ctx = egui::Context::default();
        let gui_state =
            egui_winit::State::new(egui_ctx, egui::ViewportId::ROOT, &window, None, None, None);

        let known_scenes = vec![("/garden.tsplat", "garden")];

        let render_resolution = display::RenderResolution::Ws1080P;
        let supersample = 1;
        let render_frame = display.create_render_frame(&render_resolution, supersample);

        let state = RefCell::new(AppState {
            scene: None,
            render_resolution,
            supersample,
            render_frame,
            gui_state,
            last_frame_time: None,
            subframe_count: 1,
            avg_frame_time: 1.0 / 60.0,
            panel_open: true,
            chooser_open: true,
            known_scenes,
            file_hovered: false,
            loading: false,
            paused: false,
            progress_bar: pbar::make_progress_bar(),
            error_message: None,
            azimuth: -0.72,
            elevation: 0.32,
            prev_mouse_pos: (0.0, 0.0),
            mouse_dragging: false,
            zoom: 3.0,
            stale_camera: true,
        });

        App {
            window,
            display,
            state,
        }
    }
}

trait AppLogic {
    async fn load_scene<S: AsyncRead + Unpin>(&self, byte_stream: S) -> Result<(), String>;

    async fn load_url(&self, url: String) -> Result<(), String>;

    fn show_err(&self, err_string: String);
}

impl AppLogic for Arc<App> {
    async fn load_scene<S: AsyncRead + Unpin>(&self, byte_stream: S) -> Result<(), String> {
        let app = self.clone();
        let pbar = app.state.borrow().progress_bar.clone();

        let tsplat = load::read_tsplat(byte_stream, pbar.clone()).await?;

        let new_scene = scene::Scene::new(tsplat, &app.display, pbar).await?;

        let state = &mut app.state.borrow_mut();
        state.scene = Some(new_scene);
        state.loading = false;
        state.stale_camera = true;

        Ok(())
    }

    async fn load_url(&self, url: String) -> Result<(), String> {
        let app = self.clone();
        let response =
            wasm_bindgen_futures::JsFuture::from(web_sys::window().unwrap().fetch_with_str(&url))
                .await
                .map_err(|_| "could not fetch scene data".to_string())?;
        let response = response.dyn_into::<web_sys::Response>().unwrap();

        if !response.ok() {
            return Err(format!(
                "could not fetch scene data: {}",
                response.status_text()
            ));
        }

        let stream = response
            .body()
            .ok_or("could not fetch scene data".to_string())?;
        let stream = wasm_streams::ReadableStream::from_raw(stream);

        app.load_scene(stream.into_async_read()).await
    }

    fn show_err(&self, err_string: String) {
        let state = &mut self.state.borrow_mut();
        state.loading = false;
        state.error_message = Some(err_string);
    }
}

struct HandlerInner {
    app: Option<Arc<App>>,
}

struct Handler(Arc<RefCell<HandlerInner>>);

impl ApplicationHandler for Handler {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let browser_window = web_sys::window().unwrap();
        let canvas = browser_window
            .document()
            .unwrap()
            .get_element_by_id("canvas")
            .map(|c| c.dyn_into::<web_sys::HtmlCanvasElement>().unwrap())
            .unwrap();

        let attributes = Window::default_attributes().with_canvas(Some(canvas.clone()));

        let window = event_loop.create_window(attributes).unwrap();
        window.set_prevent_default(true);

        let handler = self.0.clone();
        let init_future = async move {
            let app_orig = Arc::new(App::new(window, &canvas).await);
            handler.borrow_mut().app = Some(app_orig.clone());

            let app = app_orig.clone();
            canvas.set_ondrop(Some(
                Closure::<dyn FnMut(web_sys::DragEvent)>::new(move |ev: web_sys::DragEvent| {
                    ev.prevent_default();
                    let dt = ev.data_transfer().unwrap();
                    let items = dt.items();
                    if let Some(item) = items.get(0) {
                        if let Ok(Some(file)) = item.get_as_file() {
                            app.state.borrow_mut().begin_loading();

                            let name = file.name();
                            web_sys::console::log_1(&name.into());

                            let stream = wasm_streams::ReadableStream::from_raw(file.stream());

                            let app = app.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                if let Err(err_string) =
                                    app.load_scene(stream.into_async_read()).await
                                {
                                    app.show_err(err_string);
                                }
                            });
                        }
                    }
                })
                .into_js_value()
                .unchecked_ref(),
            ));

            let app = app_orig.clone();
            canvas.set_ondragenter(Some(
                Closure::<dyn FnMut(web_sys::DragEvent)>::new(move |ev: web_sys::DragEvent| {
                    ev.prevent_default();
                    app.state.borrow_mut().file_hovered = true;
                })
                .into_js_value()
                .unchecked_ref(),
            ));

            let app = app_orig.clone();
            canvas.set_ondragleave(Some(
                Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
                    ev.prevent_default();
                    app.state.borrow_mut().file_hovered = false;
                })
                .into_js_value()
                .unchecked_ref(),
            ));

            canvas.set_ondragover(Some(
                Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
                    ev.prevent_default();
                })
                .into_js_value()
                .unchecked_ref(),
            ));

            app_orig.window.request_redraw();
        };

        wasm_bindgen_futures::spawn_local(init_future);
    }

    fn window_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        if let Some(app) = &self.0.borrow().app {
            let _ = app
                .state
                .borrow_mut()
                .gui_state
                .on_window_event(&app.window, &event)
                .consumed;

            match event {
                winit::event::WindowEvent::CursorMoved {
                    device_id: _,
                    position,
                } => {
                    let new_x = position.x as f32;
                    let new_y = position.y as f32;
                    let mut state = app.state.borrow_mut();

                    if state.mouse_dragging {
                        let delta_x = new_x - state.prev_mouse_pos.0;
                        let delta_y = new_y - state.prev_mouse_pos.1;
                        state.azimuth -= delta_x * 0.01;
                        state.elevation += delta_y * 0.01;
                        if delta_x.abs() > 0.1 || delta_y.abs() > 0.1 {
                            state.stale_camera = true;
                        }
                    }

                    state.prev_mouse_pos = (new_x, new_y);
                }
                winit::event::WindowEvent::MouseWheel {
                    device_id: _,
                    delta,
                    phase: _,
                } => {
                    let mut state = app.state.borrow_mut();
                    match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => {
                            state.zoom *= 1.01f32.powf(y as f32);
                        }
                        winit::event::MouseScrollDelta::PixelDelta(pos) => {
                            state.zoom *= 1.01f32.powf(-pos.y as f32 / 10.0);
                        }
                    }
                    state.stale_camera = true;
                }
                winit::event::WindowEvent::MouseInput {
                    device_id: _,
                    state,
                    button,
                } => {
                    let mut appstate = app.state.borrow_mut();
                    if button == winit::event::MouseButton::Left {
                        if state.is_pressed() {
                            appstate.mouse_dragging = true;
                        } else {
                            appstate.mouse_dragging = false;
                        }
                    }
                }
                winit::event::WindowEvent::RedrawRequested => {
                    let prev_res = app.state.borrow().render_resolution.clone();
                    let prev_supersample = app.state.borrow().supersample;
                    let (platform_output, gui_render_data) = gui::show_gui(app);
                    app.state
                        .borrow_mut()
                        .gui_state
                        .handle_platform_output(&app.window, platform_output);

                    let new_res = app.state.borrow().render_resolution.clone();
                    let new_supersample = app.state.borrow().supersample;
                    if new_res != prev_res || new_supersample != prev_supersample {
                        let new_frame = app.display.create_render_frame(&new_res, new_supersample);
                        let mut state = app.state.borrow_mut();
                        state.render_frame = new_frame;
                        state.stale_camera = true;
                    }

                    let paused = app.state.borrow().paused;
                    if let Some(scene) = &mut app.state.borrow_mut().scene {
                        if !paused {
                            scene.t += 1;
                        }
                    }
                    let canvas_width = app.window.inner_size().width;
                    let canvas_height = app.window.inner_size().height;
                    let subframe_count = app.state.borrow().subframe_count;
                    let stale_camera = app.state.borrow().stale_camera;
                    let mut state = app.state.borrow_mut();
                    state.stale_camera = false;
                    app.display.render(
                        gui_render_data,
                        &mut state,
                        canvas_width,
                        canvas_height,
                        subframe_count,
                        stale_camera,
                    );

                    let app = app.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        app.window.request_redraw();
                    });
                }
                _ => (),
            }
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn run() {
    console_error_panic_hook::set_once();
    let event_loop = EventLoop::new().unwrap();
    let handler = Handler(Arc::new(RefCell::new(HandlerInner { app: None })));
    event_loop.spawn_app(handler);
}
