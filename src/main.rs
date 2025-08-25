//! Tool for planning Minecraft nether portal linkages.

use std::collections::HashMap;

use egui::Widget;
use egui::emath::GuiRounding;

mod camera;
mod entity;
mod id;
mod portal;
mod pos;
mod region;
mod util;
mod world;

pub use Dimension::{Nether, Overworld};
pub use camera::{Camera, Plane};
use egui_plot::PlotPoint;
pub use entity::Entity;
pub use id::PortalId;
use itertools::Itertools;
pub use portal::{Portal, PortalAxis};
pub use pos::{Axis, BlockPos, WorldPos};
pub use region::{BlockRegion, WorldRegion};
pub use world::{ConvertDimension, Dimension, World, WorldPortals};

/// Scroll sensitivity override for egui, particularly when zooming in/out of
/// the plot.
pub const SCROLL_SENSITIVITY: f32 = 0.25;
/// Path for loading & saving the portal configuration.
pub const SAVE_FILE_PATH: &str = "world.json";
/// Margin around each plot.
pub const PLOT_MARGIN: f32 = 8.0;

/// Ctrl+Z shortcut for undo.
pub const CMD_Z: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
/// Ctrl+Shift+Z shortcut for redo.
pub const CMD_SHIFT_Z: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::Z,
);
/// Ctrl+Y shortcut for redo.
pub const CMD_Y: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);

