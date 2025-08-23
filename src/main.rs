use egui::Widget;
use egui::emath::GuiRounding;

mod camera;
mod entity;
mod portal;
mod pos;
mod region;
mod world;

pub use Dimension::{Nether, Overworld};
pub use camera::{Camera, Plane};
use egui_plot::PlotPoint;
pub use entity::Entity;
pub use portal::{Direction, Portal, PortalAxis};
pub use pos::{Axis, BlockPos, WorldPos};
pub use region::{BlockRegion, WorldRegion};
pub use world::{ConvertDimension, Dimension, World, WorldPortals};

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
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx
            .style_mut(|style| style.explanation_tooltips = true);

        let world = std::fs::read(SAVE_FILE_PATH)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();

        Self {
            world,
            camera: Camera::default(),

            dimension: Overworld,
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
            let old_dimension = self.dimension;
            for dim in [Overworld, Nether] {
                ui.selectable_value(&mut self.dimension, dim, dim.to_string());
            }
            if old_dimension != self.dimension {
                self.camera.pos = self
                    .camera
                    .pos
                    .convert_dimension(old_dimension, self.dimension);
            }

            show_world_pos_edit(ui, &mut self.camera.pos, self.dimension);
        });

        ui.horizontal(|ui| {
            if ui.button("Add portal pair").clicked() {
                self.add_portal_in_overworld();
                self.add_portal_in_nether();
            }

            ui.checkbox(&mut self.lock_portal_size, "Lock portal size");
        });

        ui.columns(2, |uis| {
            self.show_portal_list(&mut uis[0], Overworld);
            self.show_portal_list(&mut uis[1], Nether);
        });
    }

    fn show_portal_list(&mut self, ui: &mut egui::Ui, dimension: Dimension) {
        ui.heading(dimension.to_string());

        if ui.button("Add portal").clicked() {
            match dimension {
                Overworld => self.add_portal_in_overworld(),
                Nether => self.add_portal_in_nether(),
            }
        }

        self.world.portals[dimension].retain_mut(|portal| {
            let mut keep = true;
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui.button("🗑").clicked() {
                    keep = false;
                }
                egui::TextEdit::singleline(&mut portal.name)
                    .hint_text("Portal name")
                    .show(ui);
            });

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
                portal.adjust_width(|w| dv_i64(w).ui(ui));
                ui.label("Height");
                portal.adjust_height(|h| dv_i64(h).ui(ui), dimension);
            });

            keep
        });
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
        &mut self,
        ui: &mut egui::Ui,
        plane: Plane,
        dimension: Dimension,
        new_camera: &mut Camera,
    ) -> egui::Response {
        let mut plot = egui_plot::Plot::new(plane)
            .x_axis_label(match plane {
                Plane::XY | Plane::XZ => "X",
                Plane::ZY => "Z",
            })
            .y_axis_label(match plane {
                Plane::XY | Plane::ZY => "Y",
                Plane::XZ => "Z",
            })
            .x_grid_spacer(egui_plot::log_grid_spacer(8))
            .y_grid_spacer(egui_plot::log_grid_spacer(8))
            .data_aspect(1.0)
            .x_axis_formatter(|mark, _range| mark.value.to_string())
            .y_axis_formatter(|mark, _range| {
                let y = mark.value;
                if Plane::XZ == plane { -y } else { y }.to_string()
            })
            .allow_axis_zoom_drag(false)
            .show_x(false)
            .show_y(false)
            .coordinates_formatter(
                egui_plot::Corner::LeftBottom,
                egui_plot::CoordinatesFormatter::new(|hover_point, _bounds| {
                    let (x, y, z) = match plane {
                        Plane::XY => (hover_point.x, hover_point.y, new_camera.pos.z),
                        Plane::XZ => (hover_point.x, new_camera.pos.y, hover_point.y),
                        Plane::ZY => (new_camera.pos.x, hover_point.y, hover_point.x),
                    };
                    let pos = WorldPos { x, y, z };
                    format!(
                        "Overworld: {overworld:.03}\n   Nether: {nether:.03}",
                        overworld = pos.convert_dimension(dimension, Overworld),
                        nether = pos.convert_dimension(dimension, Nether),
                    )
                }),
            );

        plot = match plane {
            Plane::XY => plot,
            Plane::XZ => plot.x_axis_position(egui_plot::VPlacement::Top),
            Plane::ZY => plot.y_axis_position(egui_plot::HPlacement::Right),
        };

        let r = plot.show(ui, |plot_ui| {
            // Compute plot bounds from camera
            let mut bounds_from_camera = egui_plot::PlotBounds::NOTHING;
            let [x, y] = match plane {
                Plane::XY => [self.camera.pos.x, self.camera.pos.y],
                Plane::XZ => [self.camera.pos.x, self.camera.pos.z],
                Plane::ZY => [self.camera.pos.z, self.camera.pos.y],
            };
            let old_width = plot_ui.plot_bounds().width();
            let old_height = plot_ui.plot_bounds().width();
            let new_width = self.camera.height / old_height * old_width;
            bounds_from_camera.set_x_center_width(x, new_width);
            bounds_from_camera.set_y_center_height(y, self.camera.height);

            plot_ui.set_plot_bounds(bounds_from_camera);

            self.show_portals_in_plot(plot_ui, plane, dimension);
        });

        // Update camera on interaction with plot
        if r.response.hovered() || r.response.dragged() {
            let bounds = r.transform.bounds();
            let PlotPoint { x, y } = bounds.center();
            match plane {
                Plane::XY => (new_camera.pos.x, new_camera.pos.y) = (x, y),
                Plane::XZ => (new_camera.pos.x, new_camera.pos.z) = (x, y),
                Plane::ZY => (new_camera.pos.z, new_camera.pos.y) = (x, y),
            }
            new_camera.width = bounds.width();
            new_camera.height = bounds.height();
        }

        r.response
    }

    fn show_portals_in_plot(
        &self,
        plot_ui: &mut egui_plot::PlotUi,
        plane: Plane,
        dimension: Dimension,
    ) {
        for portal_dim in [dimension, dimension.other()] {
            for portal in &self.world.portals[portal_dim] {
                self.show_portal_in_plot(plot_ui, plane, portal, portal_dim, dimension);
            }
        }
    }

    fn show_portal_in_plot(
        &self,
        plot_ui: &mut egui_plot::PlotUi,
        plane: Plane,
        portal: &Portal,
        portal_dimension: Dimension,
        plot_dimension: Dimension,
    ) {
        let opacity = if portal_dimension == plot_dimension {
            1.0
        } else {
            0.5
        };
        let stroke_width = if portal_dimension == plot_dimension {
            3.0
        } else {
            1.5
        };

        let region =
            WorldRegion::from(portal.region).convert_dimension(portal_dimension, plot_dimension);
        let points = world_region_to_rect_plot_points(region, plane);

        let base_color = match portal_dimension {
            Overworld => egui::Color32::BLUE,
            Nether => egui::Color32::RED,
        }
        .gamma_multiply(opacity);
        let stroke_color = base_color;
        let fill_color = base_color.gamma_multiply(0.2);

        let polygon = egui_plot::Polygon::new("", points)
            .fill_color(fill_color)
            .stroke((stroke_width, stroke_color));

        plot_ui.add(polygon);
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

            for (plane, rect) in [
                (Plane::XY, left_bottom),
                (Plane::ZY, right_bottom),
                (Plane::XZ, left_top),
            ] {
                ui.put(rect, |ui: &mut egui::Ui| {
                    self.show_view(ui, plane, self.dimension, &mut new_camera)
                });
            }
            self.camera = new_camera;

            ui.scope_builder(egui::UiBuilder::new().max_rect(right_top), |ui| {
                egui::ScrollArea::both()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| self.show_controls(ui))
            });
        });
    }
}

