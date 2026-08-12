use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use eframe::egui::{
    self, Align, Button, Color32, CursorIcon, FontId, Image, ImageSource, Key, Layout, Popup,
    PopupCloseBehavior, RichText, ScrollArea, Stroke, Vec2,
};
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::deployer::TARGET_NAME;
use crate::deployer::{
    ActiveBouchon, HistoryEntry, create_history_entry, delete_existing_bouchons, deploy_bouchon,
    detect_active_bouchon, detect_dmpconnect_dir, discard_history_entry, list_bouchons,
    list_history_entries, resolve_bouchon_dir, resolve_history_dir, restore_latest_history,
};
use crate::platform;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::platform::ElevationError;
use crate::validation::{ValidationError, ValidationOutcome, validate_bouchon};

const BACKGROUND: Color32 = Color32::from_rgb(44, 62, 80);
const SURFACE: Color32 = Color32::from_rgb(52, 73, 94);
const FIELD_BACKGROUND: Color32 = Color32::from_rgb(62, 85, 108);
const FIELD_HOVER: Color32 = Color32::from_rgb(72, 98, 124);
const ACCENT: Color32 = Color32::from_rgb(52, 152, 219);
const SUCCESS: Color32 = Color32::from_rgb(39, 174, 96);
const WARNING: Color32 = Color32::from_rgb(243, 156, 18);
const DANGER: Color32 = Color32::from_rgb(231, 76, 60);
const TEXT: Color32 = Color32::WHITE;

pub struct BouchonneurApp {
    bouchon_directory: PathBuf,
    bouchons: Vec<PathBuf>,
    selected_bouchon: Option<usize>,
    bouchon_filter: String,
    bouchon_selector_open: bool,
    focus_bouchon_filter: bool,
    highlighted_bouchon: usize,
    selected_validation: Option<SelectedValidation>,
    dmpconnect_directory: String,
    history_directory: PathBuf,
    active_bouchon: Option<ActiveBouchon>,
    history: Vec<HistoryEntry>,
    status: Status,
}

enum Status {
    Ready,
    Success(String),
    Error(String),
}

#[derive(Debug, Clone)]
enum SelectedValidation {
    Valid(String),
    Warning(String),
    Invalid(String),
}

