use std::path::{Path, PathBuf};

use eframe::egui::{
    self, Align, Button, Color32, ComboBox, FontId, Layout, RichText, Stroke, Vec2,
};
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

use crate::deployer::{
    delete_existing_bouchons, deploy_bouchon, detect_dmpconnect_dir, list_bouchons,
    resolve_bouchon_dir,
};
use crate::platform;

const BACKGROUND: Color32 = Color32::from_rgb(44, 62, 80);
const SURFACE: Color32 = Color32::from_rgb(52, 73, 94);
const ACCENT: Color32 = Color32::from_rgb(52, 152, 219);
const SUCCESS: Color32 = Color32::from_rgb(39, 174, 96);
const WARNING: Color32 = Color32::from_rgb(243, 156, 18);
const DANGER: Color32 = Color32::from_rgb(231, 76, 60);
const TEXT: Color32 = Color32::WHITE;

pub struct BouchonneurApp {
    bouchon_directory: PathBuf,
    bouchons: Vec<PathBuf>,
    selected_bouchon: Option<usize>,
    dmpconnect_directory: String,
    status: Status,
}

enum Status {
    Ready,
    Success(String),
    Error(String),
}

impl BouchonneurApp {
    pub fn new(creation_context: &eframe::CreationContext<'_>) -> Self {
        configure_style(&creation_context.egui_ctx);

        let mut app = Self {
            bouchon_directory: resolve_bouchon_dir(),
            bouchons: Vec::new(),
            selected_bouchon: None,
            dmpconnect_directory: detect_dmpconnect_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
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
    }

    fn selected_path(&self) -> Option<&Path> {
        self.selected_bouchon
            .and_then(|index| self.bouchons.get(index))
            .map(PathBuf::as_path)
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

        match deploy_bouchon(&source, &target_directory) {
            Ok(outcome) => {
                let message = format!(
                    "Bouchon déployé dans '{}'. {} ancien(s) fichier(s) remplacé(s).",
                    outcome.target_path.display(),
                    outcome.replaced_files
                );
                self.status = Status::Success(message.clone());
                show_message("Déploiement réussi", &message, MessageLevel::Info);
            }
            Err(error) => self.show_error(error.to_string()),
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

        match delete_existing_bouchons(&target_directory) {
            Ok(count) => {
                self.status = Status::Success(format!("{count} fichier(s) .do supprimé(s)."));
            }
            Err(error) => self.show_error(error.to_string()),
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
                        RichText::new(format!("Dossier : {}", self.bouchon_directory.display()))
                            .size(12.0)
                            .color(ACCENT),
                    );

                    let selected_text = self
                        .selected_path()
                        .and_then(Path::file_name)
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Aucun fichier disponible".to_owned());

                    ComboBox::from_id_salt("bouchon-selection")
                        .selected_text(selected_text)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (index, path) in self.bouchons.iter().enumerate() {
                                let label = path.file_name().unwrap_or_default().to_string_lossy();
                                ui.selectable_value(&mut self.selected_bouchon, Some(index), label);
                            }
                        });

                    ui.horizontal(|ui| {
                        if colored_button(ui, "Rafraîchir la liste", ACCENT, 180.0).clicked() {
                            self.refresh_bouchons();
                        }
                        if colored_button(ui, "Éditer le fichier", WARNING, 180.0).clicked() {
                            self.edit_selected_bouchon();
                        }
                        ui.add_enabled(false, Button::new("Infos JDD"))
                            .on_disabled_hover_text("Lien JDD à configurer");
                    });
                });

                ui.add_space(12.0);

                section(ui, "Répertoire DmpConnect-JS2", |ui| {
                    ui.label("Chemin du dossier DmpConnect-JS2 :");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.dmpconnect_directory)
                            .desired_width(f32::INFINITY),
                    );
                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        if colored_button(ui, "Parcourir...", WARNING, 160.0).clicked() {
                            self.browse_dmpconnect_directory();
                        }
                    });
                });

                ui.add_space(14.0);
                let action_width = 300.0 + 340.0 + ui.spacing().item_spacing.x;
                let action_indent = ((ui.available_width() - action_width) / 2.0).max(0.0);
                ui.horizontal(|ui| {
                    ui.add_space(action_indent);
                    if colored_button(ui, "DÉPLOYER LE BOUCHON", SUCCESS, 300.0).clicked() {
                        self.deploy_selected_bouchon();
                    }
                    if colored_button(ui, "SUPPRIMER LE BOUCHON EXISTANT", DANGER, 340.0).clicked()
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
    }
}

fn configure_style(context: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.widgets.inactive.bg_fill = SURFACE;
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

fn colored_button(ui: &mut egui::Ui, label: &str, color: Color32, width: f32) -> egui::Response {
    ui.add_sized(
        [width, 44.0],
        Button::new(RichText::new(label).color(TEXT).strong()).fill(color),
    )
}

fn show_message(title: &str, description: &str, level: MessageLevel) {
    MessageDialog::new()
        .set_title(title)
        .set_description(description)
        .set_level(level)
        .set_buttons(MessageButtons::Ok)
        .show();
}