fn world_region_to_rect_plot_points(region: WorldRegion, slice: Plane) -> Vec<[f64; 2]> {
    let WorldRegion { min, max } = region;
    let PlotPoint { x: x1, y: y1 } = world_pos_to_plot_point(min, slice);
    let PlotPoint { x: x2, y: y2 } = world_pos_to_plot_point(max, slice);
    vec![[x1, y1], [x1, y2], [x2, y2], [x2, y1]]
}
fn world_pos_to_plot_point(pos: WorldPos, slice: Plane) -> PlotPoint {
    let [x, y] = match slice {
        Plane::XY => [pos.x, pos.y],
        Plane::XZ => [pos.x, -pos.z],
        Plane::ZY => [pos.z, pos.y],
    };
    PlotPoint { x, y }
}

fn show_block_pos_edit(
    ui: &mut egui::Ui,
    BlockPos { x, y, z }: &mut BlockPos,
    dimension: Dimension,
) -> egui::Response {
    let y_range = dimension.y_range();

    let mut changed = false;
    let mut r = ui.horizontal(|ui| {
        coordinate_label(ui, "X");
        changed |= ui.add(dv_i64(x)).changed();

        coordinate_label(ui, "Y");
        changed |= ui.add(dv_i64(y).range(y_range)).changed();

        coordinate_label(ui, "Z");
        changed |= ui.add(dv_i64(z)).changed();
    });
    if changed {
        r.response.mark_changed();
    }
    r.response
}

fn show_world_pos_edit(
    ui: &mut egui::Ui,
    WorldPos { x, y, z }: &mut WorldPos,
    dimension: Dimension,
) -> egui::Response {
    let y_range = dimension.y_range();

    let mut changed = false;
    let mut r = ui.horizontal(|ui| {
        coordinate_label(ui, "X");
        changed |= ui.add(egui::DragValue::new(x)).changed();

        coordinate_label(ui, "Y");
        changed |= ui.add(egui::DragValue::new(y).range(y_range)).changed();

        coordinate_label(ui, "Z");
        changed |= ui.add(egui::DragValue::new(z)).changed();
    });
    if changed {
        r.response.mark_changed();
    }
    r.response
}

pub fn dv_i64(i: &mut i64) -> egui::DragValue<'_> {
    egui::DragValue::new(i).speed(0.1)
}

fn coordinate_label(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let r = ui.label(text);
    ui.add_space(-ui.spacing().item_spacing.x * 0.5);
    r
}
