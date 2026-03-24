//! Hand representation and scoring (soft/hard aces, bust detection).

use serde::{Deserialize, Serialize};

use super::card::{Card, Rank};

/// The state of a single hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandState {
    /// Hand is active (player can act).
    Active,
    /// Player has chosen to stand.
    Standing,
    /// Hand has gone over 21.
    Busted,
    /// Natural blackjack (Ace + 10-value on first two cards).
    Blackjack,
}

/// A player or dealer hand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hand {
    /// Cards in the hand.
    pub cards: Vec<Card>,
    /// Wager on this hand (in chips).
    pub bet: u32,
    /// Current state of the hand.
    pub state: HandState,
    /// Whether the player doubled down on this hand.
    pub is_doubled: bool,
}

impl Hand {
    /// Create a new empty hand with a given bet.
    #[must_use]
    pub fn new(bet: u32) -> Self {
        Self {
            cards: Vec::new(),
            bet,
            state: HandState::Active,
            is_doubled: false,
        }
    }

    /// Compute the best score for this hand (highest value ≤ 21, or lowest bust value).
    #[must_use]
    pub fn score(&self) -> u8 {
        let mut total: u16 = 0;
        let mut aces = 0u8;

        for card in &self.cards {
            let v = u16::from(card.rank.value());
            if card.rank == Rank::Ace {
                aces += 1;
            }
            total += v;
        }
        // Demote aces from 11 → 1 as needed to avoid bust
        while total > 21 && aces > 0 {
            total -= 10;
            aces -= 1;
        }
        // Safe: after ace demotion, total fits in u8 (max possible bust = 30 for three 10-value cards)
        u8::try_from(total).unwrap_or(u8::MAX)
    }

    /// True if the hand is a bust (score > 21).
    #[must_use]
    pub fn is_bust(&self) -> bool {
        self.score() > 21
    }

    /// True if the hand is a natural blackjack (exactly 2 cards, score == 21).
    #[must_use]
    pub fn is_natural_blackjack(&self) -> bool {
        self.cards.len() == 2 && self.score() == 21
    }

    /// True if the hand is a pair (same rank on both cards) — required for split.
    #[must_use]
    pub fn is_pair(&self) -> bool {
        self.cards.len() == 2 && self.cards[0].rank == self.cards[1].rank
    }

    /// True if the score is "soft" (at least one Ace is still counted as 11).
    #[must_use]
    pub fn is_soft(&self) -> bool {
        let mut total: u16 = 0;
        let mut aces: u8 = 0;
        for card in &self.cards {
            if card.rank == Rank::Ace {
                aces += 1;
            }
            total += u16::from(card.rank.value());
        }
        // Demote aces while busting — same logic as score()
        while total > 21 && aces > 0 {
            total -= 10;
            aces -= 1;
        }
        // Soft if at least one ace is still counted as 11
        aces > 0 && total <= 21
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::game::card::Suit;

    fn card(rank: Rank) -> Card {
        Card { suit: Suit::Spades, rank }
    }

    #[test]
    fn test_score_simple() {
        let mut h = Hand::new(10);
        h.cards.push(card(Rank::Five));
        h.cards.push(card(Rank::Seven));
        assert_eq!(h.score(), 12);
    }

    #[test]
    fn test_score_soft_ace() {
        let mut h = Hand::new(10);
        h.cards.push(card(Rank::Ace));
        h.cards.push(card(Rank::Six));
        assert_eq!(h.score(), 17); // soft 17
        assert!(h.is_soft());
    }

    #[test]
    fn test_score_ace_demotion() {
        let mut h = Hand::new(10);
        h.cards.push(card(Rank::Ace));
        h.cards.push(card(Rank::Nine));
        h.cards.push(card(Rank::Five));
        assert_eq!(h.score(), 15); // Ace demoted to 1
    }

    #[test]
    fn test_natural_blackjack() {
        let mut h = Hand::new(10);
        h.cards.push(card(Rank::Ace));
        h.cards.push(card(Rank::King));
        assert!(h.is_natural_blackjack());
        assert_eq!(h.score(), 21);
    }

    #[test]
    fn test_bust() {
        let mut h = Hand::new(10);
        h.cards.push(card(Rank::King));
        h.cards.push(card(Rank::Queen));
        h.cards.push(card(Rank::Five));
        assert!(h.is_bust());
        assert_eq!(h.score(), 25);
    }

    #[test]
    fn test_is_pair() {
        let mut h = Hand::new(10);
        h.cards.push(card(Rank::Eight));
        h.cards.push(card(Rank::Eight));
        assert!(h.is_pair());
    }

    #[test]
    fn test_not_pair_different_ranks() {
        let mut h = Hand::new(10);
        h.cards.push(card(Rank::Eight));
        h.cards.push(card(Rank::Nine));
        assert!(!h.is_pair());
    }

    #[test]
    fn test_two_aces_score() {
        let mut h = Hand::new(10);
        h.cards.push(card(Rank::Ace));
        h.cards.push(card(Rank::Ace));
        assert_eq!(h.score(), 12); // one Ace = 11, other = 1
    }

    #[test]
    fn test_two_aces_and_eight_is_soft_20() {
        let mut h = Hand::new(10);
        h.cards.push(card(Rank::Ace));
        h.cards.push(card(Rank::Ace));
        h.cards.push(card(Rank::Eight));
        assert_eq!(h.score(), 20);
        assert!(h.is_soft(), "A+A+8 should be soft 20");
    }

    #[test]
    fn test_three_aces_score() {
        let mut h = Hand::new(10);
        h.cards.push(card(Rank::Ace));
        h.cards.push(card(Rank::Ace));
        h.cards.push(card(Rank::Ace));
        assert_eq!(h.score(), 13); // 11 + 1 + 1
    }

    #[test]
    fn test_is_not_soft_when_all_aces_demoted() {
        let mut h = Hand::new(10);
        h.cards.push(card(Rank::Ace));
        h.cards.push(card(Rank::Nine));
        h.cards.push(card(Rank::Five));
        assert_eq!(h.score(), 15); // hard 15
        assert!(!h.is_soft(), "A+9+5 should be hard 15");
    }
}
