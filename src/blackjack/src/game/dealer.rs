//! Dealer AI: hits on hard/soft 16 or less, stands on soft 17+.

use super::hand::Hand;
use super::state::GameState;

/// Run the dealer's turn to completion.
/// Standard rule: dealer hits on 16 or less (soft or hard), stands on 17+ (soft or hard).
pub fn play_dealer(gs: &mut GameState) {
    loop {
        let score = gs.dealer_hand.score();
        if score >= 17 {
            break;
        }
        let card = gs.draw_card();
        gs.dealer_hand.cards.push(card);
    }
}

/// True if the dealer's hand is a natural blackjack.
#[must_use]
pub fn dealer_has_blackjack(dealer: &Hand) -> bool {
    dealer.is_natural_blackjack()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::game::card::{Card, Rank, Suit};
    use crate::game::hand::Hand;

    fn card(rank: Rank) -> Card {
        Card { suit: Suit::Hearts, rank }
    }

    #[test]
    fn test_dealer_stands_on_hard_17() {
        let mut gs = GameState::new();
        gs.dealer_hand = Hand::new(0);
        gs.dealer_hand.cards.push(card(Rank::Ten));
        gs.dealer_hand.cards.push(card(Rank::Seven));
        play_dealer(&mut gs);
        assert_eq!(gs.dealer_hand.score(), 17);
        assert_eq!(gs.dealer_hand.cards.len(), 2);
    }

    #[test]
    fn test_dealer_hits_on_16() {
        let mut gs = GameState::new();
        gs.dealer_hand = Hand::new(0);
        gs.dealer_hand.cards.push(card(Rank::Ten));
        gs.dealer_hand.cards.push(card(Rank::Six));
        play_dealer(&mut gs);
        // Dealer must have drawn at least one more card (started at 16, must hit)
        assert!(gs.dealer_hand.cards.len() >= 3);
        // After play_dealer exits, score is always >= 17 (loop invariant)
        assert!(gs.dealer_hand.score() >= 17);
    }

    #[test]
    fn test_dealer_stands_on_soft_17() {
        let mut gs = GameState::new();
        gs.dealer_hand = Hand::new(0);
        gs.dealer_hand.cards.push(card(Rank::Ace));
        gs.dealer_hand.cards.push(card(Rank::Six));
        // Soft 17: Ace + Six. Standard rule: stand on soft 17.
        play_dealer(&mut gs);
        assert_eq!(gs.dealer_hand.score(), 17);
        assert_eq!(gs.dealer_hand.cards.len(), 2);
    }

    #[test]
    fn test_dealer_blackjack_detection() {
        let mut h = Hand::new(0);
        h.cards.push(card(Rank::Ace));
        h.cards.push(card(Rank::King));
        assert!(dealer_has_blackjack(&h));
    }

    #[test]
    fn test_dealer_no_blackjack_on_non_natural() {
        let mut h = Hand::new(0);
        h.cards.push(card(Rank::Ten));
        h.cards.push(card(Rank::Five));
        h.cards.push(card(Rank::Six));
        assert!(!dealer_has_blackjack(&h));
    }
}