impl BouchonneurApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&creation_context.egui_ctx);
        configure_style(&creation_context.egui_ctx);

        let mut app = Self {
            bouchon_directory: resolve_bouchon_dir(),
            bouchons: Vec::new(),
            selected_bouchon: None,
            bouchon_filter: String::new(),
            bouchon_selector_open: false,
            focus_bouchon_filter: false,
            highlighted_bouchon: 0,
            selected_validation: None,
            dmpconnect_directory: detect_dmpconnect_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            history_directory: resolve_history_dir(),
            active_bouchon: None,
            history: Vec::new(),
            status: Status::Ready,
        };
        app.refresh_bouchons();
        app
    }

    fn refresh_bouchons(&mut self) {
        let previous_selection = self.selected_path().map(Path::to_path_buf);

        match list_bouchons(&self.bouchon_directory) {
            Ok(bouchons) => {
                self.bouchons = bouchons;
                self.selected_bouchon = previous_selection
                    .and_then(|selected| self.bouchons.iter().position(|path| path == &selected))
                    .or_else(|| (!self.bouchons.is_empty()).then_some(0));
                self.status = Status::Ready;
            }
            Err(error) => self.show_error(error.to_string()),
        }
        self.refresh_selected_validation();
        self.refresh_deployment_state();
    }

    fn refresh_selected_validation(&mut self) {
        self.selected_validation = self
            .selected_path()
            .map(|path| selected_validation(validate_bouchon(path)));
    }

    fn refresh_deployment_state(&mut self) {
        let target_directory = PathBuf::from(self.dmpconnect_directory.trim());
        if !target_directory.is_dir() {
            self.active_bouchon = None;
            self.history.clear();
            return;
        }

        match detect_active_bouchon(&target_directory, &self.bouchons) {
            Ok(active) => self.active_bouchon = active,
            Err(error) => {
                self.active_bouchon = None;
                self.status = Status::Error(error.to_string());
            }
        }
        match list_history_entries(&target_directory, &self.history_directory, &self.bouchons) {
            Ok(history) => self.history = history,
            Err(error) => {
                self.history.clear();
                self.status = Status::Error(error.to_string());
            }
        }
    }

    fn selected_path(&self) -> Option<&Path> {
        self.selected_bouchon
            .and_then(|index| self.bouchons.get(index))
            .map(PathBuf::as_path)
    }

    fn show_bouchon_selector(&mut self, ui: &mut egui::Ui) {
        let selector_width = ui.available_width();
        let selected_text = self
            .selected_path()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Aucun fichier disponible".to_owned());

        let selector = ui
            .add_enabled(
                !self.bouchons.is_empty(),
                Button::image_and_text(
                    icon(egui::include_image!("../assets/icons/search.svg")),
                    selected_text,
                )
                .right_text(
                    Image::new(egui::include_image!("../assets/icons/chevron-down.svg"))
                        .fit_to_exact_size(Vec2::splat(14.0)),
                )
                .min_size(Vec2::new(selector_width, 38.0))
                .truncate(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);

        let mut open = self.bouchon_selector_open;
        if selector.clicked() {
            open = !open;
            if open {
                self.bouchon_filter.clear();
                self.highlighted_bouchon = 0;
                self.focus_bouchon_filter = true;
            }
        }

        let mut chosen = None;
        let mut close_requested = false;
        Popup::from_response(&selector)
            .open_bool(&mut open)
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
            .width(selector.rect.width())
            .show(|ui| {
                ui.set_min_width(selector.rect.width() - 16.0);
                let previous_filter = self.bouchon_filter.clone();
                let arrow_down = ui.input(|input| input.key_pressed(Key::ArrowDown));
                let arrow_up = ui.input(|input| input.key_pressed(Key::ArrowUp));
                let enter = ui.input(|input| input.key_pressed(Key::Enter));
                let escape = ui.input(|input| input.key_pressed(Key::Escape));
                let filter_response = ui.add(
                    egui::TextEdit::singleline(&mut self.bouchon_filter)
                        .hint_text("Rechercher un bouchon…")
                        .margin(egui::Margin::symmetric(10, 7))
                        .desired_width(f32::INFINITY),
                );

                if self.focus_bouchon_filter {
                    filter_response.request_focus();
                    self.focus_bouchon_filter = false;
                }
                if self.bouchon_filter != previous_filter {
                    self.highlighted_bouchon = 0;
                }

                let matches = matching_bouchon_indices(&self.bouchons, &self.bouchon_filter);
                let last_match = matches.len().saturating_sub(1);
                self.highlighted_bouchon = self.highlighted_bouchon.min(last_match);

                if filter_response.has_focus() || filter_response.lost_focus() {
                    if arrow_down {
                        self.highlighted_bouchon = (self.highlighted_bouchon + 1).min(last_match);
                    }
                    if arrow_up {
                        self.highlighted_bouchon = self.highlighted_bouchon.saturating_sub(1);
                    }
                    if enter {
                        chosen = matches.get(self.highlighted_bouchon).copied();
                    }
                    if escape {
                        close_requested = true;
                    }
                }

                ui.add_space(2.0);
                ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                    if matches.is_empty() {
                        ui.label(RichText::new("Aucun bouchon trouvé").weak());
                    }

                    for (position, index) in matches.iter().copied().enumerate() {
                        let label = self.bouchons[index]
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();
                        let response = ui
                            .selectable_label(position == self.highlighted_bouchon, label)
                            .on_hover_cursor(CursorIcon::PointingHand);
                        if response.hovered() {
                            self.highlighted_bouchon = position;
                        }
                        if response.clicked() {
                            chosen = Some(index);
                        }
                    }
                });
            });

        if let Some(index) = chosen {
            self.selected_bouchon = Some(index);
            self.bouchon_filter.clear();
            self.refresh_selected_validation();
            open = false;
        }
        if close_requested {
            open = false;
        }
        self.bouchon_selector_open = open;
    }

    fn browse_dmpconnect_directory(&mut self) {
        let mut dialog = FileDialog::new();
        let current = Path::new(&self.dmpconnect_directory);
        if current.is_dir() {
            dialog = dialog.set_directory(current);
        }

        if let Some(directory) = dialog.pick_folder() {
            self.dmpconnect_directory = directory.display().to_string();
            self.status = Status::Ready;
            self.refresh_deployment_state();
        }
    }

    fn edit_selected_bouchon(&mut self) {
        let Some(path) = self.selected_path().map(Path::to_path_buf) else {
            self.show_error("Veuillez sélectionner un fichier bouchon.".to_owned());
            return;
        };

        if let Err(error) = platform::open_path(&path) {
            self.show_error(format!("Impossible d'ouvrir '{}': {error}", path.display()));
        }
    }

    fn deploy_selected_bouchon(&mut self) {
        let Some(source) = self.selected_path().map(Path::to_path_buf) else {
            self.show_error("Veuillez sélectionner un fichier bouchon.".to_owned());
            return;
        };
        match validate_bouchon(&source) {
            Ok(outcome) => self.selected_validation = Some(validation_from_outcome(outcome)),
            Err(error) => {
                let message = error.to_string();
                self.selected_validation = Some(SelectedValidation::Invalid(message.clone()));
                self.show_validation_error(&source, &message);
                return;
            }
        }
        let target_directory = PathBuf::from(self.dmpconnect_directory.trim());
        let history_entry = match create_history_entry(
            &target_directory,
            &self.history_directory,
            &self.bouchons,
        ) {
            Ok(entry) => entry,
            Err(error) => {
                self.show_error(error.to_string());
                return;
            }
        };

        match deploy_bouchon(&source, &target_directory) {
            Ok(outcome) => {
                self.show_deployment_success(&outcome.target_path, outcome.replaced_files)
            }
            Err(error) => {
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                if error.is_permission_denied() {
                    self.deploy_with_administrator_privileges(
                        &source,
                        &target_directory,
                        history_entry.as_ref(),
                    );
                    return;
                }

                discard_history(history_entry.as_ref());
                self.show_error(error.to_string());
            }
        }
    }

    fn show_deployment_success(&mut self, target_path: &Path, replaced_files: usize) {
        let message = format!(
            "Bouchon déployé dans '{}'. {replaced_files} ancien(s) fichier(s) remplacé(s).",
            target_path.display(),
        );
        self.refresh_deployment_state();
        self.status = Status::Success(message.clone());
        show_message("Déploiement réussi", &message, MessageLevel::Info);
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn deploy_with_administrator_privileges(
        &mut self,
        source: &Path,
        target_directory: &Path,
        history_entry: Option<&HistoryEntry>,
    ) {
        match platform::deploy_with_administrator_privileges(source, target_directory) {
            Ok(replaced_files) => {
                self.show_deployment_success(&target_directory.join(TARGET_NAME), replaced_files)
            }
            Err(ElevationError::Cancelled) => {
                discard_history(history_entry);
                self.refresh_deployment_state();
                self.status = Status::Ready;
            }
            Err(error) => {
                discard_history(history_entry);
                self.refresh_deployment_state();
                self.show_error(error.to_string());
            }
        }
    }

    fn confirm_and_delete_existing(&mut self) {
        let target_directory = PathBuf::from(self.dmpconnect_directory.trim());
        if !target_directory.is_dir() {
            self.show_error("Le répertoire DmpConnect-JS2 est invalide.".to_owned());
            return;
        }

        let confirmed = MessageDialog::new()
            .set_title("Supprimer le bouchon existant ?")
            .set_description("Tous les fichiers .do présents dans le répertoire seront supprimés.")
            .set_level(MessageLevel::Warning)
            .set_buttons(MessageButtons::YesNo)
            .show()
            == MessageDialogResult::Yes;

        if !confirmed {
            return;
        }

        let history_entry = match create_history_entry(
            &target_directory,
            &self.history_directory,
            &self.bouchons,
        ) {
            Ok(entry) => entry,
            Err(error) => {
                self.show_error(error.to_string());
                return;
            }
        };

        match delete_existing_bouchons(&target_directory) {
            Ok(count) => {
                self.refresh_deployment_state();
                self.status = Status::Success(format!("{count} fichier(s) .do supprimé(s)."));
            }
            Err(error) => {
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                if error.is_permission_denied() {
                    self.delete_with_administrator_privileges(
                        &target_directory,
                        history_entry.as_ref(),
                    );
                    return;
                }

                discard_history(history_entry.as_ref());
                self.show_error(error.to_string());
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn delete_with_administrator_privileges(
        &mut self,
        target_directory: &Path,
        history_entry: Option<&HistoryEntry>,
    ) {
        match platform::delete_with_administrator_privileges(target_directory) {
            Ok(count) => {
                self.refresh_deployment_state();
                self.status = Status::Success(format!("{count} fichier(s) .do supprimé(s)."));
            }
            Err(ElevationError::Cancelled) => {
                discard_history(history_entry);
                self.refresh_deployment_state();
                self.status = Status::Ready;
            }
            Err(error) => {
                discard_history(history_entry);
                self.refresh_deployment_state();
                self.show_error(error.to_string());
            }
        }
    }

    fn confirm_and_restore_previous(&mut self) {
        let target_directory = PathBuf::from(self.dmpconnect_directory.trim());
        let Some(previous) = self.history.first() else {
            self.show_error("Aucune version précédente n'est disponible.".to_owned());
            return;
        };
        let previous_name = previous
            .source_name
            .as_deref()
            .unwrap_or("bouchon non identifié");
        let confirmed = MessageDialog::new()
            .set_title("Restaurer la version précédente ?")
            .set_description(format!(
                "Le bouchon actif sera remplacé par « {previous_name} »."
            ))
            .set_level(MessageLevel::Warning)
            .set_buttons(MessageButtons::YesNo)
            .show()
            == MessageDialogResult::Yes;

        if !confirmed {
            return;
        }

        match restore_latest_history(&target_directory, &self.history_directory) {
            Ok(outcome) => self.show_restoration_success(outcome.restored_files),
            Err(error) => {
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                if error.is_permission_denied() {
                    self.restore_with_administrator_privileges(&target_directory);
                    return;
                }

                self.show_error(error.to_string());
            }
        }
    }

    fn show_restoration_success(&mut self, restored_files: usize) {
        let message = format!("Version précédente restaurée ({restored_files} fichier(s)).");
        self.refresh_deployment_state();
        self.status = Status::Success(message.clone());
        show_message("Restauration réussie", &message, MessageLevel::Info);
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn restore_with_administrator_privileges(&mut self, target_directory: &Path) {
        match platform::restore_with_administrator_privileges(
            &self.history_directory,
            target_directory,
        ) {
            Ok(restored_files) => self.show_restoration_success(restored_files),
            Err(ElevationError::Cancelled) => self.status = Status::Ready,
            Err(error) => self.show_error(error.to_string()),
        }
    }

    fn show_active_bouchon(&mut self, ui: &mut egui::Ui) {
        match &self.active_bouchon {
            Some(active) => {
                let name = active
                    .source_name
                    .as_deref()
                    .unwrap_or("Bouchon non identifié");
                ui.horizontal(|ui| {
                    ui.add(
                        Image::new(egui::include_image!("../assets/icons/active.svg"))
                            .fit_to_exact_size(Vec2::splat(18.0))
                            .tint(SUCCESS),
                    );
                    ui.label(RichText::new(name).strong().size(15.0));
                    if active.source_name.is_none() {
                        ui.label(RichText::new("contenu externe à la bibliothèque").weak());
                    }
                });
                ui.label(
                    RichText::new(format!(
                        "Installé {} · {}",
                        active
                            .modified_at
                            .map(format_relative_time)
                            .unwrap_or_else(|| "à une date inconnue".to_owned()),
                        active.deployed_path.display()
                    ))
                    .size(12.0)
                    .color(ACCENT),
                );
                if active.do_file_count > 1 {
                    ui.label(
                        RichText::new(format!(
                            "Attention : {} fichiers .do sont présents.",
                            active.do_file_count
                        ))
                        .color(WARNING),
                    );
                }
            }
            None => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").color(Color32::GRAY));
                    ui.label(RichText::new("Aucun bouchon actif détecté").strong());
                });
            }
        }

        ui.add_space(4.0);
        let history_description = self.history.first().map_or_else(
            || "Aucune version précédente disponible".to_owned(),
            |entry| {
                format!(
                    "{} version(s) · dernière sauvegarde : {} ({})",
                    self.history.len(),
                    entry
                        .source_name
                        .as_deref()
                        .unwrap_or("bouchon non identifié"),
                    format_relative_time(entry.created_at)
                )
            },
        );
        ui.label(RichText::new(history_description).weak().size(12.0));

        let can_restore = !self.history.is_empty();
        let restore = ui.add_enabled(
            can_restore,
            Button::image_and_text(
                icon(egui::include_image!("../assets/icons/restore.svg")),
                "Restaurer la version précédente",
            )
            .fill(WARNING),
        );
        let restore = if can_restore {
            restore.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            restore.on_disabled_hover_text("L'historique est vide")
        };
        if restore.clicked() {
            self.confirm_and_restore_previous();
        }
    }

    fn show_selected_validation(&self, ui: &mut egui::Ui) {
        let Some(validation) = &self.selected_validation else {
            return;
        };
        let (message, color) = match validation {
            SelectedValidation::Valid(message) => (message, SUCCESS),
            SelectedValidation::Warning(message) => (message, WARNING),
            SelectedValidation::Invalid(message) => (message, DANGER),
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new("●").color(color));
            ui.label(RichText::new(message).color(color).size(12.0));
        });
    }

    fn show_validation_error(&mut self, path: &Path, message: &str) {
        self.status = Status::Error(message.to_owned());
        let edit_requested = MessageDialog::new()
            .set_title("Bouchon invalide")
            .set_description(format!(
                "{message}\n\nVoulez-vous ouvrir le fichier pour le corriger ?"
            ))
            .set_level(MessageLevel::Error)
            .set_buttons(MessageButtons::YesNo)
            .show()
            == MessageDialogResult::Yes;

        if edit_requested && let Err(error) = platform::open_path(path) {
            self.show_error(format!(
                "Impossible d'ouvrir '{}' : {error}",
                path.display()
            ));
        }
    }

    fn show_error(&mut self, message: String) {
        self.status = Status::Error(message.clone());
        show_message("Erreur", &message, MessageLevel::Error);
    }
}

