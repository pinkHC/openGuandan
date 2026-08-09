use std::sync::atomic::{AtomicUsize, Ordering};

use super::{
    round::{RoundPhase, RoundState},
    types::{Card, CardRank, OrdinaryRank, Seat, Suit},
};

static NEXT_CARD_ID: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn card(rank: CardRank, suit: impl Into<Suit>) -> Card {
    let suit = suit.into();
    Card {
        id: format!("test-card-{}", NEXT_CARD_ID.fetch_add(1, Ordering::Relaxed)),
        deck_index: 0,
        suit: if matches!(rank, CardRank::SmallJoker | CardRank::BigJoker) {
            Suit::Joker
        } else {
            suit
        },
        rank,
    }
}

pub(crate) fn spade(rank: CardRank) -> Card {
    card(rank, Suit::Spade)
}

pub(crate) fn spades(ranks: &[CardRank]) -> Vec<Card> {
    ranks.iter().copied().map(spade).collect()
}

pub(crate) fn simple_round(level_rank: OrdinaryRank) -> RoundState {
    RoundState {
        number: 1,
        level_rank,
        level_owner_team: None,
        phase: RoundPhase::Playing,
        hands: [
            vec![spade(CardRank::Three)],
            vec![spade(CardRank::Four), spade(CardRank::Eight)],
            vec![spade(CardRank::Five)],
            vec![spade(CardRank::Six), spade(CardRank::Nine)],
        ],
        active_seats: Seat::all().into_iter().collect(),
        turn_seat: Seat::ZERO,
        current_play: None,
        consecutive_passes: 0,
        finish_order: Vec::new(),
        tribute: None,
    }
}
