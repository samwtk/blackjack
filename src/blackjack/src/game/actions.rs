//! Game action implementations: deal, hit, stand, double, split, insurance, new_hand.

use super::card::Rank;
use super::dealer::{dealer_has_blackjack, play_dealer};
use super::hand::{Hand, HandState};
use super::state::{Action, GamePhase, GameState};

/// Error returned when an action is not valid in the current state.
#[derive(Debug)]
pub struct ActionError(pub String);

impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Place a bet and deal initial cards. Transitions Waiting → PlayerTurn (or Complete on BJ).
pub fn deal(gs: &mut GameState, bet: u32) -> Result<(), ActionError> {
    if gs.phase != GamePhase::Waiting {
        return Err(ActionError("not in Waiting phase".into()));
    }
    if bet == 0 {
        return Err(ActionError("bet must be greater than 0".into()));
    }
    if bet > gs.chips {
        return Err(ActionError("insufficient chips".into()));
    }

    gs.chips -= bet;
    gs.player_hands.clear();
    gs.active_hand_index = 0;
    gs.insurance_bet = None;
    gs.insurance_offered = false;
    gs.split_count = 0;

    // Classic deal order: player, dealer, player, dealer
    let mut hand = Hand::new(bet);
    gs.dealer_hand = Hand::new(0);
    let c1 = gs.draw_card();
    hand.cards.push(c1);
    let c2 = gs.draw_card(); // face-up card
    gs.dealer_hand.cards.push(c2);
    let c3 = gs.draw_card();
    hand.cards.push(c3);
    let c4 = gs.draw_card(); // hole card
    gs.dealer_hand.cards.push(c4);

    // Offer insurance if dealer shows Ace
    if gs.dealer_hand.cards.first().is_some_and(|c| c.rank == Rank::Ace) {
        gs.insurance_offered = true;
    }

    // Check player natural blackjack
    if hand.is_natural_blackjack() {
        hand.state = HandState::Blackjack;
        gs.player_hands.push(hand);
        // Skip to dealer turn and resolve immediately
        gs.phase = GamePhase::DealerTurn;
        play_dealer(gs);
        resolve_round(gs);
        gs.phase = GamePhase::Complete;
    } else {
        gs.player_hands.push(hand);
        gs.phase = GamePhase::PlayerTurn;
    }

    gs.touch();
    Ok(())
}

/// Hit: deal one card to the active hand.
pub fn hit(gs: &mut GameState) -> Result<(), ActionError> {
    require_action(gs, Action::Hit)?;
    let card = gs.draw_card();
    {
        let hand = active_hand_mut(gs)?;
        hand.cards.push(card);
        if hand.is_bust() {
            hand.state = HandState::Busted;
        }
    }
    let busted = active_hand(gs).is_ok_and(|h| h.state == HandState::Busted);
    if busted {
        maybe_advance_or_end(gs);
    }
    gs.touch();
    Ok(())
}

/// Stand: end action on the active hand.
pub fn stand(gs: &mut GameState) -> Result<(), ActionError> {
    require_action(gs, Action::Stand)?;
    active_hand_mut(gs)?.state = HandState::Standing;
    maybe_advance_or_end(gs);
    gs.touch();
    Ok(())
}

/// Double down: double the bet, deal exactly one card, then stand.
pub fn double(gs: &mut GameState) -> Result<(), ActionError> {
    require_action(gs, Action::Double)?;
    let extra_bet = active_hand(gs)?.bet;
    if extra_bet > gs.chips {
        return Err(ActionError("insufficient chips to double".into()));
    }
    gs.chips -= extra_bet;
    let card = gs.draw_card();
    {
        let hand = active_hand_mut(gs)?;
        hand.bet *= 2;
        hand.is_doubled = true;
        hand.cards.push(card);
        hand.state = if hand.is_bust() {
            HandState::Busted
        } else {
            HandState::Standing
        };
    }
    maybe_advance_or_end(gs);
    gs.touch();
    Ok(())
}

