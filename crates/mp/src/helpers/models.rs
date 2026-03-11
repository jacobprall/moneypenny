use anyhow::Result;
use mp_core::config::Config;
use std::path::Path;

use crate::helpers::formatting::default_model_url;
use crate::ui;

pub async fn download_model(url: &str, dest: &Path) -> Result<()> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    if dest.exists() {
        ui::success(format!("Model already present at {}", dest.display()));
        ui::flush();
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let fname = dest
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("model");

    ui::info(format!("↓ Connecting to download {fname}..."));
    ui::flush();

    let resp = reqwest::get(url).await?;
    if !resp.status().is_success() {
        anyhow::bail!("download failed: HTTP {}", resp.status());
    }

    let total = resp.content_length();
    let size_label = match total {
        Some(n) => format!("{} MB", n / 1_000_000),
        None => "unknown size".to_string(),
    };

    ui::info(format!("↓ Downloading {fname} ({size_label})..."));
    ui::detail("This may take a few minutes on slower connections.");
    ui::flush();

    let tmp = dest.with_extension("gguf.tmp");
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_mb: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        let current_mb = downloaded / 1_000_000;
        if current_mb >= last_mb + 25 {
            last_mb = current_mb;
            if let Some(t) = total {
                let pct = (downloaded * 100) / t;
                ui::detail(format!("{current_mb} / {} MB ({pct}%)", t / 1_000_000));
            } else {
                ui::detail(format!("{current_mb} MB downloaded..."));
            }
            ui::flush();
        }
    }
    file.flush().await?;
    drop(file);

    tokio::fs::rename(&tmp, dest).await?;
    ui::success(format!("Model saved to {}", dest.display()));
    ui::flush();
    Ok(())
}

pub async fn ensure_embedding_models(config: &Config) {
    for agent in &config.agents {
        if agent.embedding.provider != "local" {
            continue;
        }
        let model_path = agent.embedding.resolve_model_path(&config.models_dir());
        let url = match default_model_url(&agent.embedding.model) {
            Some(u) => u,
            None => {
                ui::warn(format!(
                    "No download URL known for model \"{}\". Place the GGUF file at {:?} manually.",
                    agent.embedding.model, model_path
                ));
                continue;
            }
        };
        if let Err(e) = download_model(url, &model_path).await {
            ui::error(format!(
                "Failed to download model for agent \"{}\": {e}",
                agent.name
            ));
        }
    }
}
