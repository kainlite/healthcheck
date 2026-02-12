use serde_json::json;
use std::sync::atomic::{AtomicU32, Ordering};
use worker::*;

static FAILED_CHECKS: AtomicU32 = AtomicU32::new(0);
const MAX_FAILURES: u32 = 3;

#[event(scheduled)]
pub async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    // Create a fetch request to our health endpoint
    let url = env
        .var("HEALTH_CHECK_URL")
        .expect("HEALTH_CHECK_URL must be set")
        .to_string();

    let headers = Headers::new();
    headers
        .set("User-Agent", "Mozilla/5.0 (compatible; HealthCheck/1.0)")
        .expect("Failed to set User-Agent header");

    let req = Request::new_with_init(
        &url,
        RequestInit::new()
            .with_method(Method::Get)
            .with_headers(headers),
    )
    .expect("Failed to create request");

    match Fetch::Request(req).send().await {
        Ok(resp) => {
            if resp.status_code() == 200 {
                // Reset counter on successful check
                console_log!("Health check successful for {}! 🎉", url);
                FAILED_CHECKS.store(0, Ordering::SeqCst);
            } else {
                console_log!(
                    "Health check failed with status code: {}, for: {}",
                    resp.status_code(),
                    url
                );
                handle_failed_check(&env).await;
            }
        }
        Err(e) => {
            console_log!("Health check failed with error: {:?}, for: {}", e, url);
            handle_failed_check(&env).await;
        }
    }
}

async fn handle_failed_check(env: &Env) {
    let url = env
        .var("HEALTH_CHECK_URL")
        .expect("HEALTH_CHECK_URL must be set")
        .to_string();
    let current_failures = FAILED_CHECKS.fetch_add(1, Ordering::SeqCst) + 1;

    console_log!(
        "Health check failed {} times for {} 😓",
        current_failures,
        url
    );

    if current_failures >= MAX_FAILURES {
        console_log!("Sending notification to Slack 🚨");
        match env.var("SLACK_WEBHOOK_URL") {
            Ok(webhook_url) => {
                let message = json!({
                    "text": format!("🚨 Health check failed {} times in a row for service {}! Please check the service.", current_failures, url)
                });

                let req = Request::new_with_init(
                    webhook_url.to_string().as_str(),
                    RequestInit::new()
                        .with_method(Method::Post)
                        .with_body(Some(serde_json::to_string(&message).unwrap().into())),
                )
                .expect("Failed to create webhook request");

                match Fetch::Request(req).send().await {
                    Ok(_) => console_log!("Successfully sent Slack notification"),
                    Err(e) => console_log!("Failed to send Slack notification: {:?}", e),
                }
            }
            _ => {
                console_log!("SLACK_WEBHOOK_URL not configured!");
            }
        }

        // Reset counter after notification
        FAILED_CHECKS.store(0, Ordering::SeqCst);
    }
}