/// Split: split a pair into two hands, deal one card to each.
pub fn split(gs: &mut GameState) -> Result<(), ActionError> {
    require_action(gs, Action::Split)?;
    let extra_bet = active_hand(gs)?.bet;
    if extra_bet > gs.chips {
        return Err(ActionError("insufficient chips to split".into()));
    }
    gs.chips -= extra_bet;
    gs.split_count += 1;

    let idx = gs.active_hand_index;
    // Explicit guards: require exactly 2 cards before indexing (available_actions should
    // already enforce this, but we defend in depth to avoid runtime panics).
    if gs.player_hands.get(idx).map(|h| h.cards.len()).unwrap_or(0) < 2 {
        return Err(ActionError("hand does not have enough cards to split".into()));
    }
    let split_aces = gs.player_hands[idx].cards[0].rank == Rank::Ace;

    // Remove the second card to form the new hand
    let second_card = gs.player_hands[idx].cards.remove(1);
    let bet = gs.player_hands[idx].bet;

    // Deal one card to the first split hand
    let card_for_first = gs.draw_card();
    gs.player_hands[idx].cards.push(card_for_first);

    // Deal one card to the second split hand
    let card_for_second = gs.draw_card();
    let mut new_hand = Hand::new(bet);
    new_hand.cards.push(second_card);
    new_hand.cards.push(card_for_second);

    // Split aces: each hand gets exactly one card, no further action allowed
    if split_aces {
        gs.player_hands[idx].state = HandState::Standing;
        new_hand.state = HandState::Standing;
    }

    gs.player_hands.insert(idx + 1, new_hand);

    // If split aces, both hands are already standing — advance to dealer turn
    if split_aces {
        maybe_advance_or_end(gs);
    }

    gs.touch();
    Ok(())
}

/// Take insurance (pays 2:1 if dealer has blackjack).
pub fn insurance(gs: &mut GameState) -> Result<(), ActionError> {
    require_action(gs, Action::Insurance)?;
    let hand_bet = active_hand(gs)?.bet;
    let max_insurance = hand_bet / 2;
    if max_insurance == 0 {
        return Err(ActionError("bet too small for insurance".into()));
    }
    if max_insurance > gs.chips {
        return Err(ActionError("insufficient chips for insurance".into()));
    }
    gs.chips -= max_insurance;
    gs.insurance_bet = Some(max_insurance);
    gs.touch();
    Ok(())
}

/// Prepare for the next hand: resets round state, keeps chips. Requires Complete phase.
pub fn new_hand(gs: &mut GameState) -> Result<(), ActionError> {
    if gs.phase != GamePhase::Complete {
        return Err(ActionError("round is not complete yet".into()));
    }
    gs.player_hands.clear();
    gs.dealer_hand = Hand::new(0);
    gs.active_hand_index = 0;
    gs.insurance_bet = None;
    gs.insurance_offered = false;
    gs.split_count = 0;
    gs.phase = GamePhase::Waiting;
    gs.touch();
    Ok(())
}

/// Settle all hands and update chip balance. Called automatically at end of player turn.
pub fn resolve_round(gs: &mut GameState) {
    let dealer_score = gs.dealer_hand.score();
    let dealer_bj = dealer_has_blackjack(&gs.dealer_hand);

    // Resolve insurance side bet first
    if let Some(ins) = gs.insurance_bet.take()
        && dealer_bj
    {
        gs.chips += ins * 3; // 2:1 payout + original insurance bet returned
        // else insurance is lost (already deducted at placement)
    }

    for hand in &gs.player_hands {
        match hand.state {
            HandState::Busted => {
                // Chips already deducted at deal time; nothing to return
            }
            HandState::Blackjack => {
                if dealer_bj {
                    gs.chips += hand.bet; // push: return bet
                } else {
                    gs.chips += hand.bet + (hand.bet * 3 / 2); // 3:2 payout
                }
            }
            HandState::Standing => {
                let player_score = hand.score();
                if dealer_bj {
                    // Dealer blackjack beats player standing hand
                }
                else if gs.dealer_hand.is_bust() || player_score > dealer_score {
                    gs.chips += hand.bet * 2; // win: bet + winnings
                } else if player_score == dealer_score {
                    gs.chips += hand.bet; // push: return bet
                }
                // else player loses (chips already deducted)
            }
            HandState::Active => {
                // Shouldn't happen; treat as loss
            }
        }
    }
}

// ── Private helpers ──────────────────────────────────────────────────────────

fn require_action(gs: &GameState, action: Action) -> Result<(), ActionError> {
    if gs.available_actions().contains(&action) {
        Ok(())
    } else {
        Err(ActionError(format!("action {action:?} is not available right now")))
    }
}

