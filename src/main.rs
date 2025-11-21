#![windows_subsystem = "windows"]

use crate::app_context::AppContext;

mod app_context;
mod consts;
mod osc_server;
mod speed_filter;
mod settings;
mod bluetooth;
mod remote;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let window_size = [300.0, 384.0];
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(&window_size)
            .with_max_inner_size(&window_size)
            .with_min_inner_size(window_size),
        ..Default::default()
    };

    let context = AppContext::new();

    eframe::run_native(
        "VibeLink",
        options,
        Box::new(|_ctx| {
            Ok(Box::new(context))
        })
    ).map_err(|e| anyhow::anyhow!("{:?}", e))?;

    Ok(())
}
