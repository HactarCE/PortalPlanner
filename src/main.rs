//! Tool for planning Minecraft nether portal linkages.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use core::f32;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;

use egui::Widget;
use egui::emath::GuiRounding;
use oneshot::TryRecvError;
use serde::{Deserialize, Serialize};

mod camera;
mod entity;
mod id;
mod portal;
mod pos;
mod region;
mod threads;
mod util;
mod world;

pub use Dimension::{Nether, Overworld};
pub use camera::{Camera, Plane};
pub use entity::Entity;
pub use id::PortalId;
use itertools::Itertools;
pub use portal::{Portal, PortalAxis};
pub use pos::{Axis, BlockPos, WorldPos};
pub use region::{BlockRegion, WorldRegion};
use threads::AsyncSafe;
pub use world::{ConvertDimension, Dimension, World, WorldPortals};

/// Application title.
pub const TITLE: &str = "Portal Planner";

const IS_WEB: bool = cfg!(target_arch = "wasm32");

/// Scroll sensitivity override for egui, particularly when zooming in/out of
/// the plot.
pub const SCROLL_SENSITIVITY: f32 = 0.25;
/// Margin between plots.
pub const PLOT_MARGIN: f32 = 8.0;

/// Animation speed when switching dimensions.
pub const ANIMATION_SPEED: f64 = 8.0;

#[allow(missing_docs)]
mod kbd_shortcuts {
    use egui::{Key, KeyboardShortcut as Shortcut, Modifiers as Mods};

    /// Ctrl+Z shortcut for undo.
    pub const CMD_Z: Shortcut = Shortcut::new(Mods::COMMAND, Key::Z);
    /// Ctrl+Shift+Z shortcut for redo.
    pub const CMD_SHIFT_Z: Shortcut = Shortcut::new(Mods::COMMAND.plus(Mods::SHIFT), Key::Z);
    /// Ctrl+Y shortcut for redo.
    pub const CMD_Y: Shortcut = Shortcut::new(Mods::COMMAND, Key::Y);

    pub const SWITCH_DIMENSIONS: Shortcut = Shortcut::new(Mods::NONE, Key::Space);
    pub const RESET_CAMERA: Shortcut = Shortcut::new(Mods::NONE, Key::Escape);

    pub const NEW: Shortcut = Shortcut::new(Mods::COMMAND, Key::N);
    pub const IMPORT_EXPORT: Shortcut = Shortcut::new(Mods::COMMAND, Key::E);
    pub const OPEN: Shortcut = Shortcut::new(Mods::COMMAND, Key::O);
    pub const SAVE: Shortcut = Shortcut::new(Mods::COMMAND, Key::S);
    pub const SAVE_AS: Shortcut = Shortcut::new(Mods::COMMAND.plus(Mods::SHIFT), Key::S);
    pub const QUIT: Shortcut = Shortcut::new(Mods::COMMAND, Key::Q);
}

// Native
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::init();

    eframe::run_native(
        "Portal Tool",
        eframe::NativeOptions::default(),
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

// Web
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    // Redirect `log` message to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id was not a HtmlCanvasElement");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(App::new(cc)))),
            )
            .await;

        // Remove the loading text and spinner:
        if let Some(loading_text) = document.get_element_by_id("loading_text") {
            match start_result {
                Ok(_) => {
                    loading_text.remove();
                }
                Err(e) => {
                    loading_text.set_inner_html(
                        "<p> The app has crashed. See the developer console for details. </p>",
                    );
                    panic!("Failed to start eframe: {e:?}");
                }
            }
        }
    });
}

/// User preferences, autosaved on web and desktop.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Preferences {
    show_all_labels: bool,
    show_all_arrows: bool,
    arrow_coloring: ArrowColoring,

    show_zy_plot: bool,
    show_both_portal_lists: bool,

    hover_either_dimension: bool,
    lock_portal_size: bool,
    entity: Entity,

    #[cfg(not(target_arch = "wasm32"))]
    autosave: bool,
    file_path: Option<PathBuf>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            show_all_labels: true,
            show_all_arrows: false,
            arrow_coloring: ArrowColoring::default(),

            show_zy_plot: true,
            show_both_portal_lists: false,

            hover_either_dimension: true,
            lock_portal_size: true,
            entity: Entity::PLAYER,

            #[cfg(not(target_arch = "wasm32"))]
            autosave: true,
            file_path: None,
        }
    }
}
impl Preferences {
    const STORAGE_KEY: &str = "prefs";
}

/// Application state.
#[derive(Default)]
pub struct App {
    world: World,
    camera: Camera,
    animation_state: AnimationState,

    portals_hovered: PortalHoverState,

    unsaved_changes: bool,
    last_frame_state: World,
    undo_history: Vec<World>,
    redo_history: Vec<World>,

    cached_state: (World, Entity),
    cached_links: HashMap<PortalId, (PortalLinkResult, Vec<PortalId>)>,

    prefs: Preferences,

    import_export_modal_text: Option<String>,
    cached_import_export_modal_text_deserialized: Option<serde_json::Result<World>>,

    /// Task to complete before re-enabling the UI.
    ///
    /// If this is `Some`, then the UI is disabled.
    async_task: Option<oneshot::Receiver<Result<AppAsyncTaskOk, AppAsyncTaskErr>>>,
}

