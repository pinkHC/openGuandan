use super::errors::RuleError;
use super::types::{Card, CardRank, ORDINARY_RANKS, OrdinaryRank, Suit};

pub const ORDINARY_SUITS: [Suit; 4] = [Suit::Heart, Suit::Diamond, Suit::Club, Suit::Spade];

pub fn create_deck() -> Vec<Card> {
    let mut cards = Vec::with_capacity(108);

    for deck_index in [0_u8, 1_u8] {
        for suit in ORDINARY_SUITS {
            for rank in ORDINARY_RANKS {
                let rank = CardRank::from(rank);
                cards.push(Card {
                    id: format!("{deck_index}:{}:{}", suit.as_str(), rank.as_str()),
                    deck_index,
                    suit,
                    rank,
                });
            }
        }

        cards.push(Card {
            id: format!("{deck_index}:joker:small-joker"),
            deck_index,
            suit: Suit::Joker,
            rank: CardRank::SmallJoker,
        });
        cards.push(Card {
            id: format!("{deck_index}:joker:big-joker"),
            deck_index,
            suit: Suit::Joker,
            rank: CardRank::BigJoker,
        });
    }

    cards
}

pub fn shuffle_cards_with<F>(cards: &[Card], random_index: &mut F) -> Result<Vec<Card>, RuleError>
where
    F: FnMut(usize) -> usize + ?Sized,
{
    let mut shuffled = cards.to_vec();
    for index in (1..shuffled.len()).rev() {
        let swap_index = random_index(index + 1);
        if swap_index > index {
            return Err(RuleError::internal("Invalid shuffle index"));
        }
        shuffled.swap(index, swap_index);
    }
    Ok(shuffled)
}

pub fn deal_cards(cards: &[Card]) -> Result<[Vec<Card>; 4], RuleError> {
    if cards.len() != 108 {
        return Err(RuleError::internal(format!(
            "Expected 108 cards, received {}",
            cards.len()
        )));
    }

    let mut hands: [Vec<Card>; 4] = std::array::from_fn(|_| Vec::with_capacity(27));
    for (index, card) in cards.iter().enumerate() {
        hands[index % 4].push(card.clone());
    }
    Ok(hands)
}

pub const fn ordinary_rank_value(rank: OrdinaryRank) -> u8 {
    rank.index() as u8 + 2
}

pub const fn card_rank_strength(rank: CardRank, level_rank: OrdinaryRank) -> u8 {
    match rank {
        CardRank::BigJoker => 17,
        CardRank::SmallJoker => 16,
        ordinary if matches!(ordinary.as_ordinary(), Some(value) if value.index() == level_rank.index()) => {
            15
        }
        ordinary => match ordinary.as_ordinary() {
            Some(value) => ordinary_rank_value(value),
            None => unreachable!(),
        },
    }
}

pub fn is_wildcard(card: &Card, level_rank: OrdinaryRank) -> bool {
    card.suit == Suit::Heart
        && matches!(card.rank.as_ordinary(), Some(rank) if rank.index() == level_rank.index())
}

pub fn sort_cards(cards: &[Card], level_rank: OrdinaryRank) -> Vec<Card> {
    let suit_order = |suit: Suit| match suit {
        Suit::Diamond => 0_u8,
        Suit::Club => 1,
        Suit::Heart => 2,
        Suit::Spade => 3,
        Suit::Joker => 4,
    };

    let mut sorted = cards.to_vec();
    sorted.sort_by_key(|card| {
        (
            card_rank_strength(card.rank, level_rank),
            suit_order(card.suit),
            card.deck_index,
        )
    });
    sorted
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn creates_two_ordered_standard_decks_with_stable_ids() {
        let deck = create_deck();
        assert_eq!(deck.len(), 108);
        assert_eq!(deck[0].id, "0:heart:2");
        assert_eq!(deck[51].id, "0:spade:A");
        assert_eq!(deck[52].id, "0:joker:small-joker");
        assert_eq!(deck[53].id, "0:joker:big-joker");
        assert_eq!(deck[54].id, "1:heart:2");
        assert_eq!(
            deck.iter()
                .map(|card| &card.id)
                .collect::<HashSet<_>>()
                .len(),
            108
        );
    }

    #[test]
    fn deterministic_fisher_yates_and_deal_match_typescript() {
        let deck = create_deck();
        let mut calls = Vec::new();
        let shuffled = shuffle_cards_with(&deck, &mut |upper| {
            calls.push(upper);
            0
        })
        .unwrap();
        assert_eq!(calls.len(), 107);
        assert_eq!(calls[0], 108);
        assert_eq!(calls[106], 2);
        assert_eq!(shuffled[0], deck[1]);
        assert_eq!(shuffled[107], deck[0]);

        let hands = deal_cards(&shuffled).unwrap();
        assert!(hands.iter().all(|hand| hand.len() == 27));
        assert_eq!(hands[0][0], shuffled[0]);
        assert_eq!(hands[3][0], shuffled[3]);
    }

    #[test]
    fn strength_and_sorting_put_level_below_jokers() {
        let level = OrdinaryRank::Seven;
        assert_eq!(card_rank_strength(CardRank::Ace, level), 14);
        assert_eq!(card_rank_strength(CardRank::Seven, level), 15);
        assert_eq!(card_rank_strength(CardRank::SmallJoker, level), 16);
        assert_eq!(card_rank_strength(CardRank::BigJoker, level), 17);

        let cards = vec![
            Card {
                id: "spade".into(),
                deck_index: 1,
                suit: Suit::Spade,
                rank: CardRank::Three,
            },
            Card {
                id: "diamond".into(),
                deck_index: 0,
                suit: Suit::Diamond,
                rank: CardRank::Three,
            },
        ];
        let sorted = sort_cards(&cards, level);
        assert_eq!(sorted[0].id, "diamond");
    }

    #[test]
    fn rejects_bad_shuffle_indices_and_deal_sizes() {
        let error = shuffle_cards_with(&create_deck(), &mut |upper| upper).unwrap_err();
        assert_eq!(error.code, "INTERNAL_ERROR");
        assert!(deal_cards(&[]).is_err());
    }
}
