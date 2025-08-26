//! Tool for planning Minecraft nether portal linkages.

use std::collections::{HashMap, HashSet};

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
pub use entity::Entity;
pub use id::PortalId;
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

/// Animation speed when switching dimensions.
pub const ANIMATION_SPEED: f64 = 8.0;

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
    animation_state: AnimationState,

    show_all_labels: bool,
    show_all_arrows: bool,
    arrow_coloring: ArrowColoring,

    hovered_portals: Vec<PortalId>,
    new_hovered_portals: Vec<PortalId>,

    lock_portal_size: bool,
    entity: Entity,

    last_saved_state: World,
    undo_history: Vec<World>,
    redo_history: Vec<World>,

    cached_state: (World, Entity),
    cached_links: HashMap<PortalId, (PortalLinkResult, Vec<PortalId>)>,
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
            animation_state: AnimationState::default(),

            show_all_labels: true,
            show_all_arrows: false,
            arrow_coloring: ArrowColoring::default(),

            hovered_portals: vec![],
            new_hovered_portals: vec![],

            lock_portal_size: true,
            entity: Entity::PLAYER,

            last_saved_state,
            undo_history: vec![],
            redo_history: vec![],

            cached_state: (World::default(), Entity::default()),
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

    fn set_camera_dimension(&mut self, new_camera_dimension: Dimension) {
        if new_camera_dimension != self.camera.dimension {
            let scale_factor = self.camera.dimension.scale() / new_camera_dimension.scale();
            self.animation_state.aspect_ratio_scale /= scale_factor;
            self.camera.width *= scale_factor;
            self.camera.height *= scale_factor;
        }
        self.camera.set_dimension(new_camera_dimension);
    }

    fn show_controls(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;

        self.hovered_portals = std::mem::take(&mut self.new_hovered_portals);

        ui.horizontal(|ui| {
            ui.strong("View");

            let mut new_camera_dimension = self.camera.dimension;
            for dim in [Overworld, Nether] {
                ui.selectable_value(&mut new_camera_dimension, dim, dim.to_string());
            }
            self.set_camera_dimension(new_camera_dimension);

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
        ui.checkbox(&mut self.show_all_labels, "Show all labels");
        ui.checkbox(&mut self.show_all_arrows, "Show all arrows");
        ui.horizontal(|ui| {
            ui.strong("Color arrows by");
            ui.selectable_value(&mut self.arrow_coloring, ArrowColoring::BySource, "Source");
            ui.selectable_value(
                &mut self.arrow_coloring,
                ArrowColoring::ByDestination,
                "Destination",
            );
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

        let portals_by_id = self.last_saved_state.portals[dimension.other()]
            .iter()
            .map(|p| (p.id, p))
            .collect::<HashMap<PortalId, &Portal>>();

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

                    const OUTLINE_WIDTH: f32 = 2.0;

                    let r = egui::Frame::new()
                        .outer_margin(OUTLINE_WIDTH)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let mut reorder_drag_rect = egui::Rect::from_min_size(
                                    ui.cursor().min,
                                    egui::vec2(12.0, 18.0),
                                );
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
                                            ui.color_edit_button_srgb(&mut portal.color);

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
                                            portal.adjust_height(
                                                |h| dv_i64(ui, "Height", h),
                                                dimension,
                                            );
                                        });
                                    });
                                });

                                show_link_result(
                                    ui,
                                    self.cached_links.get(&portal.id),
                                    &portals_by_id,
                                );

                                // ui.small_button("Calculate naively (expensive)")
                                //     .on_hover_ui(|ui| {
                                //         let destination_dimension = dimension.other();
                                //         let link_result = match portal
                                //             .entity_collision_region(self.entity)
                                //         {
                                //             None => PortalLinkResult::EntityWontFit,
                                //             Some(entry_region) => {
                                //                 let destination_region = entry_region
                                //                     .convert_dimension(
                                //                         dimension,
                                //                         destination_dimension,
                                //                     );
                                //                 let destinations = self
                                //                     .last_saved_state
                                //                     .portals
                                //                     .portal_destinations_naive(
                                //                         destination_dimension,
                                //                         destination_region.block_region_containing(),
                                //                     );

                                //                 PortalLinkResult::Portals {
                                //                     ids: destinations
                                //                         .existing_portals
                                //                         .iter()
                                //                         .map(|p| p.id)
                                //                         .collect(),
                                //                     new_portal: destinations.new_portal,
                                //                 }
                                //             }
                                //         };

                                //         show_link_result(ui, Some(&link_result), &destinations_by_id);
                                //     });
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
                                        ui.painter().circle_filled(
                                            center + egui::vec2(dx, dy),
                                            1.5,
                                            color,
                                        );
                                    }
                                }
                            });
                        });

                    let rect = r.response.rect.intersect(ui.clip_rect());
                    let directly_hovering_this_portal = ui.input(|input| {
                        input
                            .pointer
                            .interact_pos()
                            .is_some_and(|it| rect.contains(it))
                            || input
                                .pointer
                                .press_origin()
                                .is_some_and(|it| rect.contains(it))
                    });
                    if directly_hovering_this_portal {
                        hovered_index = Some(i);
                        self.new_hovered_portals.push(portal.id);
                    }

                    if self.hovered_portals.contains(&portal.id) {
                        let [red, g, b] = portal.color;
                        ui.painter().rect_stroke(
                            r.response.rect,
                            4.0,
                            (OUTLINE_WIDTH, egui::Color32::from_rgb(red, g, b)),
                            egui::StrokeKind::Outside,
                        );

                        if self.hovered_portals.len() == 1 && !directly_hovering_this_portal {
                            r.response
                                .scroll_to_me_animation(None, egui::style::ScrollAnimation::none());
                        }
                    }
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
        let aspect_ratio_scale = self.animation_state.aspect_ratio_scale;
        let width_scale = 1.0;
        let height_scale = match plane {
            Plane::XY | Plane::ZY => aspect_ratio_scale,
            Plane::XZ => 1.0,
        };

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
            .data_aspect(match plane {
                Plane::XY | Plane::ZY => aspect_ratio_scale.recip() as f32,
                Plane::XZ => 1.0,
            })
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
                    let pos = plane.plot_to_world(*hover_point, *new_camera);
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
            let egui_plot::PlotPoint { x, y } = plane.world_to_plot(self.camera.pos);
            let raw_size = plot_ui.transform().frame().size();
            let new_width = self.camera.height * raw_size.x as f64 / raw_size.y as f64;
            bounds_from_camera.set_x_center_width(x, new_width * width_scale);
            bounds_from_camera.set_y_center_height(y, self.camera.height * height_scale);

            plot_ui.set_plot_bounds(bounds_from_camera);

            self.show_portals_in_plot(plot_ui, plane);
            self.show_portal_connections_in_plot(plot_ui, plane);
        });

        if let Some(hovered_world_pos) = r
            .response
            .hover_pos()
            .filter(|&pos| r.transform.frame().contains(pos))
            .map(|pos| r.transform.value_from_position(pos))
            .map(|point| plane.plot_to_world(point, *new_camera))
            .map(|block_pos| block_pos.into())
        {
            let BlockPos { x, y, z } = hovered_world_pos;
            for portal in &self.world.portals[new_camera.dimension] {
                let x_range = portal.region.min.x..=portal.region.max.x;
                let y_range = portal.region.min.y..=portal.region.max.y;
                let z_range = portal.region.min.z..=portal.region.max.z;
                let hovering_portal = match plane {
                    Plane::XY => x_range.contains(&x) && y_range.contains(&y),
                    Plane::XZ => x_range.contains(&x) && z_range.contains(&z),
                    Plane::ZY => z_range.contains(&z) && y_range.contains(&y),
                };
                if hovering_portal {
                    self.new_hovered_portals.push(portal.id);
                }
            }
        }

        // Update camera on interaction with plot
        if r.response.hovered() || r.response.dragged() {
            let bounds = r.transform.bounds();
            let egui_plot::PlotPoint { x, y } = bounds.center();
            match plane {
                Plane::XY => (new_camera.pos.x, new_camera.pos.y) = (x, y),
                Plane::XZ => (new_camera.pos.x, new_camera.pos.z) = (x, -y),
                Plane::ZY => (new_camera.pos.z, new_camera.pos.y) = (x, y),
            }
            new_camera.width = bounds.width() / width_scale;
            new_camera.height = bounds.height() / height_scale;
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

        let a = plane.world_to_plot(region.min);
        let b = plane.world_to_plot(region.max);
        let points = vec![[a.x, a.y], [a.x, b.y], [b.x, b.y], [b.x, a.y]];

        let base_color = match portal_dimension {
            Overworld => egui::Color32::BLUE,
            Nether => egui::Color32::RED,
        }
        .gamma_multiply(opacity);
        let stroke_color = base_color;
        let fill_color = base_color.gamma_multiply(0.2);

        let [r, g, b] = portal.color;

        let polygon = egui_plot::Polygon::new("", points)
            .fill_color(fill_color)
            .stroke((
                stroke_width,
                egui::Color32::from_rgb(r, g, b).gamma_multiply(opacity),
            ));

        plot_ui.add(polygon);
        if !portal.name.is_empty() {
            let mut job = egui::text::LayoutJob::default();
            job.append(
                if self.show_all_labels || self.hovered_portals.contains(&portal.id) {
                    &portal.name
                } else {
                    ""
                },
                0.0,
                egui::TextFormat::simple(
                    egui::FontId::proportional(14.0),
                    stroke_color.linear_multiply(0.25).additive()
                        + egui::Color32::WHITE.linear_multiply(0.75).additive(),
                ),
            );

            plot_ui.add(egui_plot::Text::new(
                "",
                plane.world_to_plot(region.center()),
                job,
            ));
        }
    }

    fn show_portal_connections_in_plot(&self, plot_ui: &mut egui_plot::PlotUi<'_>, plane: Plane) {
        if !self.show_all_arrows && self.hovered_portals.is_empty() {
            return;
        }

        let id_to_portal: HashMap<PortalId, &Portal> =
            itertools::chain(&self.world.portals.overworld, &self.world.portals.nether)
                .map(|p| (p.id, p))
                .collect();
        let overworld_portal_set: HashSet<PortalId> =
            self.world.portals.overworld.iter().map(|p| p.id).collect();
        let get_dim_of_portal = |id| {
            if overworld_portal_set.contains(id) {
                Overworld
            } else {
                Nether
            }
        };
        for (id2, (_outgoing, incoming)) in &self.cached_links {
            let Some(portal2) = id_to_portal.get(id2) else {
                continue;
            };
            let dim2 = get_dim_of_portal(id2);

            for id1 in incoming {
                if self.show_all_arrows
                    || self.hovered_portals.contains(id1)
                    || self.hovered_portals.contains(id2)
                {
                    let Some(portal1) = id_to_portal.get(id1) else {
                        continue;
                    };
                    let dim1 = get_dim_of_portal(id1);

                    self.show_portal_connection_in_plot(
                        plot_ui, plane, portal1, dim1, portal2, dim2,
                    );
                }
            }
        }
    }

    fn show_portal_connection_in_plot(
        &self,
        plot_ui: &mut egui_plot::PlotUi<'_>,
        plane: Plane,
        src: &Portal,
        src_dimension: Dimension,
        dst: &Portal,
        dst_dimension: Dimension,
    ) {
        let camera_dim = self.camera.dimension;
        let src_pos = WorldRegion::from(src.region).center();
        let dst_pos = WorldRegion::from(dst.region).center();
        let mut src_point =
            plane.world_to_plot(src_pos.convert_dimension(src_dimension, camera_dim));
        let mut dst_point =
            plane.world_to_plot(dst_pos.convert_dimension(dst_dimension, camera_dim));

        // can't use `plot_ui.dpos_dvalue_x()` because it doesn't use the
        // updated transform
        let dpos_dvalue_x = plot_ui.transform().frame().width() / self.camera.width as f32;

        // Shrink arrow by half a block
        let vector =
            (dst_point.to_pos2() - src_point.to_pos2()).normalized() * 8.0 / dpos_dvalue_x.sqrt();
        src_point.x += vector.x as f64;
        src_point.y += vector.y as f64;
        dst_point.x -= vector.x as f64;
        dst_point.y -= vector.y as f64;

        let [r, g, b] = match self.arrow_coloring {
            ArrowColoring::BySource => src.color,
            ArrowColoring::ByDestination => dst.color,
        };

        plot_ui.add(
            egui_plot::Arrows::new(
                format!("{} to {}", src.display_name(), dst.display_name()),
                egui_plot::PlotPoints::Owned(vec![src_point]),
                egui_plot::PlotPoints::Owned(vec![dst_point]),
            )
            .color(egui::Color32::from_rgb(r, g, b))
            .tip_length(dpos_dvalue_x.sqrt() / camera_dim.scale() as f32 * 6.0),
        );
    }

    fn calculate_portal_link_result(
        &self,
        portal: &Portal,
        portal_dimension: Dimension,
    ) -> PortalLinkResult {
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

        // Add outgoing connections
        for portal_dimension in [Overworld, Nether] {
            for portal in &self.world.portals[portal_dimension] {
                self.cached_links.insert(
                    portal.id,
                    (
                        self.calculate_portal_link_result(portal, portal_dimension),
                        vec![],
                    ),
                );
            }
        }

        // Add incoming connections
        for (id, (outgoing, _)) in self.cached_links.clone() {
            if let PortalLinkResult::Portals { ids, new_portal: _ } = outgoing {
                for destination_id in ids {
                    match self.cached_links.get_mut(&destination_id) {
                        Some((_, incoming)) => incoming.push(id),
                        None => log::error!("no destination portal with id {destination_id}"),
                    }
                }
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
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();

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
            let now = std::time::Instant::now();
            if !self.animation_state.is_static() {
                ctx.request_repaint();
            }
            self.animation_state
                .step((now - self.animation_state.last_frame).as_secs_f64());
            self.animation_state.last_frame = now;

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
                    }

                    // Consume the most specific shortcut first
                    if input.consume_shortcut(&CMD_SHIFT_Z) || input.consume_shortcut(&CMD_Y) {
                        self.redo();
                        self.save_to_file();
                    } else if input.consume_shortcut(&CMD_Z) {
                        self.undo();
                        self.save_to_file();
                    }

                    if input.consume_shortcut(&egui::KeyboardShortcut::new(
                        egui::Modifiers::NONE,
                        egui::Key::Space,
                    )) {
                        self.set_camera_dimension(self.camera.dimension.other());
                    }
                }
            });
        });

        let (cached_world, cached_entity) = &self.cached_state;
        if (cached_world, cached_entity) != (&self.world, &self.entity) {
            let t = std::time::Instant::now();
            self.cached_state = (self.world.clone(), self.entity);
            self.recalculate_portal_links();
            log::debug!("Recalculated portal links in {:?}", t.elapsed());
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PortalLinkResult {
    EntityWontFit,
    Portals {
        ids: Vec<PortalId>,
        new_portal: bool,
    },
}

fn show_link_result(
    ui: &mut egui::Ui,
    result: Option<&(PortalLinkResult, Vec<PortalId>)>,
    portals_by_id: &HashMap<PortalId, &Portal>,
) {
    let Some((outgoing, incoming)) = result else {
        ui.colored_label(ui.visuals().warn_fg_color, "Calculating ...");
        return;
    };

    match outgoing {
        PortalLinkResult::EntityWontFit => {
            ui.colored_label(ui.visuals().error_fg_color, "Entity won't fit");
        }
        PortalLinkResult::Portals { ids, new_portal } => {
            if !ids.is_empty() {
                let mut label_atoms = egui::Atoms::new("Links to: ");
                push_portal_list_text(ui, &mut label_atoms, ids, portals_by_id);
                ui.add(egui::AtomLayout::new(label_atoms));
            }
            if *new_portal {
                ui.colored_label(ui.visuals().error_fg_color, "Generates new portal");
            }
        }
    }

    if !incoming.is_empty() {
        let mut label_atoms = egui::Atoms::new("Links from: ");
        push_portal_list_text(ui, &mut label_atoms, incoming, portals_by_id);
        ui.add(egui::AtomLayout::new(label_atoms));
    }
}

fn push_portal_list_text(
    ui: &egui::Ui,
    atoms: &mut egui::Atoms<'_>,
    portal_ids: &[PortalId],
    portals_by_id: &HashMap<PortalId, &Portal>,
) {
    let mut is_first = true;
    for id in portal_ids {
        if is_first {
            is_first = false;
        } else {
            atoms.push_right(", ");
        }
        let (name, color) = match portals_by_id.get(id) {
            Some(p) => {
                let [r, g, b] = p.color;
                (p.display_name(), egui::Color32::from_rgb(r, g, b))
            }
            None => ("<unknown>", ui.visuals().weak_text_color()),
        };
        atoms.push_right(egui::RichText::new(name).color(color));
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
struct AnimationState {
    last_frame: std::time::Instant,
    aspect_ratio_scale: f64,
}
impl Default for AnimationState {
    fn default() -> Self {
        Self {
            last_frame: std::time::Instant::now(),
            aspect_ratio_scale: 1.0,
        }
    }
}
impl AnimationState {
    fn step(&mut self, dt: f64) {
        self.aspect_ratio_scale = self.aspect_ratio_scale.powf(1.0 - dt * ANIMATION_SPEED);
        if self.aspect_ratio_scale.log2().abs() < 0.0025 {
            self.aspect_ratio_scale = 1.0;
        }
    }

    fn is_static(&self) -> bool {
        self.aspect_ratio_scale == 1.0
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
enum ArrowColoring {
    #[default]
    BySource,
    ByDestination,
}