fn active_hand(gs: &GameState) -> Result<&Hand, ActionError> {
    gs.player_hands
        .get(gs.active_hand_index)
        .ok_or_else(|| ActionError("no active hand".into()))
}

fn active_hand_mut(gs: &mut GameState) -> Result<&mut Hand, ActionError> {
    let idx = gs.active_hand_index;
    gs.player_hands
        .get_mut(idx)
        .ok_or_else(|| ActionError("no active hand".into()))
}

/// After a hand's state changes, advance to the next hand or run the dealer turn.
fn maybe_advance_or_end(gs: &mut GameState) {
    if !gs.advance_hand() {
        // All player hands are resolved; dealer plays
        gs.phase = GamePhase::DealerTurn;
        play_dealer(gs);
        resolve_round(gs);
        gs.phase = GamePhase::Complete;
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::game::state::STARTING_CHIPS;

    #[test]
    fn test_deal_valid() {
        let mut gs = GameState::new();
        deal(&mut gs, 100).expect("deal failed");
        assert_eq!(gs.player_hands.len(), 1);
        assert_eq!(gs.player_hands[0].cards.len(), 2);
        // Dealer always starts with 2 cards; if player has natural BJ, play_dealer
        // may draw additional cards, so we assert >= 2.
        assert!(gs.dealer_hand.cards.len() >= 2);
        // Bet deduction happened unconditionally; if BJ resolved, payout was added back.
        // In PlayerTurn the bet is still outstanding; in Complete it has been settled.
        if gs.phase == GamePhase::PlayerTurn {
            assert_eq!(gs.chips, STARTING_CHIPS - 100);
        }
    }

    #[test]
    fn test_deal_insufficient_chips() {
        let mut gs = GameState::new();
        let result = deal(&mut gs, STARTING_CHIPS + 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_deal_zero_bet() {
        let mut gs = GameState::new();
        let result = deal(&mut gs, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_new_hand_resets_state() {
        let mut gs = GameState::new();
        gs.phase = GamePhase::Complete;
        new_hand(&mut gs).expect("new_hand failed");
        assert_eq!(gs.phase, GamePhase::Waiting);
        assert!(gs.player_hands.is_empty());
    }

    #[test]
    fn test_new_hand_requires_complete_phase() {
        let mut gs = GameState::new();
        assert!(new_hand(&mut gs).is_err());
    }

    #[test]
    fn test_hit_adds_card() {
        let mut gs = GameState::new();
        deal(&mut gs, 50).expect("deal");
        // Only test if in PlayerTurn (may be Complete on natural BJ)
        if gs.phase == GamePhase::PlayerTurn {
            let cards_before = gs.player_hands[0].cards.len();
            hit(&mut gs).expect("hit");
            // Hand may have ended (bust) but a card was drawn
            assert!(gs.player_hands[0].cards.len() > cards_before);
        }
    }

    #[test]
    fn test_stand_transitions_phase() {
        let mut gs = GameState::new();
        deal(&mut gs, 50).expect("deal");
        if gs.phase == GamePhase::PlayerTurn {
            stand(&mut gs).expect("stand");
            // After standing with one hand, dealer plays and round completes
            assert_eq!(gs.phase, GamePhase::Complete);
        }
    }

    #[test]
    fn test_double_doubles_bet() {
        let mut gs = GameState::new();
        deal(&mut gs, 50).expect("deal");
        if gs.phase == GamePhase::PlayerTurn {
            let original_bet = gs.player_hands[0].bet;
            double(&mut gs).expect("double");
            // Bet should be doubled (resolve_round may have changed chips, but bet is captured before)
            assert_eq!(gs.player_hands[0].bet, original_bet * 2);
            // Hand should be marked as doubled
            assert!(gs.player_hands[0].is_doubled);
            // Round must be complete after double (one card dealt then stand, dealer plays)
            assert_eq!(gs.phase, GamePhase::Complete);
        }
    }

    #[test]
    fn test_invalid_action_returns_error() {
        let mut gs = GameState::new();
        // Can't hit in Waiting phase
        assert!(hit(&mut gs).is_err());
        // Can't stand in Waiting phase
        assert!(stand(&mut gs).is_err());
    }

    #[test]
    fn test_resolve_round_win_pays_correctly() {
        let mut gs = GameState::new();
        // Set up a won hand manually
        let mut hand = Hand::new(100);
        hand.state = HandState::Standing;
        // Give player a 20 (King + Ten)
        hand.cards.push(crate::game::card::Card { suit: crate::game::card::Suit::Spades, rank: crate::game::card::Rank::King });
        hand.cards.push(crate::game::card::Card { suit: crate::game::card::Suit::Spades, rank: crate::game::card::Rank::Ten });
        gs.player_hands = vec![hand];
        gs.chips = 0; // chips already deducted at bet time

        // Dealer has 15 (Ten + Five)
        gs.dealer_hand = Hand::new(0);
        gs.dealer_hand.cards.push(crate::game::card::Card { suit: crate::game::card::Suit::Hearts, rank: crate::game::card::Rank::Ten });
        gs.dealer_hand.cards.push(crate::game::card::Card { suit: crate::game::card::Suit::Hearts, rank: crate::game::card::Rank::Five });
        gs.dealer_hand.state = HandState::Standing; // dealer stopped (not relevant to resolve_round)

        // Manually set dealer score to 15 by calling play_dealer would draw more cards;
        // instead just test with a bust dealer
        gs.dealer_hand.cards.push(crate::game::card::Card { suit: crate::game::card::Suit::Hearts, rank: crate::game::card::Rank::King }); // bust: 25
        gs.dealer_hand.state = HandState::Busted;

        resolve_round(&mut gs);
        // Player wins with bust dealer: gets bet * 2 = 200
        assert_eq!(gs.chips, 200);
    }

    #[test]
    fn test_resolve_round_push_tie() {
        use crate::game::card::{Card, Rank, Suit};
        let mut gs = GameState::new();
        // Player has 20 (King + Ten)
        let mut hand = Hand::new(100);
        hand.state = HandState::Standing;
        hand.cards.push(Card { suit: Suit::Spades, rank: Rank::King });
        hand.cards.push(Card { suit: Suit::Spades, rank: Rank::Ten });
        gs.player_hands = vec![hand];
        gs.chips = 0;
        // Dealer also has 20 (King + Ten)
        gs.dealer_hand = Hand::new(0);
        gs.dealer_hand.cards.push(Card { suit: Suit::Hearts, rank: Rank::King });
        gs.dealer_hand.cards.push(Card { suit: Suit::Hearts, rank: Rank::Ten });
        resolve_round(&mut gs);
        // Push: bet returned
        assert_eq!(gs.chips, 100);
    }

    #[test]
    fn test_resolve_round_player_bj_vs_dealer_bj_is_push() {
        use crate::game::card::{Card, Rank, Suit};
        let mut gs = GameState::new();
        let mut hand = Hand::new(100);
        hand.state = HandState::Blackjack;
        hand.cards.push(Card { suit: Suit::Spades, rank: Rank::Ace });
        hand.cards.push(Card { suit: Suit::Spades, rank: Rank::King });
        gs.player_hands = vec![hand];
        gs.chips = 0;
        // Dealer also has blackjack
        gs.dealer_hand = Hand::new(0);
        gs.dealer_hand.cards.push(Card { suit: Suit::Hearts, rank: Rank::Ace });
        gs.dealer_hand.cards.push(Card { suit: Suit::Hearts, rank: Rank::King });
        resolve_round(&mut gs);
        // Push: bet returned
        assert_eq!(gs.chips, 100);
    }

    #[test]
    fn test_resolve_round_insurance_win() {
        use crate::game::card::{Card, Rank, Suit};
        let mut gs = GameState::new();
        let mut hand = Hand::new(100);
        hand.state = HandState::Standing;
        hand.cards.push(Card { suit: Suit::Spades, rank: Rank::Ten });
        hand.cards.push(Card { suit: Suit::Spades, rank: Rank::Eight });
        gs.player_hands = vec![hand];
        gs.chips = 0;
        gs.insurance_bet = Some(50); // player took insurance
        // Dealer has blackjack
        gs.dealer_hand = Hand::new(0);
        gs.dealer_hand.cards.push(Card { suit: Suit::Hearts, rank: Rank::Ace });
        gs.dealer_hand.cards.push(Card { suit: Suit::Hearts, rank: Rank::King });
        resolve_round(&mut gs);
        // Insurance pays 2:1: 50 * 3 = 150 (original 50 + 100 profit)
        // Player's standing hand loses to dealer BJ: 0 returned
        assert_eq!(gs.chips, 150);
    }
}
