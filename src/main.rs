#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bouchonneur::app::BouchonneurApp;
use bouchonneur::privileged::{
    PrivilegedResponse, execute_privileged_command, parse_privileged_invocation,
    write_privileged_response,
};
use eframe::egui;
use std::env;
use std::process;

fn main() -> eframe::Result {
    match parse_privileged_invocation(env::args_os().skip(1)) {
        Ok(Some(invocation)) => {
            let outcome = execute_privileged_command(invocation.command);
            if let Some(result_file) = &invocation.result_file {
                let response = match &outcome {
                    Ok(affected_files) => PrivilegedResponse::Success(*affected_files),
                    Err(error) => PrivilegedResponse::Error(error.to_string()),
                };
                if let Err(error) = write_privileged_response(result_file, &response) {
                    eprintln!(
                        "Impossible d'écrire le résultat administrateur dans '{}': {error}",
                        result_file.display()
                    );
                    process::exit(3);
                }
            }

            match outcome {
                Ok(affected_files) => {
                    if invocation.result_file.is_none() {
                        println!("{affected_files}");
                    }
                }
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
            .with_inner_size([1_100.0, 820.0])
            .with_min_inner_size([760.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Bouchonneur - Déployeur de bouchons",
        options,
        Box::new(|creation_context| Ok(Box::new(BouchonneurApp::new(creation_context)))),
    )
}
