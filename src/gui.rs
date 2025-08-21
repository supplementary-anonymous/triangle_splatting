use std::sync::Arc;

use egui::Tooltip;
use wgpu::Backend;

use crate::{App, AppLogic, AppState, display::RenderResolution, pbar::Progress};

pub struct GuiRenderData {
    pub textures_delta: egui::TexturesDelta,
    pub shapes: Vec<egui::epaint::ClippedShape>,
    pub pixels_per_point: f32,
}

pub fn show_gui(app: &Arc<App>) -> (egui::PlatformOutput, GuiRenderData) {
    let backend = app.display.backend;
    let mut borrow = app.state.borrow_mut();
    let state: &mut AppState = &mut borrow;

    let raw_input = state.gui_state.take_egui_input(&app.window);
    let now = raw_input.time.unwrap_or(0.0);
    let dt = if let Some(last_frame_time) = state.last_frame_time {
        (now - last_frame_time) / state.subframe_count as f64
    } else {
        0.015
    };
    state.last_frame_time = Some(now);
    state.avg_frame_time = 0.9 * state.avg_frame_time + 0.1 * dt;

    let real_frame_time = state.avg_frame_time * state.subframe_count as f64;
    if real_frame_time < 0.018 && state.scene.is_some() {
        state.subframe_count += 1;
    } else if real_frame_time > 0.025 && state.subframe_count > 1 {
        state.subframe_count -= 1;
    }

    let real_size = app.window.inner_size();
    let width = real_size.width;
    let height = real_size.height;

    let egui::FullOutput {
        platform_output,
        textures_delta,
        shapes,
        pixels_per_point,
        viewport_output: _,
    } = state.gui_state.egui_ctx().run(raw_input, |ctx| {
        ctx.style_mut(|style| {
            style.override_font_id = Some(egui::FontId {
                size: 14.0f32,
                family: egui::FontFamily::Monospace,
            });
            style.visuals = egui::Visuals::dark();
        });

        egui::Window::new("panel")
            .default_width(0.0)
            .resizable(false)
            .anchor(egui::Align2::LEFT_TOP, [0.0, 0.0])
            .title_bar(false)
            .frame(egui::Frame::side_top_panel(&ctx.style()).multiply_with_opacity(0.95))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let file_symbol = if state.file_hovered {
                        egui::RichText::new("🗁")
                            .size(28.0)
                            .color(egui::Color32::WHITE)
                    } else {
                        egui::RichText::new("🗀").size(28.0)
                    };
                    if ui
                        .add(egui::widgets::Button::new(file_symbol).frame(false))
                        .clicked()
                    {
                        state.chooser_open = !state.chooser_open;
                    }
                    if ui
                        .add(
                            egui::widgets::Button::new(egui::RichText::new("➕").size(28.0))
                                .frame(false),
                        )
                        .clicked()
                    {
                        egui::gui_zoom::zoom_in(ctx);
                    }
                    if ui
                        .add(
                            egui::widgets::Button::new(egui::RichText::new("➖").size(28.0))
                                .frame(false),
                        )
                        .clicked()
                    {
                        egui::gui_zoom::zoom_out(ctx);
                    }
                    let playpause_symbol = if state.paused { "▶" } else { "⏸" };
                    if ui
                        .add(
                            egui::widgets::Button::new(
                                egui::RichText::new(playpause_symbol).size(28.0),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        state.paused = !state.paused;
                    }
                    if ui
                        .add(
                            egui::widgets::Button::new(egui::RichText::new("⛶").size(28.0))
                                .frame(false),
                        )
                        .clicked()
                    {
                        if app.window.fullscreen().is_some() {
                            app.window.set_fullscreen(None);
                        } else {
                            app.window
                                .set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                        }
                    }
                    let min_symbol = if state.panel_open { "➖" } else { "🗖" };
                    if ui
                        .add(
                            egui::widgets::Button::new(egui::RichText::new(min_symbol).size(28.0))
                                .frame(false),
                        )
                        .clicked()
                    {
                        state.panel_open = !state.panel_open;
                    }
                });

                if !state.panel_open {
                    return;
                }

                ui.separator();
                ui.vertical_centered(|ui| {
                    ui.label("status:");
                });
                ui.separator();

                egui::Grid::new("status_grid").show(ui, |ui| {
                    ui.label("backend:");
                    if backend == Backend::Gl {
                        let res = ui.label(egui::RichText::new("WebGL ⚠").color(egui::Color32::RED));
                        if res.contains_pointer() {
                            Tooltip::for_widget(&res)
                                .popup
                                .show(|ui| {
                                    ui.label("Use a device that supports WebGPU for better performance.");
                                });
                        }
                    } else {
                        ui.label(format!("{}", backend));
                    }
                    ui.end_row();


                    ui.label("resolution:");
                    egui::ComboBox::new("resolution", "")
                        .selected_text(state.render_resolution.to_string())
                        .width(20.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut state.render_resolution,
                                RenderResolution::Ws360P,
                                RenderResolution::Ws360P.to_string(),
                            );
                            ui.selectable_value(
                                &mut state.render_resolution,
                                RenderResolution::Ws720P,
                                RenderResolution::Ws720P.to_string(),
                            );
                            ui.selectable_value(
                                &mut state.render_resolution,
                                RenderResolution::Ws1080P,
                                RenderResolution::Ws1080P.to_string(),
                            );
                            ui.selectable_value(
                                &mut state.render_resolution,
                                RenderResolution::Ws1440P,
                                RenderResolution::Ws1440P.to_string(),
                            );
                            ui.selectable_value(
                                &mut state.render_resolution,
                                RenderResolution::Ws2160P,
                                RenderResolution::Ws2160P.to_string(),
                            );
                            ui.selectable_value(
                                &mut state.render_resolution,
                                RenderResolution::Native(width, height),
                                format!("native: {}", RenderResolution::Native(width, height)),
                            );
                        });
                    ui.end_row();

                    ui.label("samples:");
                    egui::ComboBox::new("samples", "")
                        .selected_text((state.supersample * state.supersample).to_string())
                        .width(20.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.supersample, 1, "1".to_owned());
                            ui.selectable_value(&mut state.supersample, 2, "4".to_owned());
                            ui.selectable_value(&mut state.supersample, 3, "9".to_owned());
                            ui.selectable_value(&mut state.supersample, 4, "16".to_owned());
                            ui.selectable_value(&mut state.supersample, 5, "25".to_owned());
                        });
                    ui.end_row();

                    let frame_rate = 1.0 / state.avg_frame_time;

                    ui.label("frame rate:");
                    ui.label(format!("{:.0}/s", frame_rate));
                    ui.end_row();

                    let res = ui.link("subframes:");
                    if res.contains_pointer() {
                        Tooltip::for_widget(&res)
                            .popup
                            .show(|ui| {
                                ui.label("Browser frame rates are typically limited by the screen refresh rate, so we allow drawing multiple frames between each refresh.");
                            });
                    }
                    ui.label(format!("{}", state.subframe_count));
                    ui.end_row();
                });

                ui.separator();
                ui.vertical_centered(|ui| {
                    ui.label("controls:");
                    ui.label("click+drag to rotate");
                    ui.label("scroll to zoom");
                });
                ui.separator();
            });

        let mut chooser_open = state.chooser_open;
        if state.chooser_open {
            egui::Window::new("open file")
                .title_bar(true)
                .open(&mut chooser_open)
                .collapsible(false)
                .resizable([false, false])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label("\nchoose a scene:\n");
                        let mut selected_url = "";
                        ui.horizontal(|ui| {
                            ui.add_space(100.0);
                            egui::ComboBox::from_label("scene")
                                .selected_text("")
                                .show_ui(ui, |ui| {
                                    for &(url, name) in &state.known_scenes {
                                        ui.selectable_value(&mut selected_url, url, name);
                                    }
                                });
                        });
                        ui.label("\nNote that this scene is displayed at a reduced fidelity to fit within github size limits.\n");

                        if selected_url != "" {
                            let app = app.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                app.state.borrow_mut().begin_loading();
                                if let Err(err_string) =
                                    app.load_url(selected_url.to_string()).await
                                {
                                    app.show_err(err_string);
                                }
                            });
                        }
                    });
                });
        }
        state.chooser_open = chooser_open;

        if state.loading {
            egui::Window::new("loading")
                .title_bar(false)
                .resizable([false, false])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add(
                            egui::ProgressBar::new(state.progress_bar.get_progress()).animate(true),
                        );
                        ui.label(format!("\n{}…\n", state.progress_bar.get_status()));
                    });
                });
        }

        let mut error_open = state.error_message.is_some();
        if error_open {
            egui::Window::new("error")
                .title_bar(true)
                .open(&mut error_open)
                .collapsible(false)
                .resizable([false, false])
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(format!(
                            "\n{}\n",
                            state.error_message.as_ref().unwrap_or(&"".to_string())
                        ));
                    });
                });
        }
        if !error_open {
            state.error_message = None;
        }
    });

    (
        platform_output,
        GuiRenderData {
            textures_delta,
            shapes,
            pixels_per_point,
        },
    )
}
