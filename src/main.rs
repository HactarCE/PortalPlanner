use egui::Widget;
use egui::emath::GuiRounding;

mod entity;
mod portal;
mod pos;
mod region;
mod view;
mod world;

pub use entity::Entity;
pub use portal::{Direction, Portal, PortalAxis};
pub use pos::{Axis, BlockPos, WorldPos};
pub use region::{BlockRegion, WorldRegion};
pub use view::{Camera, ViewSlice};
pub use world::{Dimension, World, WorldPortals};

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

    dimension: Dimension,
    lock_portal_size: bool,
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

            dimension: Dimension::Overworld,
            lock_portal_size: true,
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

    fn show_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for dim in [Dimension::Overworld, Dimension::Nether] {
                ui.selectable_value(&mut self.dimension, dim, dim.to_string());
            }
        });
        if ui.button("Add portal pair").clicked() {
            self.add_portal_in_overworld();
            self.add_portal_in_nether();
        }
        ui.checkbox(&mut self.lock_portal_size, "Lock portal size");
        ui.columns(2, |uis| {
            self.show_portal_list(&mut uis[0], Dimension::Overworld);
            self.show_portal_list(&mut uis[1], Dimension::Nether);
        });
    }

    fn show_portal_list(&mut self, ui: &mut egui::Ui, dimension: Dimension) {
        ui.heading(dimension.to_string());

        if ui.button("Add portal").clicked() {
            match dimension {
                Dimension::Overworld => self.add_portal_in_overworld(),
                Dimension::Nether => self.add_portal_in_nether(),
            }
        }

        for portal in &mut self.world.portals[dimension] {
            ui.add_space(2.0);
            egui::TextEdit::singleline(&mut portal.name)
                .hint_text("Portal name")
                .show(ui);

            ui.add_space(8.0);

            portal.adjust_axis(|axis| {
                ui.horizontal(|ui| {
                    ui.label("Facing");
                    ui.selectable_value(axis, PortalAxis::X, "X");
                    ui.selectable_value(axis, PortalAxis::Z, "Z");
                });
            });

            portal.adjust_min(
                |min| show_block_pos_edit(ui, min, dimension),
                self.lock_portal_size,
                dimension,
            );

            portal.adjust_max(
                |max| show_block_pos_edit(ui, max, dimension),
                self.lock_portal_size,
                dimension,
            );

            ui.horizontal(|ui| {
                ui.label("Width");
                portal.adjust_width(|w| egui::DragValue::new(w).ui(ui));
                ui.label("Height");
                portal.adjust_height(|h| egui::DragValue::new(h).ui(ui), dimension);
            });
        }
    }

    fn add_portal_in_overworld(&mut self) {
        let new_portal = Portal::new_minimal(self.camera.pos.into(), PortalAxis::X);
        self.world.portals.overworld.push(new_portal);
    }
    fn add_portal_in_nether(&mut self) {
        let new_portal =
            Portal::new_minimal(self.camera.pos.overworld_to_nether().into(), PortalAxis::X);
        self.world.portals.nether.push(new_portal);
    }

    fn show_view(
        &self,
        ui: &mut egui::Ui,
        view_slice: ViewSlice,
        dimension: Dimension,
        new_camera: &mut Camera,
    ) -> egui::Response {
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

        let mut plot = egui_plot::Plot::new(view_slice)
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
            .x_axis_formatter(|mark, _range| (mark.value / dimension.scale()).to_string())
            .y_axis_formatter(|mark, _range| {
                let y = mark.value / dimension.scale();
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
            let egui::Vec2 { x, y } = (r.size() / 2.0).floor_ui();

            use egui::{Rect, vec2};
            let left_bottom = Rect::from_two_pos(center + vec2(-x, y), center).shrink(PLOT_MARGIN);
            let right_bottom = Rect::from_two_pos(center + vec2(x, y), center).shrink(PLOT_MARGIN);
            let left_top = Rect::from_two_pos(center + vec2(-x, -y), center).shrink(PLOT_MARGIN);
            let right_top = Rect::from_two_pos(center + vec2(x, -y), center).shrink(PLOT_MARGIN);

            ui.scope_builder(egui::UiBuilder::new().max_rect(right_top), |ui| {
                egui::ScrollArea::both()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| self.show_controls(ui))
            });
            for (view_slice, rect) in [
                (ViewSlice::XY, left_bottom),
                (ViewSlice::ZY, right_bottom),
                (ViewSlice::XZ, left_top),
            ] {
                ui.put(rect, |ui: &mut egui::Ui| {
                    self.show_view(ui, view_slice, self.dimension, &mut new_camera)
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

fn show_block_pos_edit(
    ui: &mut egui::Ui,
    BlockPos { x, y, z }: &mut BlockPos,
    dimension: Dimension,
) -> egui::Response {
    let mut changed = false;
    let mut r = ui.horizontal(|ui| {
        ui.label("X");
        let r = ui.add(egui::DragValue::new(x));
        changed |= r.changed();

        ui.label("Y");
        let r = ui.add(egui::DragValue::new(y).range(dimension.y_range()));
        changed |= r.changed();

        ui.label("Z");
        let r = ui.add(egui::DragValue::new(z));
        changed |= r.changed();
    });
    if changed {
        r.response.mark_changed();
    }
    r.response
}
