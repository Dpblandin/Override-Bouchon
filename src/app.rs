use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use eframe::egui::{
    self, Align, Button, Color32, CursorIcon, FontId, Image, ImageSource, Key, Layout, Modal,
    Popup, PopupCloseBehavior, RichText, ScrollArea, Stroke, Vec2,
};
use rfd::FileDialog;

use crate::catalog::{BouchonCatalog, BouchonValidation};
use crate::deployer::{
    ActiveBouchon, HistoryEntry, detect_active_bouchon, detect_dmpconnect_dir,
    list_history_entries, resolve_bouchon_dir, resolve_history_dir,
};
use crate::platform;
use crate::validation::ValidationOutcome;
use crate::workflow::{BouchonWorkflow, WorkflowError, WorkflowOutcome};

const BACKGROUND: Color32 = Color32::from_rgb(44, 62, 80);
const SURFACE: Color32 = Color32::from_rgb(52, 73, 94);
const FIELD_BACKGROUND: Color32 = Color32::from_rgb(62, 85, 108);
const FIELD_HOVER: Color32 = Color32::from_rgb(72, 98, 124);
const ACCENT: Color32 = Color32::from_rgb(52, 152, 219);
const SUCCESS: Color32 = Color32::from_rgb(39, 174, 96);
const WARNING: Color32 = Color32::from_rgb(243, 156, 18);
const DANGER: Color32 = Color32::from_rgb(231, 76, 60);
const TEXT: Color32 = Color32::WHITE;
const MODAL_TEXT: Color32 = Color32::from_rgb(235, 240, 245);