fn main() -> eframe::Result {
    env_logger::builder()
        .filter_module("portal_tool", log::LevelFilter::Debug)
        .init();
    eframe::run_native(
        "Portal Tool",
        eframe::NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

/// Application state.
pub struct App {
    world: World,
    camera: Camera,

    lock_portal_size: bool,
    entity: Entity,
    last_calculated_entity: Entity,

    last_saved_state: World,
    undo_history: Vec<World>,
    redo_history: Vec<World>,

    cached_links: HashMap<PortalId, PortalLinkResult>,
}

impl App {
    /// Constructs the application state.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx
            .style_mut(|style| style.explanation_tooltips = true);

        let world: World = std::fs::read(SAVE_FILE_PATH)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();

        let last_saved_state = world.clone();

        Self {
            world,
            camera: Camera::default(),
            entity: Entity::PLAYER,

            lock_portal_size: true,
            last_calculated_entity: Entity::default(),

            last_saved_state,
            undo_history: vec![],
            redo_history: vec![],

            cached_links: HashMap::new(), // will be recomputed on first frame
        }
    }

    fn save_to_file(&self) {
        if let Ok(bytes) = serde_json::to_vec_pretty(&self.world) {
            match std::fs::write(SAVE_FILE_PATH, &bytes) {
                Ok(()) => (),
                Err(e) => eprintln!("error saving {SAVE_FILE_PATH:?}: {e}"),
            }
        }
    }

    fn undo(&mut self) {
        if let Some(new_state) = self.undo_history.pop() {
            let old_state = std::mem::replace(&mut self.world, new_state);
            self.last_saved_state = self.world.clone();
            self.redo_history.push(old_state);
        }
    }
    fn redo(&mut self) {
        if let Some(new_state) = self.redo_history.pop() {
            let old_state = std::mem::replace(&mut self.world, new_state);
            self.last_saved_state = self.world.clone();
            self.undo_history.push(old_state);
        }
    }

    fn show_controls(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.strong("View");

            let mut camera_dimension = self.camera.dimension;
            for dim in [Overworld, Nether] {
                ui.selectable_value(&mut camera_dimension, dim, dim.to_string());
            }
            self.camera.set_dimension(camera_dimension);

            show_world_pos_edit(ui, &mut self.camera.pos, self.camera.dimension);
        });

        ui.horizontal(|ui| {
            ui.strong("Entity");
            ui.menu_button("Load…", |ui| {
                if ui.button("Player").clicked() {
                    self.entity = Entity::PLAYER;
                }
                if ui.button("Ender pearl").clicked() {
                    self.entity = Entity::ENDER_PEARL;
                }
                if ui.button("Arrow").clicked() {
                    self.entity = Entity::ARROW;
                }
                if ui.button("Ghast").clicked() {
                    self.entity = Entity::GHAST;
                }
                if ui.button("Item").clicked() {
                    self.entity = Entity::ITEM;
                }
            });
            coordinate_label(ui, "Width");
            ui.add(egui::DragValue::new(&mut self.entity.width).range(0.0..=256.0));
            coordinate_label(ui, "Height");
            ui.add(egui::DragValue::new(&mut self.entity.height).range(0.0..=256.0));
            ui.checkbox(&mut self.entity.is_projectile, "Projectile");
        });

        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("Add portal pair").clicked() {
                self.add_portal_in_overworld();
                self.add_portal_in_nether();
                changed = true;
            }

            ui.checkbox(&mut self.lock_portal_size, "Lock portal size");
        });

        ui.columns(2, |uis| {
            self.show_portal_list(&mut uis[0], Overworld);
            self.show_portal_list(&mut uis[1], Nether);
        });

        if changed {
            self.save_to_file();
        }
    }

    fn show_portal_list(&mut self, ui: &mut egui::Ui, dimension: Dimension) {
        ui.heading(dimension.to_string());

        if ui.button("Add portal").clicked() {
            match dimension {
                Overworld => self.add_portal_in_overworld(),
                Nether => self.add_portal_in_nether(),
            }
        }

        let destination_id_to_name = self.world.portals[dimension.other()]
            .iter()
            .map(|p| (p.id, p.name.clone()))
            .collect::<HashMap<PortalId, String>>();

        ui.separator();

        let mut reorder_drag_source = None;
        let mut hovered_index = None;
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt(dimension)
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for (i, portal) in self.world.portals[dimension].iter_mut().enumerate() {
                    if i > 0 {
                        ui.separator();
                    }

                    let r = ui.horizontal(|ui| {
                        let mut reorder_drag_rect =
                            egui::Rect::from_min_size(ui.cursor().min, egui::vec2(12.0, 18.0));
                        ui.advance_cursor_after_rect(reorder_drag_rect);

                        ui.vertical(|ui| {
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ui.ctx(),
                                egui::Id::new(portal.id).with("header"),
                                false,
                            )
                            .show_header(ui, |ui| {
                                egui::Sides::new().shrink_left().show(
                                    ui,
                                    |ui| {
                                        ui.horizontal(|ui| {
                                            egui::TextEdit::singleline(&mut portal.name)
                                                .hint_text("Portal name")
                                                .show(ui);
                                        });
                                    },
                                    |ui| {
                                        if ui.button("🗑").clicked() {
                                            remove = Some(i);
                                        }
                                    },
                                );
                            })
                            .body(|ui| {
                                ui.vertical(|ui| {
                                    portal.adjust_axis(|axis| {
                                        ui.horizontal(|ui| {
                                            ui.label("Facing");
                                            ui.selectable_value(axis, PortalAxis::X, "X");
                                            ui.selectable_value(axis, PortalAxis::Z, "Z");
                                        });
                                    });

                                    portal.adjust_min(
                                        |min| show_block_pos_edit(ui, min),
                                        self.lock_portal_size,
                                        dimension,
                                    );

                                    portal.adjust_max(
                                        |max| show_block_pos_edit(ui, max),
                                        self.lock_portal_size,
                                        dimension,
                                    );

                                    ui.horizontal(|ui| {
                                        portal.adjust_width(|w| dv_i64(ui, "Width", w));
                                        portal
                                            .adjust_height(|h| dv_i64(ui, "Height", h), dimension);
                                    });
                                });
                            });

                            match self.cached_links.get(&portal.id) {
                                None => {
                                    ui.colored_label(ui.visuals().warn_fg_color, "Calculating ...");
                                }
                                Some(PortalLinkResult::EntityWontFit) => {
                                    ui.colored_label(
                                        ui.visuals().error_fg_color,
                                        "Entity won't fit",
                                    );
                                }
                                Some(PortalLinkResult::Portals { ids, new_portal }) => {
                                    if !ids.is_empty() {
                                        let mut names = ids.iter().map(|id| {
                                            destination_id_to_name
                                                .get(id)
                                                .map(|s| s.as_str())
                                                .unwrap_or("<unknown>")
                                        });
                                        ui.strong(format!("Links to: {}", names.join(", ")));
                                    }
                                    if *new_portal {
                                        ui.colored_label(
                                            ui.visuals().warn_fg_color,
                                            "Generates new portal",
                                        );
                                    }
                                }
                            }
                        });

                        reorder_drag_rect.max.y = ui.min_rect().max.y;
                        let r = ui.interact(
                            reorder_drag_rect,
                            egui::Id::new(portal.id).with("reorder"),
                            egui::Sense::drag(),
                        );
                        let color;
                        if r.dragged() {
                            reorder_drag_source = Some(i);
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                            color = ui.visuals().strong_text_color();
                        } else if r.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                            color = ui.visuals().text_color();
                        } else {
                            color = ui.visuals().weak_text_color();
                        }
                        let center = reorder_drag_rect.center();
                        let sp = 5.0;
                        for dx in [-sp / 2.0, sp / 2.0] {
                            for dy in [-sp, 0.0, sp] {
                                ui.painter()
                                    .circle_filled(center + egui::vec2(dx, dy), 1.5, color);
                            }
                        }
                    });

                    let rect = r.response.rect.intersect(ui.clip_rect());
                    ui.input(|input| {
                        if rect.contains(input.pointer.interact_pos()?) {
                            hovered_index = Some(i);
                        }
                        None::<()>
                    });
                }
            });

        if let (Some(i), Some(j)) = (reorder_drag_source, hovered_index) {
            if i < j {
                self.world.portals[dimension][i..=j].rotate_left(1);
            } else if i > j {
                self.world.portals[dimension][j..=i].rotate_right(1);
            }
        }
        if let Some(i) = remove {
            self.world.portals[dimension].remove(i);
        }
    }

    fn add_portal_in_overworld(&mut self) {
        let new_portal =
            Portal::new_minimal(self.camera.pos.into(), PortalAxis::X, self.camera.dimension);
        self.world.portals.overworld.push(new_portal);
    }
    fn add_portal_in_nether(&mut self) {
        let new_portal = Portal::new_minimal(
            self.camera.pos.overworld_to_nether().into(),
            PortalAxis::X,
            self.camera.dimension,
        );
        self.world.portals.nether.push(new_portal);
    }

    fn show_view(
        &mut self,
        ui: &mut egui::Ui,
        plane: Plane,
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
            .allow_boxed_zoom(false)
            .show_x(false)
            .show_y(false)
            .coordinates_formatter(
                egui_plot::Corner::LeftBottom,
                egui_plot::CoordinatesFormatter::new(|hover_point, _bounds| {
                    let (x, y, z) = match plane {
                        Plane::XY => (hover_point.x, hover_point.y, new_camera.pos.z),
                        Plane::XZ => (hover_point.x, new_camera.pos.y, -hover_point.y),
                        Plane::ZY => (new_camera.pos.x, hover_point.y, hover_point.x),
                    };
                    let pos = WorldPos { x, y, z };
                    format!(
                        "Overworld: {overworld:10.03}\n   Nether: {nether:10.03}",
                        overworld = pos.convert_dimension(self.camera.dimension, Overworld),
                        nether = pos.convert_dimension(self.camera.dimension, Nether),
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
                Plane::XZ => [self.camera.pos.x, -self.camera.pos.z],
                Plane::ZY => [self.camera.pos.z, self.camera.pos.y],
            };
            let old_width = plot_ui.plot_bounds().width();
            let old_height = plot_ui.plot_bounds().width();
            let new_width = self.camera.height / old_height * old_width;
            bounds_from_camera.set_x_center_width(x, new_width);
            bounds_from_camera.set_y_center_height(y, self.camera.height);

            plot_ui.set_plot_bounds(bounds_from_camera);

            self.show_portals_in_plot(plot_ui, plane);
        });

        // Update camera on interaction with plot
        if r.response.hovered() || r.response.dragged() {
            let bounds = r.transform.bounds();
            let PlotPoint { x, y } = bounds.center();
            match plane {
                Plane::XY => (new_camera.pos.x, new_camera.pos.y) = (x, y),
                Plane::XZ => (new_camera.pos.x, new_camera.pos.z) = (x, -y),
                Plane::ZY => (new_camera.pos.z, new_camera.pos.y) = (x, y),
            }
            new_camera.width = bounds.width();
            new_camera.height = bounds.height();
        }

        r.response
    }

    fn show_portals_in_plot(&self, plot_ui: &mut egui_plot::PlotUi<'_>, plane: Plane) {
        let dimension = self.camera.dimension;
        for portal_dim in [dimension, dimension.other()] {
            for portal in &self.world.portals[portal_dim] {
                self.show_portal_in_plot(plot_ui, plane, portal, portal_dim, dimension);
            }
        }
    }

    fn show_portal_in_plot(
        &self,
        plot_ui: &mut egui_plot::PlotUi<'_>,
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
        if !portal.name.is_empty() {
            let mut job = egui::text::LayoutJob::default();
            job.append(
                &portal.name,
                0.0,
                egui::TextFormat::simple(
                    egui::FontId::proportional(14.0),
                    stroke_color.linear_multiply(0.25).additive()
                        + egui::Color32::WHITE.linear_multiply(0.75).additive(),
                ),
            );

            plot_ui.add(egui_plot::Text::new(
                "",
                world_pos_to_plot_point(region.center(), plane),
                job,
            ));
        }
    }

    fn portal_link_result(&self, portal: &Portal, portal_dimension: Dimension) -> PortalLinkResult {
        let destination_dimension = portal_dimension.other();
        let Some(entry_region) = portal.entity_collision_region(self.entity) else {
            return PortalLinkResult::EntityWontFit;
        };
        let destination_region =
            entry_region.convert_dimension(portal_dimension, destination_dimension);
        let destinations = self.world.portals.portal_destinations(
            destination_dimension,
            destination_region.block_region_containing(),
        );
        PortalLinkResult::Portals {
            ids: destinations.existing_portals.iter().map(|p| p.id).collect(),
            new_portal: destinations.new_portal,
        }
    }

    fn recalculate_portal_links(&mut self) {
        self.cached_links.clear();
        for portal_dimension in [Overworld, Nether] {
            for portal in &self.world.portals[portal_dimension] {
                self.cached_links
                    .insert(portal.id, self.portal_link_result(portal, portal_dimension));
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
        let mut must_recalculate_portal_links = false;

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
                    self.show_view(ui, plane, &mut new_camera)
                });
            }
            self.camera = new_camera;

            ui.scope_builder(egui::UiBuilder::new().max_rect(right_top), |ui| {
                egui::ScrollArea::horizontal()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| self.show_controls(ui))
            });

            let is_text_field_active = ui.ctx().wants_keyboard_input();
            ui.input_mut(|input| {
                if !input.pointer.is_decidedly_dragging() && !is_text_field_active {
                    if self.last_saved_state != self.world {
                        let state_to_save =
                            std::mem::replace(&mut self.last_saved_state, self.world.clone());
                        self.redo_history.clear();
                        self.undo_history.push(state_to_save);
                        self.save_to_file();
                        must_recalculate_portal_links = true;
                    }

                    // Consume the most specific shortcut first
                    if input.consume_shortcut(&CMD_SHIFT_Z) || input.consume_shortcut(&CMD_Y) {
                        self.redo();
                        self.save_to_file();
                    } else if input.consume_shortcut(&CMD_Z) {
                        self.undo();
                        self.save_to_file();
                    }
                }
            });
        });

        if self.last_calculated_entity != self.entity {
            self.last_calculated_entity = self.entity;
            must_recalculate_portal_links = true;
        }

        if must_recalculate_portal_links {
            let t = std::time::Instant::now();
            self.recalculate_portal_links();
            log::debug!("Recalculated portal links in {:?}", t.elapsed());
        }
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

