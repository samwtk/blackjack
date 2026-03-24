//! Card primitives: suits, ranks, deck creation, and shuffling.

use rand::seq::SliceRandom;
use rand_core::OsRng;
use serde::{Deserialize, Serialize};

/// Card suit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Suit {
    /// Clubs
    Clubs,
    /// Diamonds
    Diamonds,
    /// Hearts
    Hearts,
    /// Spades
    Spades,
}

/// Card rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rank {
    /// Two
    Two,
    /// Three
    Three,
    /// Four
    Four,
    /// Five
    Five,
    /// Six
    Six,
    /// Seven
    Seven,
    /// Eight
    Eight,
    /// Nine
    Nine,
    /// Ten
    Ten,
    /// Jack
    Jack,
    /// Queen
    Queen,
    /// King
    King,
    /// Ace
    Ace,
}

impl Rank {
    /// Base point value (Ace = 11; handled as soft/hard in hand scoring).
    #[must_use]
    pub fn value(self) -> u8 {
        match self {
            Rank::Two => 2,
            Rank::Three => 3,
            Rank::Four => 4,
            Rank::Five => 5,
            Rank::Six => 6,
            Rank::Seven => 7,
            Rank::Eight => 8,
            Rank::Nine => 9,
            Rank::Ten | Rank::Jack | Rank::Queen | Rank::King => 10,
            Rank::Ace => 11,
        }
    }
}

/// A playing card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Card {
    /// Suit of the card.
    pub suit: Suit,
    /// Rank of the card.
    pub rank: Rank,
}

/// Build a fresh 6-deck shoe (312 cards) and shuffle it with OsRng.
#[must_use]
pub fn new_shoe() -> Vec<Card> {
    let suits = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];
    let ranks = [
        Rank::Two, Rank::Three, Rank::Four, Rank::Five, Rank::Six,
        Rank::Seven, Rank::Eight, Rank::Nine, Rank::Ten,
        Rank::Jack, Rank::Queen, Rank::King, Rank::Ace,
    ];
    let mut deck: Vec<Card> = (0..6)
        .flat_map(|_| suits.iter().flat_map(|&s| ranks.iter().map(move |&r| Card { suit: s, rank: r })))
        .collect();
    deck.shuffle(&mut OsRng);
    deck
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_shoe_has_312_cards() {
        let shoe = new_shoe();
        assert_eq!(shoe.len(), 312);
    }

    #[test]
    fn test_shoe_has_correct_distribution() {
        let shoe = new_shoe();
        // 6 decks * 4 suits * 1 rank = 24 of each rank
        let aces = shoe.iter().filter(|c| c.rank == Rank::Ace).count();
        assert_eq!(aces, 24);
        let kings = shoe.iter().filter(|c| c.rank == Rank::King).count();
        assert_eq!(kings, 24);
    }

    #[test]
    fn test_rank_values() {
        assert_eq!(Rank::Two.value(), 2);
        assert_eq!(Rank::Ten.value(), 10);
        assert_eq!(Rank::Jack.value(), 10);
        assert_eq!(Rank::Queen.value(), 10);
        assert_eq!(Rank::King.value(), 10);
        assert_eq!(Rank::Ace.value(), 11);
    }

    #[test]
    fn test_shoe_is_shuffled() {
        // A shuffled shoe should not be in canonical (sorted) order.
        // Build what the canonical order would look like (unshuffled).
        let suits = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];
        let ranks = [
            Rank::Two, Rank::Three, Rank::Four, Rank::Five, Rank::Six,
            Rank::Seven, Rank::Eight, Rank::Nine, Rank::Ten,
            Rank::Jack, Rank::Queen, Rank::King, Rank::Ace,
        ];
        let canonical: Vec<Card> = (0..6)
            .flat_map(|_| {
                suits.iter().flat_map(|&s| ranks.iter().map(move |&r| Card { suit: s, rank: r }))
            })
            .collect();
        let shoe = new_shoe();
        assert_ne!(shoe, canonical, "Shoe should not be in canonical sorted order after shuffle");
    }
}
