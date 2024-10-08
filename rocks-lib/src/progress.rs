use eyre::Result;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::time::Duration;

pub async fn with_spinner<F, Fut, T>(
    progress: &MultiProgress,
    message: String,
    callback: F,
) -> Result<T>
where
    F: FnOnce() -> Fut + Send,
    Fut: std::future::Future<Output = Result<T>> + Send,
    T: Send + 'static,
{
    let spinner = progress.add(ProgressBar::new_spinner());
    spinner.set_style(ProgressStyle::default_spinner().template("{spinner:.green} {msg}")?);
    spinner.enable_steady_tick(Duration::from_millis(100));
    spinner.set_message(message.clone());
    callback()
        .await
        .map_err(|err| {
            spinner.abandon_with_message(format!("{} failed: {}", message, err));
            err
        })
        .inspect(|_| {
            spinner.finish_with_message(format!("{} - Done.", message));
        })
}