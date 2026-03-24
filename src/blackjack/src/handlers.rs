//! HTTP request handlers — thin layer that delegates to game logic.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::game::actions::{deal, double, hit, insurance, new_hand, split, stand, ActionError};
use crate::game::card::Card;
use crate::game::hand::Hand;
use crate::game::state::{Action, GamePhase, GameState};
use crate::session::SessionStore;

// ── Response types ────────────────────────────────────────────────────────────

/// Public game state returned to the client on every response.
#[derive(Serialize)]
pub struct GameStateResponse {
    /// Session identifier.
    pub session_id: Uuid,
    /// Current chip balance.
    pub chips: u32,
    /// Current game phase.
    pub phase: GamePhase,
    /// All player hands.
    pub player_hands: Vec<Hand>,
    /// Dealer's face-up card (first card). Hidden hole card during PlayerTurn.
    pub dealer_visible_card: Option<Card>,
    /// Full dealer hand — only revealed in DealerTurn and Complete phases.
    pub dealer_hand: Option<Hand>,
    /// Actions the player can take right now.
    pub available_actions: Vec<Action>,
}

/// Build a client-safe response from the server-side game state.
pub fn to_response(gs: &GameState) -> GameStateResponse {
    let reveal_dealer = matches!(gs.phase, GamePhase::DealerTurn | GamePhase::Complete);
    GameStateResponse {
        session_id: gs.session_id,
        chips: gs.chips,
        phase: gs.phase.clone(),
        player_hands: gs.player_hands.clone(),
        dealer_visible_card: gs.dealer_hand.cards.first().copied(),
        dealer_hand: if reveal_dealer {
            Some(gs.dealer_hand.clone())
        } else {
            None
        },
        available_actions: gs.available_actions(),
    }
}

// ── Error helpers ─────────────────────────────────────────────────────────────

fn error(status: StatusCode, msg: &str) -> Response {
    (status, Json(serde_json::json!({"error": msg}))).into_response()
}

fn not_found() -> Response {
    error(StatusCode::NOT_FOUND, "session not found")
}

fn action_error(e: ActionError) -> Response {
    error(StatusCode::BAD_REQUEST, &e.to_string())
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /game/new — create a new session.
pub async fn create_game(State(store): State<SessionStore>) -> Response {
    let gs = GameState::new();
    let id = store.create(gs);
    match store.with(id, to_response) {
        Some(resp) => (StatusCode::CREATED, Json(resp)).into_response(),
        None => error(StatusCode::INTERNAL_SERVER_ERROR, "failed to create session"),
    }
}

/// GET /game/:id — fetch current game state.
pub async fn get_game(
    State(store): State<SessionStore>,
    Path(id): Path<Uuid>,
) -> Response {
    match store.with(id, to_response) {
        Some(resp) => Json(resp).into_response(),
        None => not_found(),
    }
}

/// DELETE /game/:id — end session.
pub async fn delete_game(
    State(store): State<SessionStore>,
    Path(id): Path<Uuid>,
) -> Response {
    if store.remove(id) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        not_found()
    }
}

/// POST /game/:id/deal — place bet and deal.
#[derive(Deserialize)]
pub struct DealRequest {
    /// Chips to bet.
    pub bet: u32,
}

/// Handler for deal requests.
pub async fn deal_hand(
    State(store): State<SessionStore>,
    Path(id): Path<Uuid>,
    Json(body): Json<DealRequest>,
) -> Response {
    let bet = body.bet;
    apply_action(store, id, |gs| deal(gs, bet))
}

/// POST /game/:id/hit
pub async fn hit_hand(
    State(store): State<SessionStore>,
    Path(id): Path<Uuid>,
) -> Response {
    apply_action(store, id, hit)
}

/// POST /game/:id/stand
pub async fn stand_hand(
    State(store): State<SessionStore>,
    Path(id): Path<Uuid>,
) -> Response {
    apply_action(store, id, stand)
}

/// POST /game/:id/double
pub async fn double_hand(
    State(store): State<SessionStore>,
    Path(id): Path<Uuid>,
) -> Response {
    apply_action(store, id, double)
}

/// POST /game/:id/split
pub async fn split_hand(
    State(store): State<SessionStore>,
    Path(id): Path<Uuid>,
) -> Response {
    apply_action(store, id, split)
}

/// POST /game/:id/insurance
pub async fn insurance_hand(
    State(store): State<SessionStore>,
    Path(id): Path<Uuid>,
) -> Response {
    apply_action(store, id, insurance)
}

/// POST /game/:id/new-hand
pub async fn next_hand(
    State(store): State<SessionStore>,
    Path(id): Path<Uuid>,
) -> Response {
    apply_action(store, id, new_hand)
}

// ── Helper ────────────────────────────────────────────────────────────────────

fn apply_action<F>(store: SessionStore, id: Uuid, f: F) -> Response
where
    F: FnOnce(&mut GameState) -> Result<(), ActionError>,
{
    let result = store.with_mut(id, |gs| f(gs).map(|()| to_response(gs)));
    match result {
        None => not_found(),
        Some(Err(e)) => action_error(e),
        Some(Ok(resp)) => Json(resp).into_response(),
    }
}