impl App {
    /// Constructs the application state.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.style_mut(|style| {
            style.explanation_tooltips = true;
            style.interaction.selectable_labels = false;
        });

        let storage = cc.storage.as_ref();

        App {
            prefs: storage
                .and_then(|storage| storage.get_string(Preferences::STORAGE_KEY))
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),

            ..Default::default()
        }
    }

    /// Returns `true` if the current file is saved or if the user confirms
    /// discard.
    fn is_ok_to_discard_state(&self) -> bool {
        !self.unsaved_changes
            || rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title("Discard unsaved changes?")
                .set_buttons(rfd::MessageButtons::OkCancel)
                .show()
                == rfd::MessageDialogResult::Ok
    }

    fn reset(&mut self) {
        if self.is_ok_to_discard_state() {
            self.load(World::default());
        }
    }
    fn load(&mut self, world: World) {
        self.world = world.clone();
        self.last_frame_state = world;
        self.undo_history = vec![];
        self.redo_history = vec![];
        self.unsaved_changes = false;
        self.prefs.file_path = None;
    }

    fn toggle_import_export(&mut self) {
        match serde_json::to_string_pretty(&self.world) {
            Ok(s) => self.import_export_modal_text = Some(s),
            Err(e) => show_error_dialog(("Export error", e)),
        }
    }

    fn open(&mut self) {
        if !self.is_ok_to_discard_state() {
            return;
        }
        self.spawn_async_task(async move || {
            match rfd::AsyncFileDialog::new()
                .add_filter("JSON", &["json"])
                .pick_file()
                .await
            {
                Some(file_handle) => {
                    let contents = file_handle.read().await;
                    let world = serde_json::from_slice(&contents)
                        .map_err(|e| ("Error deserializing file", e))?;
                    Ok(AppAsyncTaskOk::Load {
                        #[cfg(not(target_arch = "wasm32"))]
                        path: Some(file_handle.path().to_path_buf()),
                        #[cfg(target_arch = "wasm32")]
                        path: None,
                        world,
                    })
                }
                None => Ok(AppAsyncTaskOk::None),
            }
        });
    }
    fn save(&mut self) {
        self.save_internal(self.prefs.file_path.clone());
    }
    fn save_as(&mut self) {
        self.save_internal(None);
    }
    fn save_internal(&mut self, path: Option<PathBuf>) {
        let serialization_result = serde_json::to_string_pretty(&self.world);
        self.spawn_async_task(async move || {
            let contents_to_write =
                serialization_result.map_err(|e| ("Error serializing file", e))?;

            if let Some(path) = path {
                std::fs::write(&path, &contents_to_write)
                    .map_err(|e| ("Error saving to file", e))?;
                return Ok(AppAsyncTaskOk::MarkSaved {
                    path: (!IS_WEB).then_some(path),
                });
            }

            let out = rfd::AsyncFileDialog::new()
                .add_filter("JSON", &["json"])
                .save_file()
                .await;
            match out {
                Some(file_handle) => {
                    file_handle
                        .write(contents_to_write.as_bytes())
                        .await
                        .map_err(|e| ("Error saving file", e))?;
                    Ok(AppAsyncTaskOk::MarkSaved {
                        #[cfg(not(target_arch = "wasm32"))]
                        path: Some(file_handle.path().to_path_buf()),
                        #[cfg(target_arch = "wasm32")]
                        path: None,
                    })
                }
                None => Ok(AppAsyncTaskOk::None),
            }
        });
    }

    fn spawn_async_task<
        F: 'static + AsyncSafe + Future<Output = Result<AppAsyncTaskOk, AppAsyncTaskErr>>,
    >(
        &mut self,
        f: impl FnOnce() -> F,
    ) {
        if self.async_task.is_some() {
            log::error!("cannot spawn async task; one is already running");
        }
        let (tx, rx) = oneshot::channel();
        let task = f();
        self.async_task = Some(rx);
        threads::spawn(async { tx.send(task.await).expect("channel disconnected") });
    }

    fn undo(&mut self) {
        if let Some(new_state) = self.undo_history.pop() {
            let old_state = std::mem::replace(&mut self.world, new_state);
            self.last_frame_state = self.world.clone();
            self.redo_history.push(old_state);
        }
    }
    fn redo(&mut self) {
        if let Some(new_state) = self.redo_history.pop() {
            let old_state = std::mem::replace(&mut self.world, new_state);
            self.last_frame_state = self.world.clone();
            self.undo_history.push(old_state);
        }
    }

    fn toggle_camera_dimension(&mut self) {
        self.set_camera_dimension(self.camera.dimension.other());
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

    fn show_all_portal_lists(&mut self, ui: &mut egui::Ui) {
        self.portals_hovered.in_list = None;
        if self.prefs.show_both_portal_lists {
            if ui.available_width() >= 800.0 {
                ui.columns(2, |uis| {
                    uis[0].group(|ui| self.show_portal_list(ui, Overworld, true));
                    uis[1].group(|ui| self.show_portal_list(ui, Nether, true));
                });
            } else {
                ui.group(|ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("portal_list")
                        .show(ui, |ui| {
                            self.show_portal_list(ui, Overworld, false);
                            ui.add(egui::Separator::default().grow(6.0));
                            self.show_portal_list(ui, Nether, false);
                        });
                });
            }
        } else {
            ui.group(|ui| self.show_portal_list(ui, self.camera.dimension, true));
        }
    }

    fn show_entity_config(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            coordinate_label(ui, "Width");
            ui.add(
                egui::DragValue::new(&mut self.prefs.entity.width)
                    .range(0.0..=16.0)
                    .speed(0.01),
            );
            coordinate_label(ui, "Height");
            ui.add(
                egui::DragValue::new(&mut self.prefs.entity.height)
                    .range(0.0..=16.0)
                    .speed(0.01),
            );
            ui.checkbox(&mut self.prefs.entity.is_projectile, "Projectile")
                .on_hover_text(include_str!("text/projectile.txt").trim());
        });

        ui.separator();

        ui.horizontal_wrapped(|ui| {
            for (name, entity) in [
                ("Player", Entity::PLAYER),
                ("Ghast", Entity::GHAST),
                ("Item", Entity::ITEM),
                ("Arrow", Entity::ARROW),
                ("Ender pearl", Entity::ENDER_PEARL),
            ] {
                let mut atoms = egui::Atoms::new(name);
                atoms.push_right(egui::Atom::grow());
                atoms.push_right(egui::RichText::new(format!("{entity:.02}")).small());
                ui.selectable_value(&mut self.prefs.entity, entity, name);
            }
        });
    }

    fn show_portal_list(&mut self, ui: &mut egui::Ui, dimension: Dimension, scrollable: bool) {
        ui.horizontal(|ui| {
            ui.set_min_height(26.0);

            ui.scope(|ui| {
                if big_img_button(ui, egui::include_image!("img/portal-plus.svg"))
                    .on_hover_text("Add portal")
                    .clicked()
                {
                    match dimension {
                        Overworld => self.add_portal_in_overworld(),
                        Nether => self.add_portal_in_nether(),
                    }
                }

                if big_img_button(ui, egui::include_image!("img/map-marker-plus.svg"))
                    .on_hover_text("Add test point")
                    .clicked()
                {
                    self.world.test_points[dimension].push(self.camera.pos);
                }
            });

            let mut new_camera_dimension = self.camera.dimension;
            for dim in [Overworld, Nether] {
                if !self.prefs.show_both_portal_lists || dim == dimension {
                    ui.selectable_value(
                        &mut new_camera_dimension,
                        dim,
                        egui::RichText::new(dim.to_string()).heading(),
                    );
                }
            }
            self.set_camera_dimension(new_camera_dimension);
        });

        let portals_by_id = self.world.portals[dimension.other()]
            .iter()
            .map(|p| (p.id, p.clone()))
            .collect::<HashMap<PortalId, Portal>>();

        if !self.world.test_points[dimension].is_empty() {
            ui.separator();
        }

        self.world.test_points[dimension].retain_mut(|test_point| {
            let mut keep = true;

            egui::Sides::new().shrink_left().show(
                ui,
                |ui| {
                    if img_button(ui, egui::include_image!("img/crosshairs.svg"))
                        .on_hover_text("Show in plot")
                        .clicked()
                    {
                        self.camera.pos =
                            test_point.convert_dimension(dimension, self.camera.dimension);
                    }

                    show_world_pos_edit(ui, test_point, Some(3));

                    let destination_portals = self
                        .world
                        .portals
                        .entity_destinations(dimension, *test_point)
                        .iter()
                        .map(|p| p.id)
                        .collect_vec();

                    if destination_portals.is_empty() {
                        ui.colored_label(ui.visuals().error_fg_color, "Generates new portal");
                    } else {
                        let mut label_atoms = egui::Atoms::new("Links to: ");
                        push_portal_list_text(
                            ui,
                            &mut label_atoms,
                            &destination_portals,
                            &portals_by_id,
                        );
                        ui.add(egui::AtomLayout::new(label_atoms));
                    }
                },
                |ui| {
                    keep = !img_button(ui, egui::include_image!("img/delete.svg"))
                        .on_hover_text("Delete test point")
                        .clicked();
                },
            );

            keep
        });

        let mut reorder_drag_start = None;
        let mut reorder_drag_end = None;
        let mut remove = None;
        let mut show_in_plot = None;
        let mut show_portal_list_contents = |ui: &mut egui::Ui| {
            for (i, portal) in self.world.portals[dimension].iter_mut().enumerate() {
                ui.separator();

                const OUTLINE_WIDTH: f32 = 2.0;

                let r = egui::Frame::new()
                    .outer_margin(OUTLINE_WIDTH)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
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
                                            if img_button(
                                                ui,
                                                egui::include_image!("img/crosshairs.svg"),
                                            )
                                            .on_hover_text("Show in plot")
                                            .clicked()
                                            {
                                                show_in_plot = Some(i);
                                            }

                                            ui.color_edit_button_srgb(&mut portal.color);

                                            ui.horizontal(|ui| {
                                                egui::TextEdit::singleline(&mut portal.name)
                                                    .hint_text("Portal name")
                                                    .show(ui);
                                            });
                                        },
                                        |ui| {
                                            if img_button(
                                                ui,
                                                egui::include_image!("img/delete.svg"),
                                            )
                                            .on_hover_text("Delete test point")
                                            .clicked()
                                            {
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
                                            self.prefs.lock_portal_size,
                                            dimension,
                                        );

                                        portal.adjust_max(
                                            |max| show_block_pos_edit(ui, max),
                                            self.prefs.lock_portal_size,
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
                            });

                            reorder_drag_rect.max.y = ui.min_rect().max.y;
                            let r = ui.interact(
                                reorder_drag_rect,
                                egui::Id::new(portal.id).with("reorder"),
                                egui::Sense::drag(),
                            );
                            let color;
                            if r.dragged() {
                                reorder_drag_start = Some(i);
                                self.portals_hovered.in_list = Some(portal.id);
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
                let rect_contains = |p: Option<_>| p.is_some_and(|it| rect.contains(it));
                let hovering_this = ui.input(|input| rect_contains(input.pointer.interact_pos()));
                let dragging = ui.input(|input| input.pointer.is_decidedly_dragging());
                if hovering_this {
                    if dragging {
                        reorder_drag_end = Some(i);
                    } else {
                        self.portals_hovered.in_list = Some(portal.id);
                    }
                }

                if self.portals_hovered.contains(portal.id) {
                    let [red, g, b] = portal.color;
                    ui.painter().rect_stroke(
                        r.response.rect,
                        4.0,
                        (OUTLINE_WIDTH, egui::Color32::from_rgb(red, g, b)),
                        egui::StrokeKind::Outside,
                    );
                }
                if self.portals_hovered.in_plot.iter().exactly_one().ok() == Some(&portal.id) {
                    r.response.scroll_to_me(None);
                }
            }
        };
        if scrollable {
            egui::ScrollArea::vertical()
                .id_salt(("portal_list", dimension))
                .auto_shrink([false; 2])
                .show(ui, show_portal_list_contents);
        } else {
            // can't use `ScrollArea::new([false; 2])` because then
            // `scroll_to_me()` wouldn't work.
            show_portal_list_contents(ui);
        }

        if let Some(i) = show_in_plot {
            self.set_camera_dimension(dimension);
            self.camera.pos = WorldRegion::from(self.world.portals[dimension][i].region).center();
        }
        if let (Some(i), Some(j)) = (reorder_drag_start, reorder_drag_end) {
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

        let mut plot = egui_plot::Plot::new(("plot", plane))
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
            self.show_test_points_in_plot(plot_ui, plane);
        });

        if let Some(hovered_world_pos) = r
            .response
            .hover_pos()
            .filter(|&pos| r.transform.frame().contains(pos))
            .map(|pos| r.transform.value_from_position(pos))
            .map(|point| plane.plot_to_world(point, *new_camera))
        {
            if self.prefs.hover_either_dimension {
                self.process_portal_hovers(Overworld, plane, hovered_world_pos);
                self.process_portal_hovers(Nether, plane, hovered_world_pos);
            } else {
                self.process_portal_hovers(new_camera.dimension, plane, hovered_world_pos);
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

        if self.portals_hovered.contains(portal.id) {
            if let Some(region) = portal.entity_collision_region(self.prefs.entity) {
                let region = WorldRegion::from(
                    region
                        .convert_dimension(portal_dimension, portal_dimension.other())
                        .block_region_containing(),
                )
                .convert_dimension(portal_dimension.other(), plot_dimension);

                let a = plane.world_to_plot(region.min);
                let b = plane.world_to_plot(region.max);
                let points = vec![[a.x, a.y], [a.x, b.y], [b.x, b.y], [b.x, a.y]];

                plot_ui
                    .add(egui_plot::Polygon::new("", points).stroke((1.0, egui::Color32::WHITE)));
            }
        }

        if !portal.name.is_empty() {
            let mut job = egui::text::LayoutJob::default();
            job.append(
                if self.prefs.show_all_labels || self.portals_hovered.contains(portal.id) {
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
        if !self.prefs.show_all_arrows && self.portals_hovered.is_empty() {
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
                if self.prefs.show_all_arrows
                    || self.portals_hovered.contains(*id1)
                    || self.portals_hovered.contains(*id2)
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

    fn dpos_dvalue_x(&self, plot_ui: &mut egui_plot::PlotUi<'_>) -> f32 {
        // can't use `plot_ui.dpos_dvalue_x()` because it doesn't use the
        // updated transform
        plot_ui.transform().frame().width() / self.camera.width as f32
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

        let dpos_dvalue_x = self.dpos_dvalue_x(plot_ui);

        // Shrink arrow by half a block
        let vector =
            (dst_point.to_pos2() - src_point.to_pos2()).normalized() * 8.0 / dpos_dvalue_x.sqrt();
        src_point.x += vector.x as f64;
        src_point.y += vector.y as f64;
        dst_point.x -= vector.x as f64;
        dst_point.y -= vector.y as f64;

        let [r, g, b] = match self.prefs.arrow_coloring {
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

    fn show_test_points_in_plot(&self, plot_ui: &mut egui_plot::PlotUi<'_>, plane: Plane) {
        let dpos_dvalue_x = self.dpos_dvalue_x(plot_ui);
        for dim in [Overworld, Nether] {
            for &test_point in &self.world.test_points[dim] {
                let plot_point =
                    plane.world_to_plot(test_point.convert_dimension(dim, self.camera.dimension));
                let destination_portals = self.world.portals.entity_destinations(dim, test_point);
                let [r, g, b] = match destination_portals.first() {
                    Some(p) => p.color,
                    None => [255, 0, 0], // red (error)
                };
                plot_ui.add(
                    egui_plot::Points::new("", egui_plot::PlotPoints::Owned(vec![plot_point]))
                        .shape(egui_plot::MarkerShape::Diamond)
                        .radius(dpos_dvalue_x.sqrt() / self.camera.dimension.scale() as f32 * 3.0)
                        .color(egui::Color32::from_rgb(r, g, b)),
                );
            }
        }
    }

    fn process_portal_hovers(&mut self, dimension: Dimension, plane: Plane, hovered_pos: WorldPos) {
        let WorldPos { x, y, z } = hovered_pos;
        for portal in &self.world.portals[dimension] {
            let WorldRegion { min, max } = WorldRegion::from(portal.region)
                .convert_dimension(dimension, self.camera.dimension);
            let x_range = min.x..=max.x;
            let y_range = min.y..=max.y;
            let z_range = min.z..=max.z;
            let is_hovering_portal = match plane {
                Plane::XY => x_range.contains(&x) && y_range.contains(&y),
                Plane::XZ => x_range.contains(&x) && z_range.contains(&z),
                Plane::ZY => z_range.contains(&z) && y_range.contains(&y),
            };
            if is_hovering_portal {
                self.portals_hovered.in_plot_for_next_frame.push(portal.id);
            }
        }
    }

    fn calculate_portal_link_result(
        &self,
        portal: &Portal,
        portal_dimension: Dimension,
    ) -> PortalLinkResult {
        let destination_dimension = portal_dimension.other();
        let Some(entry_region) = portal.entity_collision_region(self.prefs.entity) else {
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

    fn show_menu_bar(
        &mut self,
        ui: &mut egui::Ui,
        collapse_menu: bool,
        collapse_camera_controls: bool,
    ) {
        fn menu_no_autoclose<'a>(
            ui: &mut egui::Ui,
            atoms: impl egui::IntoAtoms<'a>,
            add_contents: impl FnOnce(&mut egui::Ui),
        ) {
            let config = egui::containers::menu::MenuConfig::new()
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
            if egui::containers::menu::is_in_menu(ui) {
                egui::containers::menu::SubMenuButton::new(atoms)
                    .config(config)
                    .ui(ui, add_contents);
            } else {
                egui::containers::menu::MenuButton::new(atoms)
                    .config(config)
                    .ui(ui, add_contents);
            }
        }

        fn button_with_kbd(
            ui: &mut egui::Ui,
            text: &str,
            shortcut: &egui::KeyboardShortcut,
        ) -> egui::Response {
            egui::Button::new(text)
                .shortcut_text(ui.ctx().format_shortcut(shortcut))
                .ui(ui)
        }

        egui::MenuBar::new().ui(ui, |ui| {
            let mut menu_contents = |ui: &mut egui::Ui| {
                menu_no_autoclose(ui, "File", |ui| {
                    if button_with_kbd(ui, "New", &kbd_shortcuts::NEW).clicked() {
                        self.reset();
                        ui.close();
                    }
                    ui.separator();
                    if button_with_kbd(ui, "Open…", &kbd_shortcuts::OPEN).clicked() {
                        self.open();
                        ui.close();
                    }
                    ui.separator();
                    if button_with_kbd(ui, "Save", &kbd_shortcuts::SAVE).clicked() {
                        self.save();
                        ui.close();
                    }
                    if button_with_kbd(ui, "Save As…", &kbd_shortcuts::SAVE_AS).clicked() {
                        self.save_as();
                        ui.close();
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        ui.separator();
                        ui.checkbox(&mut self.prefs.autosave, "Auto Save");
                    }
                    ui.separator();
                    if button_with_kbd(ui, "Import/Export…", &kbd_shortcuts::IMPORT_EXPORT)
                        .clicked()
                    {
                        self.toggle_import_export();
                        ui.close();
                    }

                    // no File->Quit on web pages
                    if !IS_WEB {
                        #[cfg(target_os = "macos")]
                        let exit_text = "Quit";
                        #[cfg(not(target_os = "macos"))]
                        let exit_text = "Exit";

                        ui.separator();
                        if button_with_kbd(ui, exit_text, &kbd_shortcuts::QUIT).clicked() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            ui.close();
                        }
                    }
                });

                menu_no_autoclose(ui, "Edit", |ui| {
                    ui.add_enabled_ui(!self.undo_history.is_empty(), |ui| {
                        if ui.button("Undo").clicked() {
                            self.undo();
                        }
                    });
                    ui.add_enabled_ui(!self.redo_history.is_empty(), |ui| {
                        if ui.button("Redo").clicked() {
                            self.redo();
                        }
                    });
                });

                menu_no_autoclose(ui, "View", |ui| {
                    let button = egui::Button::new("Switch dimension")
                        .shortcut_text(ui.ctx().format_shortcut(&kbd_shortcuts::SWITCH_DIMENSIONS));
                    if ui.add(button).clicked() {
                        self.toggle_camera_dimension();
                        ui.close();
                    }

                    let button = egui::Button::new("Reset camera")
                        .shortcut_text(ui.ctx().format_shortcut(&kbd_shortcuts::RESET_CAMERA));
                    if ui.add(button).clicked() {
                        self.camera.reset();
                        ui.close();
                    }

                    ui.separator();

                    ui.checkbox(&mut self.prefs.show_zy_plot, "Show ZY Plot");
                    ui.checkbox(
                        &mut self.prefs.show_both_portal_lists,
                        "Show Both Portal Lists",
                    );

                    ui.separator();

                    ui.checkbox(&mut self.prefs.show_all_labels, "Show Portal Labels");
                    ui.checkbox(&mut self.prefs.show_all_arrows, "Show Link Arrows");
                    ui.horizontal(|ui| {
                        ui.strong("Color arrows by");
                        ui.selectable_value(
                            &mut self.prefs.arrow_coloring,
                            ArrowColoring::BySource,
                            "Source",
                        );
                        ui.selectable_value(
                            &mut self.prefs.arrow_coloring,
                            ArrowColoring::ByDestination,
                            "Destination",
                        );
                    });
                });

                menu_no_autoclose(ui, "Settings", |ui| {
                    ui.checkbox(
                        &mut self.prefs.hover_either_dimension,
                        "Hover Portals In Both Dimensions",
                    )
                    .on_hover_text(include_str!("text/hover_either_dimension.txt").trim());
                    ui.checkbox(
                        &mut self.prefs.lock_portal_size,
                        "Lock Portal Size When Editing",
                    )
                    .on_hover_text(include_str!("text/lock_portal_size.txt").trim());
                    ui.separator();
                    egui::global_theme_preference_buttons(ui);
                    ui.separator();
                    if ui.button("Reset all settings").clicked() {
                        self.prefs = Preferences::default();
                    };
                });
            };

            if collapse_menu {
                ui.menu_button("Menu", menu_contents);
            } else {
                menu_contents(ui);
            }

            ui.separator();

            let mut camera_controls_contents = |ui: &mut egui::Ui| {
                ui.horizontal(|ui| {
                    let mut new_camera_dimension = self.camera.dimension;
                    for dim in [Overworld, Nether] {
                        ui.selectable_value(&mut new_camera_dimension, dim, dim.to_string());
                    }
                    self.set_camera_dimension(new_camera_dimension);
                });

                if !collapse_camera_controls {
                    ui.separator();
                }

                ui.horizontal(|ui| {
                    if img_button(ui, egui::include_image!("img/home.svg"))
                        .on_hover_text("Reset camera")
                        .clicked()
                    {
                        self.camera.reset();
                    }

                    show_world_pos_edit(ui, &mut self.camera.pos, Some(0));
                });
            };

            if collapse_camera_controls {
                menu_no_autoclose(ui, "Position", camera_controls_contents);
            } else {
                camera_controls_contents(ui);
            }

            ui.separator();

            menu_no_autoclose(ui, "Entity size", |ui| self.show_entity_config(ui));

            ui_unless_overflow(ui, |ui| ui.small(format!("{:#.02}", self.prefs.entity)));
        });
    }

    fn show_import_export_modal(&mut self, ctx: &egui::Context) {
        if let Some(mut text) = self.import_export_modal_text.take() {
            let r = egui::Modal::new(egui::Id::new("import_export")).show(ctx, |ui| {
                let r = egui::ScrollArea::vertical()
                    .max_width(ui.ctx().screen_rect().width() / 2.0)
                    .max_height(ui.ctx().screen_rect().height() / 4.0)
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.with_layout(
                            egui::Layout::top_down(egui::Align::LEFT)
                                .with_cross_justify(true)
                                .with_main_justify(true),
                            |ui| {
                                egui::TextEdit::multiline(&mut text)
                                    .clip_text(false)
                                    .show(ui)
                                    .response
                            },
                        )
                        .inner
                    })
                    .inner;
                if r.changed() {
                    self.cached_import_export_modal_text_deserialized = None;
                }

                let deserialized = self
                    .cached_import_export_modal_text_deserialized
                    .take()
                    .unwrap_or_else(|| serde_json::from_str(&text));

                match &deserialized {
                    Ok(_) => ui.label(""),
                    Err(e) => ui.colored_label(ui.visuals().error_fg_color, e.to_string()),
                };

                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        ui.close();
                    }

                    if ui
                        .add_enabled(deserialized.is_ok(), egui::Button::new("Import"))
                        .clicked()
                        && let Ok(world) = &deserialized
                        && self.is_ok_to_discard_state()
                    {
                        self.load(world.clone());
                        ui.close();
                    }
                });

                self.cached_import_export_modal_text_deserialized = Some(deserialized);
            });

            if r.should_close() {
                self.cached_import_export_modal_text_deserialized = None;
            } else {
                self.import_export_modal_text = Some(text);
            }
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        match serde_json::to_string_pretty(&self.prefs) {
            Ok(prefs_str) => storage.set_string(Preferences::STORAGE_KEY, prefs_str),
            Err(e) => log::error!("error saving preferences: {e}"),
        }
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        for e in &mut raw_input.events {
            if let egui::Event::MouseWheel { delta, .. } = e {
                *delta *= SCROLL_SENSITIVITY;
            }
        }
    }

    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|input| input.viewport().close_requested()) && !self.is_ok_to_discard_state() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        // Disable the whole UI if there is a dialog open.
        let mut disable_everything = false;
        if let Some(async_task) = self.async_task.take() {
            match async_task.try_recv() {
                // still waiting
                Err(TryRecvError::Empty) => {
                    disable_everything = true;
                    self.async_task = Some(async_task);
                }
                // async task crashed, probably
                Err(TryRecvError::Disconnected) => {
                    show_error_dialog(("Error", "Channel disconnected"));
                }
                // async task succeeded
                Ok(Ok(ok)) => match ok {
                    AppAsyncTaskOk::None => (),
                    AppAsyncTaskOk::MarkSaved { path } => {
                        self.unsaved_changes = false;
                        self.prefs.file_path = path;
                    }
                    AppAsyncTaskOk::Load { path, world } => {
                        self.load(world);
                        self.prefs.file_path = path;
                    }
                },
                // async task failed
                Ok(Err(e)) => show_error_dialog(e),
            }
        }

        egui_extras::install_image_loaders(ctx); // ok to call every frame

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            if disable_everything {
                ui.disable();
            }

            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            ui_unless_overflow(ui, |ui| self.show_menu_bar(ui, false, false))
                .or_else(|| ui_unless_overflow(ui, |ui| self.show_menu_bar(ui, false, true)))
                .unwrap_or_else(|| self.show_menu_bar(ui, true, true));
        });

        egui::TopBottomPanel::bottom("bottom_bar").show(ctx, |ui| {
            if disable_everything {
                ui.disable();
            }

            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            ui.spacing_mut().scroll.bar_width /= 1.5;
            ui.spacing_mut().scroll.bar_inner_margin = 0.0;
            let sp = std::mem::take(&mut ui.spacing_mut().item_spacing);
            ui.horizontal(|ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("bottom_bar")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        show_credits(ui);
                        ui.add_space(sp.x);
                        ui.separator();
                        ui.add_space(sp.x);
                        show_powered_by_egui(ui);
                        ui.add_space(sp.x);
                        ui.separator();
                        ui.add_space(sp.x);
                        show_source_code_link(ui);
                        ui.add_space(sp.x);
                        ui.separator();
                        ui.add_space(sp.x);
                        ui.small("MDI icons by Google and Simran B.");
                    });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if disable_everything {
                ui.disable();
            }

            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();

            let mut new_camera = self.camera;

            let r = ui.available_rect_before_wrap();
            let center = r.center().round_ui();
            let egui::Vec2 { x, y } = (r.size() / 2.0).floor_ui();

            use egui::{Rect, vec2};
            let m = PLOT_MARGIN / 2.0;
            let left_bottom = Rect::from_two_pos(center + vec2(-x, y), center + vec2(-m, m));
            let right_bottom = Rect::from_two_pos(center + vec2(x, y), center + vec2(m, m));
            let left_top = Rect::from_two_pos(center + vec2(-x, -y), center + vec2(-m, -m));
            let right_top = Rect::from_two_pos(center + vec2(x, -y), center + vec2(m, -m));

            self.portals_hovered.in_plot =
                std::mem::take(&mut self.portals_hovered.in_plot_for_next_frame);
            for (plane, rect) in [
                (Plane::XY, left_bottom),
                (Plane::ZY, right_bottom),
                (Plane::XZ, left_top),
            ] {
                if !self.prefs.show_zy_plot && plane == Plane::ZY {
                    continue;
                }
                ui.put(rect, |ui: &mut egui::Ui| {
                    ui.group(|ui| self.show_view(ui, plane, &mut new_camera))
                        .response
                });
            }
            self.camera = new_camera;
            let now = web_time::Instant::now();
            if !self.animation_state.is_static() {
                ctx.request_repaint();
            }
            self.animation_state
                .step((now - self.animation_state.last_frame).as_secs_f64());
            self.animation_state.last_frame = now;

            let controls_rect = if self.prefs.show_zy_plot {
                right_top
            } else {
                right_top.union(right_bottom)
            };
            ui.scope_builder(egui::UiBuilder::new().max_rect(controls_rect), |ui| {
                self.show_all_portal_lists(ui);
            });

            let is_text_field_active = ui.ctx().wants_keyboard_input();
            ui.input_mut(|input| {
                if !input.pointer.is_decidedly_dragging() && !is_text_field_active {
                    if self.last_frame_state != self.world {
                        self.unsaved_changes = true;
                        let old_state =
                            std::mem::replace(&mut self.last_frame_state, self.world.clone());
                        self.redo_history.clear();
                        self.undo_history.push(old_state);
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if self.prefs.autosave && self.unsaved_changes {
                        self.save();
                    }

                    // Consume the most specific shortcut first
                    if input.consume_shortcut(&kbd_shortcuts::CMD_SHIFT_Z)
                        || input.consume_shortcut(&kbd_shortcuts::CMD_Y)
                    {
                        self.redo();
                        self.save();
                    } else if input.consume_shortcut(&kbd_shortcuts::CMD_Z) {
                        self.undo();
                        self.save();
                    }

                    if input.consume_shortcut(&kbd_shortcuts::SWITCH_DIMENSIONS) {
                        self.toggle_camera_dimension();
                    }

                    if input.consume_shortcut(&kbd_shortcuts::RESET_CAMERA) {
                        self.camera.reset();
                    }

                    if input.consume_shortcut(&kbd_shortcuts::NEW) {
                        self.reset();
                    }
                    if input.consume_shortcut(&kbd_shortcuts::IMPORT_EXPORT) {
                        self.toggle_import_export();
                    }
                    if input.consume_shortcut(&kbd_shortcuts::OPEN) {
                        self.open();
                    }
                    if input.consume_shortcut(&kbd_shortcuts::SAVE) {
                        self.save();
                    }
                    if input.consume_shortcut(&kbd_shortcuts::SAVE_AS) {
                        self.save_as();
                    }
                    if input.consume_shortcut(&kbd_shortcuts::QUIT) && self.is_ok_to_discard_state()
                    {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            });
        });

        self.show_import_export_modal(ctx);

        let (cached_world, cached_entity) = &self.cached_state;
        if (cached_world, cached_entity) != (&self.world, &self.prefs.entity) {
            let t = web_time::Instant::now();
            self.cached_state = (self.world.clone(), self.prefs.entity);
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
    fixed_decimals: Option<usize>,
) -> egui::Response {
    let make_drag_value = |value| {
        let dv = egui::DragValue::new(value).speed(0.1);
        match fixed_decimals {
            Some(num_decimals) => dv.fixed_decimals(num_decimals),
            None => dv,
        }
    };

    ui.horizontal(|ui| {
        coordinate_label(ui, "X");
        ui.add(make_drag_value(x));

        coordinate_label(ui, "Y");
        ui.add(make_drag_value(y).range(Overworld.y_min()..=Overworld.y_max() + 1));

        coordinate_label(ui, "Z");
        ui.add(make_drag_value(z));
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
    portals_by_id: &HashMap<PortalId, Portal>,
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
    portals_by_id: &HashMap<PortalId, Portal>,
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
    last_frame: web_time::Instant,
    aspect_ratio_scale: f64,
}
impl Default for AnimationState {
    fn default() -> Self {
        Self {
            last_frame: web_time::Instant::now(),
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

#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
enum ArrowColoring {
    #[default]
    BySource,
    ByDestination,
}

#[derive(Debug, Default, Clone)]
struct PortalHoverState {
    in_list: Option<PortalId>,
    in_plot: Vec<PortalId>,
    in_plot_for_next_frame: Vec<PortalId>,
}
impl PortalHoverState {
    fn is_empty(&self) -> bool {
        self.in_list.is_none() && self.in_plot.is_empty()
    }
    fn contains(&self, id: PortalId) -> bool {
        self.in_list == Some(id) || self.in_plot.contains(&id)
    }
}

fn show_credits(ui: &mut egui::Ui) {
    ui.label(format!("{TITLE} v{} by ", env!("CARGO_PKG_VERSION")));
    ui.hyperlink_to("Andrew Farkas", "https://ajfarkas.dev/");
}

fn show_powered_by_egui(ui: &mut egui::Ui) {
    ui.label("Powered by ");
    ui.hyperlink_to("egui", "https://github.com/emilk/egui");
    ui.label(" and ");
    ui.hyperlink_to(
        "eframe",
        "https://github.com/emilk/egui/tree/master/crates/eframe",
    );
}

fn show_source_code_link(ui: &mut egui::Ui) {
    ui.hyperlink_to(
        egui::RichText::new(" source code").small(),
        env!("CARGO_PKG_REPOSITORY"),
    );
}

/// Returns `true` if the UI fits in the available space, or `false` if it needs
/// more space.
///
/// Uses a sizing pass. No visible UI is displayed.
fn does_ui_fit(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui)) -> bool {
    let available_size = ui.available_size();
    let r = ui
        .new_child(egui::UiBuilder::new().sizing_pass().invisible())
        .scope(f)
        .response
        .rect;
    // Allow overflow by one pixel in case of rounding error.
    r.width() < available_size.x + 1.0 && r.height() < available_size.y + 1.0
}
/// Shows UI using `f` if it can be done in the available space (determined
/// automatically using a sizing pass), or displays nothing and returns `None`
/// if the UI does not fit.
///
/// Always returns `None` if `ui.is_sizing_pass()`.
fn ui_unless_overflow<R>(ui: &mut egui::Ui, mut f: impl FnMut(&mut egui::Ui) -> R) -> Option<R> {
    if ui.is_sizing_pass() {
        return None;
    }

    does_ui_fit(ui, |ui| {
        f(ui);
    })
    .then(|| f(ui))
}

fn big_img_button(ui: &mut egui::Ui, source: egui::ImageSource<'_>) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding.y = ui.spacing().button_padding.x;
        ui.add(
            egui::Button::new(egui::Image::new(source).tint(ui.visuals().strong_text_color()))
                .frame_when_inactive(false),
        )
    })
    .inner
}
fn img_button(ui: &mut egui::Ui, source: egui::ImageSource<'_>) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding.x = ui.spacing().button_padding.y;
        ui.add(
            egui::Button::new(egui::Image::new(source).tint(ui.visuals().strong_text_color()))
                .frame_when_inactive(false),
        )
    })
    .inner
}

/// Task to complete before re-enabling the UI.
enum AppAsyncTaskOk {
    /// No action needed.
    None,
    /// File has been saved; clear the "unsaved" flag.
    MarkSaved { path: Option<PathBuf> },
    /// Load world from file.
    Load { path: Option<PathBuf>, world: World },
}
/// Error message dialog to display before re-enabling the UI.
struct AppAsyncTaskErr {
    title: String,
    description: String,
}
impl<T: ToString, D: ToString> From<(T, D)> for AppAsyncTaskErr {
    fn from((title, description): (T, D)) -> Self {
        Self {
            title: title.to_string(),
            description: description.to_string(),
        }
    }
}

fn show_error_dialog(e: impl Into<AppAsyncTaskErr>) {
    let e = e.into();
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(e.title)
        .set_description(e.description)
        .show();
}