pub struct BouchonneurApp {
    catalog: BouchonCatalog,
    selected_bouchon: Option<PathBuf>,
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
    dialog: Option<AppDialog>,
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

#[derive(Clone)]
struct AppDialog {
    title: String,
    message: String,
    tone: DialogTone,
    action: DialogAction,
}

#[derive(Clone, Copy)]
enum DialogTone {
    Success,
    Warning,
    Danger,
}

#[derive(Clone)]
enum DialogAction {
    Dismiss,
    Delete,
    Restore,
    Edit(PathBuf),
}

#[derive(Clone, Copy)]
enum DialogDecision {
    Cancel,
    Confirm,
}

impl BouchonneurApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&creation_context.egui_ctx);
        configure_style(&creation_context.egui_ctx);

        let mut app = Self {
            catalog: BouchonCatalog::new(resolve_bouchon_dir()),
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
            dialog: None,
        };
        app.refresh_bouchons();
        app
    }

    fn refresh_bouchons(&mut self) {
        let previous_selection = self.selected_bouchon.clone();

        match self.catalog.refresh() {
            Ok(()) => {
                self.selected_bouchon = previous_selection
                    .filter(|selected| self.catalog.entry(selected).is_some())
                    .or_else(|| {
                        self.catalog
                            .entries()
                            .first()
                            .map(|entry| entry.path().to_path_buf())
                    });
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
            .and_then(|path| self.catalog.entry(path))
            .map(|entry| selected_validation(entry.validation()));
    }

    fn refresh_deployment_state(&mut self) {
        let target_directory = PathBuf::from(self.dmpconnect_directory.trim());
        if !target_directory.is_dir() {
            self.active_bouchon = None;
            self.history.clear();
            return;
        }

        match detect_active_bouchon(&target_directory, &self.catalog) {
            Ok(active) => self.active_bouchon = active,
            Err(error) => {
                self.active_bouchon = None;
                self.status = Status::Error(error.to_string());
            }
        }
        match list_history_entries(&target_directory, &self.history_directory, &self.catalog) {
            Ok(history) => self.history = history,
            Err(error) => {
                self.history.clear();
                self.status = Status::Error(error.to_string());
            }
        }
    }

    fn selected_path(&self) -> Option<&Path> {
        self.selected_bouchon
            .as_deref()
            .filter(|path| self.catalog.entry(path).is_some())
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
                !self.catalog.is_empty(),
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

                let matches = self.catalog.matching_indices(&self.bouchon_filter);
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
                        let label = self.catalog.entries()[index].name();
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
            self.selected_bouchon = Some(self.catalog.entries()[index].path().to_path_buf());
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
        let target_directory = PathBuf::from(self.dmpconnect_directory.trim());
        let outcome = BouchonWorkflow::new(&self.history_directory, &self.catalog)
            .deploy(&source, &target_directory);

        match outcome {
            Ok(WorkflowOutcome::Completed(outcome)) => {
                self.show_deployment_success(&outcome.target_path, outcome.replaced_files)
            }
            Ok(WorkflowOutcome::Cancelled) => self.handle_cancelled_workflow(),
            Err(WorkflowError::Validation(error)) => {
                let message = error.to_string();
                self.selected_validation = Some(SelectedValidation::Invalid(message.clone()));
                self.show_validation_error(&source, &message);
            }
            Err(error) => self.show_error(error.to_string()),
        }
    }

    fn show_deployment_success(&mut self, target_path: &Path, replaced_files: usize) {
        let message = format!(
            "Bouchon déployé dans '{}'. {replaced_files} ancien(s) fichier(s) remplacé(s).",
            target_path.display(),
        );
        self.refresh_deployment_state();
        self.status = Status::Success(message.clone());
        self.dialog = Some(AppDialog {
            title: "Déploiement réussi".to_owned(),
            message,
            tone: DialogTone::Success,
            action: DialogAction::Dismiss,
        });
    }

    fn request_delete_existing(&mut self) {
        let target_directory = PathBuf::from(self.dmpconnect_directory.trim());
        if !target_directory.is_dir() {
            self.show_error("Le répertoire DmpConnect-JS2 est invalide.".to_owned());
            return;
        }

        self.dialog = Some(AppDialog {
            title: "Supprimer le bouchon existant ?".to_owned(),
            message: "Tous les fichiers .do présents dans le répertoire seront supprimés. Une sauvegarde sera conservée dans l’historique.".to_owned(),
            tone: DialogTone::Danger,
            action: DialogAction::Delete,
        });
    }

    fn delete_existing(&mut self) {
        let target_directory = PathBuf::from(self.dmpconnect_directory.trim());
        let outcome =
            BouchonWorkflow::new(&self.history_directory, &self.catalog).delete(&target_directory);
        match outcome {
            Ok(WorkflowOutcome::Completed(count)) => {
                self.refresh_deployment_state();
                self.status = Status::Success(format!("{count} fichier(s) .do supprimé(s)."));
            }
            Ok(WorkflowOutcome::Cancelled) => self.handle_cancelled_workflow(),
            Err(error) => self.show_error(error.to_string()),
        }
    }

    fn request_restore_previous(&mut self) {
        let Some(previous) = self.history.first() else {
            self.show_error("Aucune version précédente n'est disponible.".to_owned());
            return;
        };
        let previous_name = previous
            .source_name
            .as_deref()
            .unwrap_or("bouchon non identifié");
        self.dialog = Some(AppDialog {
            title: "Restaurer la version précédente ?".to_owned(),
            message: format!("Le bouchon actif sera remplacé par « {previous_name} »."),
            tone: DialogTone::Warning,
            action: DialogAction::Restore,
        });
    }

    fn restore_previous(&mut self) {
        let target_directory = PathBuf::from(self.dmpconnect_directory.trim());
        let outcome =
            BouchonWorkflow::new(&self.history_directory, &self.catalog).restore(&target_directory);
        match outcome {
            Ok(WorkflowOutcome::Completed(outcome)) => {
                self.show_restoration_success(outcome.restored_files)
            }
            Ok(WorkflowOutcome::Cancelled) => self.handle_cancelled_workflow(),
            Err(error) => self.show_error(error.to_string()),
        }
    }

    fn show_restoration_success(&mut self, restored_files: usize) {
        let message = format!("Version précédente restaurée ({restored_files} fichier(s)).");
        self.refresh_deployment_state();
        self.status = Status::Success(message.clone());
        self.dialog = Some(AppDialog {
            title: "Restauration réussie".to_owned(),
            message,
            tone: DialogTone::Success,
            action: DialogAction::Dismiss,
        });
    }

    fn handle_cancelled_workflow(&mut self) {
        self.refresh_deployment_state();
        self.status = Status::Ready;
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
            self.request_restore_previous();
        }
    }

    fn show_selected_validation(&self, ui: &mut egui::Ui) {
        let Some(validation) = &self.selected_validation else {
            return;
        };
        let (message, color, validation_icon) = match validation {
            SelectedValidation::Valid(message) => (
                message,
                SUCCESS,
                egui::include_image!("../assets/icons/active.svg"),
            ),
            SelectedValidation::Warning(message) => (
                message,
                WARNING,
                egui::include_image!("../assets/icons/info.svg"),
            ),
            SelectedValidation::Invalid(message) => (
                message,
                DANGER,
                egui::include_image!("../assets/icons/info.svg"),
            ),
        };
        ui.horizontal(|ui| {
            ui.add(
                Image::new(validation_icon)
                    .fit_to_exact_size(Vec2::splat(16.0))
                    .tint(color),
            );
            ui.label(RichText::new(message).color(color).size(12.0));
        });
    }

    fn show_validation_error(&mut self, path: &Path, message: &str) {
        self.status = Status::Error(message.to_owned());
        self.dialog = Some(AppDialog {
            title: "Bouchon invalide".to_owned(),
            message: format!("{message}\n\nTu peux ouvrir le fichier pour le corriger."),
            tone: DialogTone::Danger,
            action: DialogAction::Edit(path.to_path_buf()),
        });
    }

    fn show_error(&mut self, message: String) {
        self.status = Status::Error(message.clone());
        self.dialog = Some(AppDialog {
            title: "Une erreur est survenue".to_owned(),
            message,
            tone: DialogTone::Danger,
            action: DialogAction::Dismiss,
        });
    }

    fn show_dialog(&mut self, context: &egui::Context) {
        let Some(dialog) = self.dialog.clone() else {
            return;
        };
        let (color, image) = match dialog.tone {
            DialogTone::Success => (SUCCESS, egui::include_image!("../assets/icons/active.svg")),
            DialogTone::Warning => (WARNING, egui::include_image!("../assets/icons/restore.svg")),
            DialogTone::Danger => (DANGER, egui::include_image!("../assets/icons/info.svg")),
        };
        let frame = egui::Frame::new()
            .fill(SURFACE)
            .stroke(Stroke::new(1.0, color.gamma_multiply(0.7)))
            .corner_radius(16)
            .inner_margin(24);
        let response = Modal::new(egui::Id::new("app-dialog"))
            .backdrop_color(Color32::from_black_alpha(170))
            .frame(frame)
            .show(context, |ui| {
                ui.set_width(430.0);
                ui.horizontal(|ui| {
                    egui::Frame::new()
                        .fill(color.gamma_multiply(0.22))
                        .corner_radius(24)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.add(
                                Image::new(image)
                                    .fit_to_exact_size(Vec2::splat(26.0))
                                    .tint(color),
                            );
                        });
                    ui.add_space(6.0);
                    ui.label(RichText::new(&dialog.title).size(20.0).color(TEXT).strong());
                });
                ui.add_space(14.0);
                ui.add(
                    egui::Label::new(RichText::new(&dialog.message).size(14.0).color(MODAL_TEXT))
                        .wrap(),
                );
                ui.add_space(20.0);

                let mut decision = None;
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (primary_label, primary_color) = match dialog.action {
                        DialogAction::Dismiss => ("OK", ACCENT),
                        DialogAction::Delete => ("Supprimer", DANGER),
                        DialogAction::Restore => ("Restaurer", WARNING),
                        DialogAction::Edit(_) => ("Éditer le fichier", WARNING),
                    };
                    if dialog_button(ui, primary_label, primary_color, true).clicked() {
                        decision = Some(DialogDecision::Confirm);
                    }
                    if !matches!(dialog.action, DialogAction::Dismiss)
                        && dialog_button(ui, "Annuler", FIELD_BACKGROUND, false).clicked()
                    {
                        decision = Some(DialogDecision::Cancel);
                    }
                });
                decision
            });

        let decision = response
            .inner
            .or_else(|| response.should_close().then_some(DialogDecision::Cancel));
        let Some(decision) = decision else {
            return;
        };
        self.dialog = None;
        if matches!(decision, DialogDecision::Cancel) {
            return;
        }

        match dialog.action {
            DialogAction::Dismiss => {}
            DialogAction::Delete => self.delete_existing(),
            DialogAction::Restore => self.restore_previous(),
            DialogAction::Edit(path) => {
                if let Err(error) = platform::open_path(&path) {
                    self.show_error(format!(
                        "Impossible d'ouvrir '{}' : {error}",
                        path.display()
                    ));
                }
            }
        }
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
                                    self.catalog.directory().display()
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
                                self.request_delete_existing();
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
        self.show_dialog(ui.ctx());
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

fn dialog_button(ui: &mut egui::Ui, label: &str, color: Color32, strong: bool) -> egui::Response {
    let text = if strong {
        RichText::new(label).color(TEXT).strong()
    } else {
        RichText::new(label).color(TEXT)
    };
    ui.add_sized([140.0, 40.0], Button::new(text).fill(color))
        .on_hover_cursor(CursorIcon::PointingHand)
}

fn selected_validation(validation: &BouchonValidation) -> SelectedValidation {
    match validation {
        BouchonValidation::Valid(outcome) => validation_from_outcome(outcome.clone()),
        BouchonValidation::Invalid(message) => SelectedValidation::Invalid(message.clone()),
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
