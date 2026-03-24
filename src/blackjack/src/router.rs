//! Axum router wiring all endpoints.

use axum::{
    Router,
    routing::{delete, get, post},
};
use tower_http::trace::TraceLayer;

use crate::handlers::{
    create_game, deal_hand, delete_game, double_hand, get_game, hit_hand,
    insurance_hand, next_hand, split_hand, stand_hand,
};
use crate::session::SessionStore;

/// Build the application router.
pub fn router(store: SessionStore) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/game/new", post(create_game))
        .route("/game/{id}", get(get_game))
        .route("/game/{id}", delete(delete_game))
        .route("/game/{id}/deal", post(deal_hand))
        .route("/game/{id}/hit", post(hit_hand))
        .route("/game/{id}/stand", post(stand_hand))
        .route("/game/{id}/double", post(double_hand))
        .route("/game/{id}/split", post(split_hand))
        .route("/game/{id}/insurance", post(insurance_hand))
        .route("/game/{id}/new-hand", post(next_hand))
        .layer(TraceLayer::new_for_http())
        .with_state(store)
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({"status": "healthy"}))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn body_json(body: Body) -> serde_json::Value {
        let bytes = body.collect().await.expect("body").to_bytes();
        serde_json::from_slice(&bytes).expect("json")
    }

    #[tokio::test]
    async fn test_health() {
        let app = router(SessionStore::new());
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), 200);
        let json = body_json(resp.into_body()).await;
        assert_eq!(json["status"], "healthy");
    }

    #[tokio::test]
    async fn test_create_game() {
        let app = router(SessionStore::new());
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/game/new")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), 201);
        let json = body_json(resp.into_body()).await;
        assert!(json["session_id"].is_string());
        assert_eq!(json["phase"], "Waiting");
        assert_eq!(json["chips"], 1000);
    }

    #[tokio::test]
    async fn test_get_unknown_session() {
        let app = router(SessionStore::new());
        let fake_id = uuid::Uuid::new_v4();
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/game/{fake_id}"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), 404);
    }

    #[tokio::test]
    async fn test_deal_and_complete_flow() {
        let store = SessionStore::new();
        let app = router(store.clone());

        // Create session
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/game/new")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        let json = body_json(resp.into_body()).await;
        let id = json["session_id"].as_str().expect("id");

        // Deal
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(format!("/game/{id}/deal"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"bet":100}"#))
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), 200);
        let json = body_json(resp.into_body()).await;
        let phase = json["phase"].as_str().expect("phase");
        assert!(phase == "PlayerTurn" || phase == "Complete", "unexpected phase: {phase}");
    }

    #[tokio::test]
    async fn test_invalid_action_returns_400() {
        let store = SessionStore::new();
        let app = router(store.clone());

        // Create session
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/game/new")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        let json = body_json(resp.into_body()).await;
        let id = json["session_id"].as_str().expect("id");

        // Try to hit before dealing — must 400
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(format!("/game/{id}/hit"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), 400);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let store = SessionStore::new();
        let app = router(store.clone());

        // Create
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/game/new")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        let json = body_json(resp.into_body()).await;
        let id = json["session_id"].as_str().expect("id");

        // Delete
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri(format!("/game/{id}"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), 204);

        // GET after delete should 404
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/game/{id}"))
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resp.status(), 404);
    }

    /// The most critical enclave security invariant: the dealer's hole card must NEVER
    /// be exposed in the response during PlayerTurn.
    #[tokio::test]
    async fn test_dealer_hole_card_hidden_during_player_turn() {
        let store = SessionStore::new();
        let app = router(store.clone());

        // Create session
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/game/new")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        let json = body_json(resp.into_body()).await;
        let id = json["session_id"].as_str().expect("id");

        // Deal
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(format!("/game/{id}/deal"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"bet":100}"#))
                    .expect("req"),
            )
            .await
            .expect("resp");
        let json = body_json(resp.into_body()).await;

        if json["phase"] == "PlayerTurn" {
            // Hole card must be absent; only the face-up card is exposed
            assert!(json["dealer_hand"].is_null(), "dealer_hand must be null during PlayerTurn");
            assert!(
                !json["dealer_visible_card"].is_null(),
                "dealer_visible_card must be present during PlayerTurn"
            );
        }
        // If phase is Complete (natural BJ scenario), dealer_hand is legitimately revealed — no check needed
    }

    /// After a player stands, the round completes and the full dealer hand is revealed.
    #[tokio::test]
    async fn test_dealer_hand_revealed_after_complete() {
        let store = SessionStore::new();
        let app = router(store.clone());

        // Create + deal
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/game/new")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        let json = body_json(resp.into_body()).await;
        let id = json["session_id"].as_str().expect("id");

        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri(format!("/game/{id}/deal"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"bet":100}"#))
                    .expect("req"),
            )
            .await
            .expect("resp");
        let json = body_json(resp.into_body()).await;

        if json["phase"] == "PlayerTurn" {
            // Stand to end the round
            let resp = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri(format!("/game/{id}/stand"))
                        .body(Body::empty())
                        .expect("req"),
                )
                .await
                .expect("resp");
            let json = body_json(resp.into_body()).await;
            assert_eq!(json["phase"], "Complete");
            // Full dealer hand must be revealed in Complete phase
            assert!(
                !json["dealer_hand"].is_null(),
                "dealer_hand must be revealed in Complete phase"
            );
            assert_eq!(json["available_actions"], serde_json::json!(["NewHand"]));
        }
    }
}
