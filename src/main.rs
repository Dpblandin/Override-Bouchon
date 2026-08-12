#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bouchonneur::app::BouchonneurApp;
use bouchonneur::privileged::{execute_privileged_command, parse_privileged_command};
use eframe::egui;
use std::env;
use std::process;

fn main() -> eframe::Result {
    match parse_privileged_command(env::args_os().skip(1)) {
        Ok(Some(command)) => {
            match execute_privileged_command(command) {
                Ok(affected_files) => println!("{affected_files}"),
                Err(error) => {
                    eprintln!("{error}");
                    process::exit(1);
                }
            }
            return Ok(());
        }
        Err(error) => {
            eprintln!("{error}");
            process::exit(2);
        }
        Ok(None) => {}
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1_100.0, 700.0])
            .with_min_inner_size([760.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Bouchonneur - Déployeur de bouchons",
        options,
        Box::new(|creation_context| Ok(Box::new(BouchonneurApp::new(creation_context)))),
    )
}