fn show_block_pos_edit(ui: &mut egui::Ui, BlockPos { x, y, z }: &mut BlockPos) {
    ui.horizontal(|ui| {
        dv_i64(ui, "X", x);
        dv_i64(ui, "Y", y);
        dv_i64(ui, "Z", z);
    });
}

fn show_world_pos_edit(
    ui: &mut egui::Ui,
    WorldPos { x, y, z }: &mut WorldPos,
    dimension: Dimension,
) -> egui::Response {
    ui.horizontal(|ui| {
        coordinate_label(ui, "X");
        ui.add(egui::DragValue::new(x));

        coordinate_label(ui, "Y");
        ui.add(egui::DragValue::new(y).range(dimension.y_range()));

        coordinate_label(ui, "Z");
        ui.add(egui::DragValue::new(z));
    })
    .response
}

fn dv_i64(ui: &mut egui::Ui, label: &str, i: &mut i64) -> egui::Response {
    ui.horizontal(|ui| {
        coordinate_label(ui, label);
        egui::DragValue::new(i)
            .speed(0.125)
            .update_while_editing(false)
            .ui(ui);
    })
    .response
}

fn coordinate_label(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let r = ui.label(text);
    ui.add_space(-ui.spacing().item_spacing.x * 0.5);
    r
}

enum PortalLinkResult {
    EntityWontFit,
    Portals {
        ids: Vec<PortalId>,
        new_portal: bool,
    },
}