impl eframe::App for BouchonneurApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(18))
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .id_salt("main-content")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("Bouchonneur")
                                    .font(FontId::proportional(30.0))
                                    .strong(),
                            );
                            ui.label(
                                RichText::new("Déployeur de bouchons")
                                    .size(15.0)
                                    .color(ACCENT),
                            );
                            ui.add_space(12.0);
                        });

                        section(ui, "Sélection du fichier bouchon", |ui| {
                            ui.label("Fichiers disponibles dans le dossier 'bouchons/' :");
                            ui.label(
                                RichText::new(format!(
                                    "Dossier : {}",
                                    self.bouchon_directory.display()
                                ))
                                .size(12.0)
                                .color(ACCENT),
                            );

                            self.show_bouchon_selector(ui);
                            self.show_selected_validation(ui);

                            ui.horizontal(|ui| {
                                if colored_button(
                                    ui,
                                    egui::include_image!("../assets/icons/refresh.svg"),
                                    "Rafraîchir la liste",
                                    ACCENT,
                                    180.0,
                                )
                                .clicked()
                                {
                                    self.refresh_bouchons();
                                }
                                if colored_button(
                                    ui,
                                    egui::include_image!("../assets/icons/edit.svg"),
                                    "Éditer le fichier",
                                    WARNING,
                                    180.0,
                                )
                                .clicked()
                                {
                                    self.edit_selected_bouchon();
                                }
                                ui.add_enabled(
                                    false,
                                    Button::image_and_text(
                                        icon(egui::include_image!("../assets/icons/info.svg")),
                                        "Infos JDD",
                                    ),
                                )
                                .on_disabled_hover_text("Lien JDD à configurer");
                            });
                        });

                        ui.add_space(12.0);

                        section(ui, "Répertoire DmpConnect-JS2", |ui| {
                            ui.label("Chemin du dossier DmpConnect-JS2 :");
                            let path_response = ui.add(
                                egui::TextEdit::singleline(&mut self.dmpconnect_directory)
                                    .desired_width(f32::INFINITY),
                            );
                            if path_response.lost_focus() {
                                self.refresh_deployment_state();
                            }
                            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                                if colored_button(
                                    ui,
                                    egui::include_image!("../assets/icons/folder.svg"),
                                    "Parcourir...",
                                    WARNING,
                                    160.0,
                                )
                                .clicked()
                                {
                                    self.browse_dmpconnect_directory();
                                }
                            });
                        });

                        ui.add_space(12.0);

                        section(ui, "Bouchon actuellement installé", |ui| {
                            self.show_active_bouchon(ui);
                        });

                        ui.add_space(14.0);
                        let action_width = 300.0 + 340.0 + ui.spacing().item_spacing.x;
                        let action_indent = ((ui.available_width() - action_width) / 2.0).max(0.0);
                        ui.horizontal(|ui| {
                            ui.add_space(action_indent);
                            if colored_button(
                                ui,
                                egui::include_image!("../assets/icons/rocket.svg"),
                                "DÉPLOYER LE BOUCHON",
                                SUCCESS,
                                300.0,
                            )
                            .clicked()
                            {
                                self.deploy_selected_bouchon();
                            }
                            if colored_button(
                                ui,
                                egui::include_image!("../assets/icons/trash.svg"),
                                "SUPPRIMER LE BOUCHON EXISTANT",
                                DANGER,
                                340.0,
                            )
                            .clicked()
                            {
                                self.confirm_and_delete_existing();
                            }
                        });

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(4.0);
                        let (status, color) = match &self.status {
                            Status::Ready => ("Prêt à bouchonner !", TEXT),
                            Status::Success(message) => (message.as_str(), SUCCESS),
                            Status::Error(message) => (message.as_str(), DANGER),
                        };
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(status).color(color));
                        });
                        ui.add_space(3.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("Bouchonneur Rust v0.1")
                                    .size(11.0)
                                    .color(ACCENT),
                            );
                        });
                    });
            });
    }
}

