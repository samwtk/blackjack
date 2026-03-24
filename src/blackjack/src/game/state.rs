//! Top-level game state and phase management.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::card::{Card, new_shoe};
use super::hand::{Hand, HandState};

/// Reshuffle when fewer than this many cards remain.
pub const RESHUFFLE_THRESHOLD: usize = 52;
/// Starting chip count for new sessions.
pub const STARTING_CHIPS: u32 = 1_000;
/// Maximum number of splits allowed per round (results in 4 hands max).
pub const MAX_SPLITS: usize = 3;

/// Phase of the current round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GamePhase {
    /// Waiting for the player to place a bet and deal.
    Waiting,
    /// Player is making decisions on their hands.
    PlayerTurn,
    /// Dealer is playing out their hand.
    DealerTurn,
    /// Round is complete; results available.
    Complete,
}

/// What actions the player can take right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Deal a new hand (only in Waiting phase).
    Deal,
    /// Hit: take another card.
    Hit,
    /// Stand: end action on this hand.
    Stand,
    /// Double down.
    Double,
    /// Split a pair.
    Split,
    /// Take insurance (dealer shows Ace, before player acts).
    Insurance,
    /// Start next hand after round is complete.
    NewHand,
}

/// Per-round outcome for a player hand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HandResult {
    /// Player wins (1:1).
    Win,
    /// Player has a natural blackjack (3:2).
    Blackjack,
    /// Push — bet returned.
    Push,
    /// Player loses.
    Lose,
}

/// The full server-side game state for one session.
pub struct GameState {
    /// Unique session identifier.
    pub session_id: Uuid,
    /// The shoe (deck of cards).
    pub deck: Vec<Card>,
    /// All player hands (grows after splits).
    pub player_hands: Vec<Hand>,
    /// Index of the hand the player is currently acting on.
    pub active_hand_index: usize,
    /// The dealer's hand.
    pub dealer_hand: Hand,
    /// Player's chip balance.
    pub chips: u32,
    /// Insurance bet amount, if placed.
    pub insurance_bet: Option<u32>,
    /// Current phase of the game.
    pub phase: GamePhase,
    /// Last activity timestamp (for session expiry).
    pub last_activity: Instant,
    /// Number of splits performed this round.
    pub split_count: usize,
    /// Whether insurance was offered this round.
    pub insurance_offered: bool,
}

impl GameState {
    /// Create a new game state with a freshly shuffled shoe.
    #[must_use]
    pub fn new() -> Self {
        Self {
            session_id: Uuid::new_v4(),
            deck: new_shoe(),
            player_hands: Vec::new(),
            active_hand_index: 0,
            dealer_hand: Hand::new(0),
            chips: STARTING_CHIPS,
            insurance_bet: None,
            phase: GamePhase::Waiting,
            last_activity: Instant::now(),
            split_count: 0,
            insurance_offered: false,
        }
    }

    /// Draw the top card from the deck, reshuffling if below threshold.
    ///
    /// Uses `pop()` (O(1)) rather than `remove(0)` (O(n)); cards are pre-shuffled so
    /// drawing from the back is equivalent. The deck always has cards here:
    /// either ≥ RESHUFFLE_THRESHOLD if untouched, or 312 if just reshuffled.
    pub fn draw_card(&mut self) -> Card {
        if self.deck.len() < RESHUFFLE_THRESHOLD {
            self.deck = new_shoe();
        }
        // Fallback reshuffle handles the (impossible in practice) case where the deck
        // is empty after the threshold check, avoiding any unwrap() panic.
        if self.deck.is_empty() {
            self.deck = new_shoe();
        }
        self.deck.pop().unwrap_or_else(|| {
            // new_shoe() always returns 312 cards; reaching here is unreachable.
            // Return a card rather than panicking to satisfy the infallible return type.
            new_shoe().pop().unwrap_or(Card {
                suit: super::card::Suit::Spades,
                rank: super::card::Rank::Ace,
            })
        })
    }

    /// Touch last_activity timestamp.
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Compute the list of legal actions for the current state.
    #[must_use]
    pub fn available_actions(&self) -> Vec<Action> {
        match self.phase {
            GamePhase::Waiting => vec![Action::Deal],
            GamePhase::Complete => vec![Action::NewHand],
            GamePhase::DealerTurn => vec![],
            GamePhase::PlayerTurn => {
                let hand = match self.player_hands.get(self.active_hand_index) {
                    Some(h) => h,
                    None => return vec![],
                };
                if hand.state != HandState::Active {
                    return vec![];
                }
                let mut actions = vec![Action::Hit, Action::Stand];
                // Double: only on first 2 cards
                if hand.cards.len() == 2 && self.chips >= hand.bet {
                    actions.push(Action::Double);
                }
                // Split: pair + under split limit + enough chips
                if hand.is_pair()
                    && self.split_count < MAX_SPLITS
                    && self.chips >= hand.bet
                {
                    actions.push(Action::Split);
                }
                // Insurance: offered this round, not yet taken, first hand only (not on split hands),
                // and player hasn't acted yet (still has exactly 2 cards).
                if self.insurance_offered
                    && self.insurance_bet.is_none()
                    && self.active_hand_index == 0
                    && hand.cards.len() == 2
                {
                    actions.push(Action::Insurance);
                }
                actions
            }
        }
    }

    /// Advance active_hand_index to the next Active hand, or return false if none remain.
    pub fn advance_hand(&mut self) -> bool {
        for i in (self.active_hand_index + 1)..self.player_hands.len() {
            if self.player_hands[i].state == HandState::Active {
                self.active_hand_index = i;
                return true;
            }
        }
        false
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_new_game_defaults() {
        let gs = GameState::new();
        assert_eq!(gs.chips, STARTING_CHIPS);
        assert_eq!(gs.phase, GamePhase::Waiting);
        assert!(gs.player_hands.is_empty());
    }

    #[test]
    fn test_draw_card_reduces_deck() {
        let mut gs = GameState::new();
        let initial = gs.deck.len();
        gs.draw_card();
        assert_eq!(gs.deck.len(), initial - 1);
    }

    #[test]
    fn test_available_actions_waiting() {
        let gs = GameState::new();
        assert_eq!(gs.available_actions(), vec![Action::Deal]);
    }

    #[test]
    fn test_available_actions_complete() {
        let mut gs = GameState::new();
        gs.phase = GamePhase::Complete;
        assert_eq!(gs.available_actions(), vec![Action::NewHand]);
    }

    #[test]
    fn test_available_actions_dealer_turn() {
        let mut gs = GameState::new();
        gs.phase = GamePhase::DealerTurn;
        assert!(gs.available_actions().is_empty());
    }
}
