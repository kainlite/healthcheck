use axum::{routing::get, Router};
use serde_json::json;
use std::{
    sync::atomic::{AtomicU32, Ordering},
    thread::sleep,
    time::Duration,
};
use tower_service::Service;
use worker::*;

static FAILED_CHECKS: AtomicU32 = AtomicU32::new(0);
const MAX_FAILURES: u32 = 3;

fn router() -> Router {
    Router::new().route("/", get(root))
}

#[event(fetch)]
async fn fetch(
    req: HttpRequest,
    _env: Env,
    _ctx: Context,
) -> Result<axum::http::Response<axum::body::Body>> {
    console_error_panic_hook::set_once();
    Ok(router().call(req).await?)
}

pub async fn root() -> &'static str {
    "Hello Axum!"
}

#[event(scheduled)]
pub async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    // Create a fetch request to our health endpoint
    let url = env
        .var("HEALTH_CHECK_URL")
        .expect("HEALTH_CHECK_URL must be set")
        .to_string();

    let req = Request::new_with_init(&url, RequestInit::new().with_method(Method::Get))
        .expect("Failed to create request");

    match Fetch::Request(req).send().await {
        Ok(resp) => {
            if resp.status_code() == 200 {
                // Reset counter on successful check
                FAILED_CHECKS.store(0, Ordering::SeqCst);
            } else {
                handle_failed_check(&env).await;
            }
        }
        Err(_) => {
            handle_failed_check(&env).await;
        }
    }
}

async fn handle_failed_check(env: &Env) {
    let current_failures = FAILED_CHECKS.fetch_add(1, Ordering::SeqCst) + 1;

    // Wait for a few seconds before checking again
    sleep(Duration::from_secs(30));

    if current_failures >= MAX_FAILURES {
        if let Ok(webhook_url) = env.var("SLACK_WEBHOOK_URL") {
            let message = json!({
                "content": "🚨 Health check failed 3 times in a row! Please check the service."
            });

            let req = Request::new_with_init(
                webhook_url.to_string().as_str(),
                RequestInit::new()
                    .with_method(Method::Post)
                    .with_body(Some(serde_json::to_string(&message).unwrap().into())),
            )
            .expect("Failed to create webhook request");
            let _ = Fetch::Request(req).send().await;
        }

        // Reset counter after notification
        FAILED_CHECKS.store(0, Ordering::SeqCst);
    }
}
