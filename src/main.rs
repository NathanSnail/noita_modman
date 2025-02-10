use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Noita Mod Manager",
        options,
        Box::new(|cc| {
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Ok(Box::<App>::default())
        }),
    )
}

enum GitHost {
    Github,
    Gitlab,
    Other,
}

struct GitMod {
    remote: Option<String>,
}

struct SteamMod {
    workshop_id: String,
}

struct ModWorkshopMod {
    link: String,
}

enum ModSource {
    Git(GitMod),
    Steam(SteamMod),
    ModWorkshop(ModWorkshopMod),
    Manual,
}

enum ModKind {
    Normal(bool),
    Translation,
    Gamemode,
}

struct Mod {
    source: ModSource,
    kind: ModKind,
    name: String,
    description: String,
    unsafe_api: bool,
}

impl Mod {
    fn render(&self, ui: &mut egui::Ui) {
        ui.label(&self.name);
    }
}

struct App {
    search: String,
    mods: Vec<Mod>,
}

impl Default for App {
    fn default() -> Self {
        return App {
            search: "".to_owned(),
            mods: Vec::new(),
        };
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Mod Manager");
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.text_edit_singleline(&mut self.search);
            });
            egui::SidePanel::right("right").show_inside(ui, |ui| {
                ui.label("right");
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        for i in 1..=1000 {
                            ui.label("hi ".to_owned() + &i.to_string());
                        }
                    })
            });
            egui::CentralPanel::default().show_inside(ui, |ui| {
                for nmod in self.mods.iter() {
                    nmod.render(ui);
                }
            });
        });
    }
}
