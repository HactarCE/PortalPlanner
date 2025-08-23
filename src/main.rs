use egui::{Rect, emath::GuiRounding};
use serde::{Deserialize, Serialize};

mod entity;
mod portal;
mod pos;
mod region;

pub use entity::Entity;
pub use portal::{Direction, Portal};
pub use pos::{Axis, BlockPos, WorldPos};
pub use region::{BlockRegion, WorldRegion};

pub const SCROLL_SENSITIVITY: f32 = 0.25;
pub const SAVE_FILE_PATH: &str = "world.json";
pub const PLOT_MARGIN: f32 = 8.0;

fn main() -> eframe::Result {
    eframe::run_native(
        "Portal Tool",
        eframe::NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

pub struct App {
    world: World,
    camera: Camera,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let world = std::fs::read(SAVE_FILE_PATH)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();

        Self {
            world,
            camera: Camera::default(),
        }
    }

    fn save_to_file(&self) {
        if let Ok(bytes) = serde_json::to_vec(&self.world) {
            match std::fs::write(SAVE_FILE_PATH, &bytes) {
                Ok(()) => (),
                Err(e) => eprintln!("error saving {SAVE_FILE_PATH:?}: {e}"),
            }
        }
    }
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_to_file();
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        for e in &mut raw_input.events {
            if let egui::Event::MouseWheel { delta, .. } = e {
                *delta *= SCROLL_SENSITIVITY;
            }
        }
    }

    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut new_camera = self.camera;

            let r = ui.available_rect_before_wrap();
            let center = r.center().round_ui();
            let size = (r.size() / 2.0).floor_ui();
            let xy = Rect::from_two_pos(center + egui::vec2(-size.x, size.y), center);
            let zy = Rect::from_two_pos(center + egui::vec2(size.x, size.y), center);
            let xz = Rect::from_two_pos(center + egui::vec2(-size.x, -size.y), center);
            let plots = [
                (ViewSlice::XY, xy),
                (ViewSlice::ZY, zy),
                (ViewSlice::XZ, xz),
            ];
            for (i, (view_slice, rect)) in plots.into_iter().enumerate() {
                ui.put(rect.shrink(PLOT_MARGIN), |ui: &mut egui::Ui| {
                    let points = egui_polygon(
                        BlockRegion {
                            min: BlockPos {
                                x: -1,
                                y: 60,
                                z: -3,
                            },
                            max: BlockPos { x: -1, y: 65, z: 5 },
                        }
                        .into(),
                        view_slice,
                    );
                    let polygon = egui_plot::Polygon::new("", points)
                        .fill_color(egui::Color32::PURPLE.gamma_multiply(0.2))
                        .stroke((3.0, egui::Color32::PURPLE));

                    let mut plot = egui_plot::Plot::new(format!("plot_{i}"))
                        .x_axis_label(match view_slice {
                            ViewSlice::XY | ViewSlice::XZ => "X",
                            ViewSlice::ZY => "Z",
                        })
                        .y_axis_label(match view_slice {
                            ViewSlice::XY | ViewSlice::ZY => "Y",
                            ViewSlice::XZ => "Z",
                        })
                        .x_grid_spacer(egui_plot::log_grid_spacer(8))
                        .y_grid_spacer(egui_plot::log_grid_spacer(8))
                        .data_aspect(1.0)
                        .x_axis_formatter(|mark, _range| mark.value.to_string())
                        .y_axis_formatter(|mark, _range| {
                            let y = mark.value;
                            if ViewSlice::XZ == view_slice { -y } else { y }.to_string()
                        })
                        .allow_axis_zoom_drag(false);

                    plot = match view_slice {
                        ViewSlice::XY => plot,
                        ViewSlice::XZ => plot.x_axis_position(egui_plot::VPlacement::Top),
                        ViewSlice::ZY => plot.y_axis_position(egui_plot::HPlacement::Right),
                    };

                    let r = plot.show(ui, |plot_ui| {
                        // Compute plot bounds from camera
                        let mut bounds_from_camera = egui_plot::PlotBounds::NOTHING;
                        let [x, y] = match view_slice {
                            ViewSlice::XY => [self.camera.pos.x, self.camera.pos.y],
                            ViewSlice::XZ => [self.camera.pos.x, self.camera.pos.z],
                            ViewSlice::ZY => [self.camera.pos.z, self.camera.pos.y],
                        };
                        let old_width = plot_ui.plot_bounds().width();
                        let old_height = plot_ui.plot_bounds().width();
                        let new_height = self.camera.height / old_height * old_width;
                        bounds_from_camera.set_x_center_width(x, new_height);
                        bounds_from_camera.set_y_center_height(y, self.camera.height);

                        plot_ui.set_plot_bounds(bounds_from_camera);
                        plot_ui.add(polygon);
                    });

                    // Update camera on interaction with plot
                    if r.response.hovered() || r.response.dragged() {
                        let bounds = r.transform.bounds();
                        let egui_plot::PlotPoint { x, y } = bounds.center();
                        match view_slice {
                            ViewSlice::XY => (new_camera.pos.x, new_camera.pos.y) = (x, y),
                            ViewSlice::XZ => (new_camera.pos.x, new_camera.pos.z) = (x, y),
                            ViewSlice::ZY => (new_camera.pos.z, new_camera.pos.y) = (x, y),
                        }
                        new_camera.width = bounds.width();
                        new_camera.height = bounds.height();
                    }

                    r.response
                });
            }

            self.camera = new_camera;
        });
    }
}

fn egui_polygon(region: WorldRegion, slice: ViewSlice) -> Vec<[f64; 2]> {
    let WorldRegion { min, max } = region;
    let [[x1, y1], [x2, y2]] = match slice {
        ViewSlice::XY => [[min.x, min.y], [max.x, max.y]],
        ViewSlice::XZ => [[min.x, -min.z], [max.x, -max.z]],
        ViewSlice::ZY => [[min.z, min.y], [max.z, max.y]],
    };
    vec![[x1, y1], [x1, y2], [x2, y2], [x2, y1]]
}

/// Slice of the world to view.
#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ViewSlice {
    #[default]
    XY,
    XZ,
    ZY,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct World {
    pub portals: WorldPortals,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct WorldPortals {
    pub overworld: Vec<Portal>,
    pub nether: Vec<Portal>,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq)]
pub struct Camera {
    /// Position of the center of the viewport.
    pub pos: WorldPos,
    /// Width of viewport, measured in world coordinates.
    pub width: f64,
    /// Height of viewport, measured in world coordinates.
    pub height: f64,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            pos: WorldPos {
                x: 0.0,
                y: 64.0,
                z: 0.0,
            },
            width: 100.0,
            height: 100.0,
        }
    }
}