fn configure_style(context: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.text_edit_bg_color = Some(FIELD_BACKGROUND);
    visuals.widgets.inactive.bg_fill = FIELD_BACKGROUND;
    visuals.widgets.hovered.bg_fill = FIELD_HOVER;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    context.set_visuals(visuals);

    context.all_styles_mut(|style| {
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        style.spacing.button_padding = Vec2::new(14.0, 9.0);
    });
}

fn section(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, Color32::from_rgb(127, 140, 141)))
        .inner_margin(12)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(16.0).strong());
            ui.add_space(4.0);
            content(ui);
        });
}

fn icon(source: ImageSource<'static>) -> Image<'static> {
    Image::new(source).fit_to_exact_size(Vec2::splat(18.0))
}

fn colored_button(
    ui: &mut egui::Ui,
    image: ImageSource<'static>,
    label: &str,
    color: Color32,
    width: f32,
) -> egui::Response {
    ui.add_sized(
        [width, 44.0],
        Button::image_and_text(icon(image), RichText::new(label).color(TEXT).strong())
            .image_tint_follows_text_color(true)
            .fill(color),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

fn matching_bouchon_indices(bouchons: &[PathBuf], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    bouchons
        .iter()
        .enumerate()
        .filter_map(|(index, path)| {
            let filename = path.file_name()?.to_string_lossy();
            (query.is_empty() || filename.to_lowercase().contains(&query)).then_some(index)
        })
        .collect()
}

fn selected_validation(result: Result<ValidationOutcome, ValidationError>) -> SelectedValidation {
    match result {
        Ok(outcome) => validation_from_outcome(outcome),
        Err(error) => SelectedValidation::Invalid(error.to_string()),
    }
}

fn validation_from_outcome(outcome: ValidationOutcome) -> SelectedValidation {
    match outcome {
        ValidationOutcome::Valid(format) => SelectedValidation::Valid(format.label().to_owned()),
        ValidationOutcome::Unchecked { extension } => {
            let format = extension.map_or_else(
                || "sans extension".to_owned(),
                |extension| format!(".{extension}"),
            );
            SelectedValidation::Warning(format!(
                "Format {format} non vérifié — déploiement autorisé"
            ))
        }
    }
}

fn discard_history(entry: Option<&HistoryEntry>) {
    if let Some(entry) = entry {
        let _ = discard_history_entry(entry);
    }
}

fn format_relative_time(time: SystemTime) -> String {
    let elapsed = SystemTime::now()
        .duration_since(time)
        .unwrap_or(Duration::ZERO);
    match elapsed.as_secs() {
        0..=59 => "à l’instant".to_owned(),
        60..=3_599 => format!("il y a {} min", elapsed.as_secs() / 60),
        3_600..=86_399 => format!("il y a {} h", elapsed.as_secs() / 3_600),
        seconds => format!("il y a {} j", seconds / 86_400),
    }
}

fn show_message(title: &str, description: &str, level: MessageLevel) {
    MessageDialog::new()
        .set_title(title)
        .set_description(description)
        .set_level(level)
        .set_buttons(MessageButtons::Ok)
        .show();
}

#[cfg(test)]
mod tests {
    use super::matching_bouchon_indices;
    use std::path::PathBuf;

    #[test]
    fn matching_bouchons_ignores_case_and_matches_anywhere() {
        let bouchons = vec![
            PathBuf::from("CasNominal.xml"),
            PathBuf::from("bouchon_test_3.json"),
            PathBuf::from("autre.pdf"),
        ];

        assert_eq!(matching_bouchon_indices(&bouchons, "TEST_3"), vec![1]);
        assert_eq!(matching_bouchon_indices(&bouchons, "nominal"), vec![0]);
    }

    #[test]
    fn empty_filter_keeps_every_bouchon() {
        let bouchons = vec![PathBuf::from("a.xml"), PathBuf::from("b.json")];

        assert_eq!(matching_bouchon_indices(&bouchons, "  "), vec![0, 1]);
    }
}
