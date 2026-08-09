use std::collections::HashSet;

use indexmap::IndexMap;
use serde_json::json;

use super::cards::{card_rank_strength, is_wildcard, ordinary_rank_value};
use super::errors::RuleError;
use super::types::{
    Card, CardRank, Combination, CombinationDeclaration, CombinationKind, ORDINARY_RANKS,
    ORDINARY_SUITS, OrdinaryRank, OrdinarySuit, Suit, WildcardAssignment,
};

struct VirtualCard {
    rank: CardRank,
    suit: Suit,
}

type SemanticKey = (
    CombinationKind,
    usize,
    Option<CardRank>,
    Option<OrdinaryRank>,
    Option<OrdinarySuit>,
);

fn semantic_key(combination: &Combination) -> SemanticKey {
    (
        combination.kind,
        combination.size,
        combination.primary_rank,
        combination.sequence_top,
        combination.suit,
    )
}

fn count_ranks(cards: &[VirtualCard]) -> IndexMap<CardRank, usize> {
    let mut counts = IndexMap::new();
    for card in cards {
        *counts.entry(card.rank).or_insert(0) += 1;
    }
    counts
}

fn find_sequence_top(
    counts: &IndexMap<CardRank, usize>,
    sequence_length: usize,
    copies_per_rank: usize,
) -> Option<OrdinaryRank> {
    if counts.len() != sequence_length {
        return None;
    }
    let has_copies = |rank| counts.get(&CardRank::from(rank)).copied() == Some(copies_per_rank);
    if has_copies(OrdinaryRank::Ace)
        && ORDINARY_RANKS[..sequence_length - 1]
            .iter()
            .copied()
            .all(&has_copies)
    {
        return Some(ORDINARY_RANKS[sequence_length - 2]);
    }
    ORDINARY_RANKS
        .windows(sequence_length)
        .find(|pattern| pattern.iter().copied().all(&has_copies))
        .and_then(|pattern| pattern.last())
        .copied()
}

fn classify_resolved(
    cards: &[VirtualCard],
    wildcard_assignments: &IndexMap<String, WildcardAssignment>,
) -> Vec<Combination> {
    let mut combinations = Vec::new();
    let counts = count_ranks(cards);
    let size = cards.len();
    let ordinary_only = cards.iter().all(|card| card.suit != Suit::Joker);
    let first_rank = cards.first().map(|card| card.rank);
    let mut add = |kind, primary_rank, sequence_top, suit| {
        combinations.push(Combination {
            kind,
            size,
            primary_rank,
            sequence_top,
            suit,
            wildcard_assignments: wildcard_assignments.clone(),
        });
    };

    if size == 1 {
        add(CombinationKind::Single, first_rank, None, None);
    }

    if size == 2 && counts.len() == 1 {
        add(CombinationKind::Pair, first_rank, None, None);
    }

    if size == 3 && ordinary_only && counts.len() == 1 {
        add(CombinationKind::Triple, first_rank, None, None);
    }

    if size == 5
        && counts.len() == 2
        && let Some((&rank, _)) = counts
            .iter()
            .find(|(rank, count)| **count == 3 && rank.as_ordinary().is_some())
    {
        add(CombinationKind::FullHouse, Some(rank), None, None);
    }

    if let Some(sequence_top) = find_sequence_top(&counts, 5, 1) {
        add(CombinationKind::Straight, None, Some(sequence_top), None);

        if let Some(first_suit) = cards.first().and_then(|card| card.suit.as_ordinary())
            && cards
                .iter()
                .all(|card| card.suit.as_ordinary() == Some(first_suit))
        {
            add(
                CombinationKind::StraightFlush,
                None,
                Some(sequence_top),
                Some(first_suit),
            );
        }
    }

    if let Some(pair_top) = find_sequence_top(&counts, 3, 2) {
        add(
            CombinationKind::ConsecutivePairs,
            None,
            Some(pair_top),
            None,
        );
    }
    if let Some(triple_top) = find_sequence_top(&counts, 2, 3) {
        add(
            CombinationKind::ConsecutiveTriples,
            None,
            Some(triple_top),
            None,
        );
    }

    if size >= 4 && ordinary_only && counts.len() == 1 {
        add(CombinationKind::Bomb, first_rank, None, None);
    }

    combinations
}

fn visit_wildcard_assignments(
    cards: &[Card],
    wildcard_indexes: &[usize],
    wildcard_position: usize,
    assignments: &mut IndexMap<String, WildcardAssignment>,
    resolved: &mut Vec<(Vec<VirtualCard>, IndexMap<String, WildcardAssignment>)>,
) {
    if wildcard_position == wildcard_indexes.len() {
        let virtual_cards = cards
            .iter()
            .map(|card| {
                assignments.get(&card.id).map_or_else(
                    || VirtualCard {
                        rank: card.rank,
                        suit: card.suit,
                    },
                    |assignment| VirtualCard {
                        rank: CardRank::from(assignment.rank),
                        suit: Suit::from(assignment.suit),
                    },
                )
            })
            .collect();
        resolved.push((virtual_cards, assignments.clone()));
        return;
    }

    let card = &cards[wildcard_indexes[wildcard_position]];

    for rank in ORDINARY_RANKS {
        for suit in ORDINARY_SUITS {
            assignments.insert(card.id.clone(), WildcardAssignment { rank, suit });
            visit_wildcard_assignments(
                cards,
                wildcard_indexes,
                wildcard_position + 1,
                assignments,
                resolved,
            );
        }
    }
    assignments.shift_remove(&card.id);
}

fn enumerate_resolved_cards(
    cards: &[Card],
    level_rank: OrdinaryRank,
) -> Vec<(Vec<VirtualCard>, IndexMap<String, WildcardAssignment>)> {
    let wildcard_indexes = cards
        .iter()
        .enumerate()
        .filter_map(|(index, card)| is_wildcard(card, level_rank).then_some(index))
        .collect::<Vec<_>>();

    let mut resolved = Vec::new();
    visit_wildcard_assignments(
        cards,
        &wildcard_indexes,
        0,
        &mut IndexMap::new(),
        &mut resolved,
    );
    resolved
}

fn matches_declaration(combination: &Combination, declaration: &CombinationDeclaration) -> bool {
    combination.kind == declaration.kind
        && declaration
            .primary_rank
            .is_none_or(|rank| combination.primary_rank == Some(rank))
        && declaration
            .sequence_top
            .is_none_or(|rank| combination.sequence_top == Some(rank))
}

pub fn list_combinations(cards: &[Card], level_rank: OrdinaryRank) -> Vec<Combination> {
    if cards.is_empty() || cards.len() > 10 {
        return Vec::new();
    }
    if cards
        .iter()
        .map(|card| card.id.as_str())
        .collect::<HashSet<_>>()
        .len()
        != cards.len()
    {
        return Vec::new();
    }

    let all_joker = cards.len() == 4
        && cards
            .iter()
            .filter(|card| card.rank == CardRank::SmallJoker)
            .count()
            == 2
        && cards
            .iter()
            .filter(|card| card.rank == CardRank::BigJoker)
            .count()
            == 2;

    let mut unique: IndexMap<SemanticKey, Combination> = IndexMap::new();
    if all_joker {
        let combination = Combination {
            kind: CombinationKind::JokerBomb,
            size: 4,
            primary_rank: None,
            sequence_top: None,
            suit: None,
            wildcard_assignments: IndexMap::new(),
        };
        unique.insert(semantic_key(&combination), combination);
    }

    if cards.len() == 1 && is_wildcard(&cards[0], level_rank) {
        let combination = Combination {
            kind: CombinationKind::Single,
            size: 1,
            primary_rank: Some(CardRank::from(level_rank)),
            sequence_top: None,
            suit: None,
            wildcard_assignments: IndexMap::new(),
        };
        unique.insert(semantic_key(&combination), combination);
        return unique.into_values().collect();
    }

    for (resolved_cards, assignments) in enumerate_resolved_cards(cards, level_rank) {
        for combination in classify_resolved(&resolved_cards, &assignments) {
            unique
                .entry(semantic_key(&combination))
                .or_insert(combination);
        }
    }

    unique.into_values().collect()
}

pub fn resolve_combination(
    cards: &[Card],
    level_rank: OrdinaryRank,
    declaration: Option<&CombinationDeclaration>,
) -> Result<Combination, RuleError> {
    let candidates = list_combinations(cards, level_rank);
    let mut matches = candidates
        .into_iter()
        .filter(|candidate| {
            declaration.is_none_or(|declaration| matches_declaration(candidate, declaration))
        })
        .collect::<Vec<_>>();

    if matches.is_empty() {
        return Err(RuleError::new(
            "INVALID_COMBINATION",
            "所选牌张不能组成声明的合法牌型",
        ));
    }

    if matches.len() > 1 {
        let options = matches
            .iter()
            .map(|candidate| CombinationDeclaration {
                kind: candidate.kind,
                primary_rank: candidate.primary_rank,
                sequence_top: candidate.sequence_top,
            })
            .collect::<Vec<_>>();
        return Err(RuleError::new(
            "AMBIGUOUS_COMBINATION",
            "所选牌张可以解释为多种牌型，请明确声明",
        )
        .with_details(json!({ "options": options })));
    }

    Ok(matches.pop().expect("a single combination match exists"))
}

pub const fn is_bomb_combination(kind: CombinationKind) -> bool {
    matches!(
        kind,
        CombinationKind::Bomb | CombinationKind::StraightFlush | CombinationKind::JokerBomb
    )
}

fn bomb_strength(combination: &Combination, level_rank: OrdinaryRank) -> Option<(u8, usize, u8)> {
    match combination.kind {
        CombinationKind::JokerBomb => Some((4, 0, 0)),
        CombinationKind::Bomb if combination.size >= 6 => combination
            .primary_rank
            .map(|rank| (3, combination.size, card_rank_strength(rank, level_rank))),
        CombinationKind::StraightFlush => combination
            .sequence_top
            .map(|top| (2, 0, ordinary_rank_value(top))),
        CombinationKind::Bomb => combination.primary_rank.map(|rank| {
            (
                if combination.size == 5 { 1 } else { 0 },
                0,
                card_rank_strength(rank, level_rank),
            )
        }),
        _ => None,
    }
}

pub fn can_beat(
    challenger: &Combination,
    incumbent: &Combination,
    level_rank: OrdinaryRank,
) -> bool {
    let challenger_is_bomb = is_bomb_combination(challenger.kind);
    let incumbent_is_bomb = is_bomb_combination(incumbent.kind);

    if challenger_is_bomb != incumbent_is_bomb {
        return challenger_is_bomb;
    }
    if challenger_is_bomb {
        return bomb_strength(challenger, level_rank) > bomb_strength(incumbent, level_rank);
    }
    if challenger.kind != incumbent.kind || challenger.size != incumbent.size {
        return false;
    }

    if let (Some(challenger_top), Some(incumbent_top)) =
        (challenger.sequence_top, incumbent.sequence_top)
    {
        return ordinary_rank_value(challenger_top) > ordinary_rank_value(incumbent_top);
    }
    if let (Some(challenger_rank), Some(incumbent_rank)) =
        (challenger.primary_rank, incumbent.primary_rank)
    {
        return card_rank_strength(challenger_rank, level_rank)
            > card_rank_strength(incumbent_rank, level_rank);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::test_support::{card, spade, spades};
    use CardRank::*;

    fn declaration(
        kind: CombinationKind,
        primary_rank: Option<CardRank>,
        sequence_top: Option<OrdinaryRank>,
    ) -> CombinationDeclaration {
        CombinationDeclaration {
            kind,
            primary_rank,
            sequence_top,
        }
    }

    fn resolve(cards: &[Card], declaration: Option<&CombinationDeclaration>) -> Combination {
        resolve_combination(cards, OrdinaryRank::Seven, declaration).unwrap()
    }

    #[test]
    fn recognizes_all_ten_prescribed_combination_kinds() {
        use CombinationKind::*;

        let cases: [(CombinationKind, &[CardRank]); 8] = [
            (Single, &[Three]),
            (Pair, &[Three, Three]),
            (Triple, &[Four, Four, Four]),
            (FullHouse, &[Five, Five, Five, Nine, Nine]),
            (ConsecutivePairs, &[Three, Three, Four, Four, Five, Five]),
            (ConsecutiveTriples, &[Eight, Eight, Eight, Nine, Nine, Nine]),
            (Bomb, &[Queen, Queen, Queen, Queen]),
            (JokerBomb, &[SmallJoker, SmallJoker, BigJoker, BigJoker]),
        ];
        for (expected, ranks) in cases {
            assert_eq!(resolve(&spades(ranks), None).kind, expected, "{ranks:?}");
        }

        let mut straight = spades(&[Three, Four, Five, Six, Seven]);
        straight[4].suit = Suit::Club;
        assert_eq!(resolve(&straight, None).kind, Straight);

        let straight_flush =
            [Six, Seven, Eight, Nine, Ten].map(|rank| card(rank, OrdinarySuit::Heart));
        let declaration = declaration(StraightFlush, None, Some(OrdinaryRank::Ten));
        assert_eq!(
            resolve_combination(&straight_flush, OrdinaryRank::Queen, Some(&declaration),)
                .unwrap()
                .kind,
            StraightFlush
        );
    }

    #[test]
    fn ace_is_low_or_high_but_does_not_wrap() {
        for (ranks, top) in [
            (&[Ace, Two, Three, Four, Five][..], OrdinaryRank::Five),
            (&[Ten, Jack, Queen, King, Ace], OrdinaryRank::Ace),
        ] {
            let declaration = declaration(CombinationKind::Straight, None, Some(top));
            assert_eq!(
                resolve(&spades(ranks), Some(&declaration)).sequence_top,
                Some(top)
            );
        }
        assert!(
            list_combinations(&spades(&[Jack, Queen, King, Ace, Two]), OrdinaryRank::Seven,)
                .is_empty()
        );
    }

    #[test]
    fn wildcard_builds_bomb_but_is_natural_when_played_alone() {
        let wildcard = card(CardRank::Seven, OrdinarySuit::Heart);
        let bomb_declaration = declaration(CombinationKind::Bomb, Some(CardRank::Eight), None);
        let mut cards = spades(&[CardRank::Eight; 3]);
        cards.push(wildcard.clone());
        let bomb = resolve(&cards, Some(&bomb_declaration));
        assert_eq!(bomb.kind, CombinationKind::Bomb);
        assert_eq!(
            bomb.wildcard_assignments[&wildcard.id].rank,
            OrdinaryRank::Eight
        );

        let single = resolve(&[wildcard], None);
        assert_eq!(single.primary_rank, Some(CardRank::Seven));
        assert!(single.wildcard_assignments.is_empty());
    }

    #[test]
    fn bomb_hierarchy_matches_rules() {
        let four = resolve(&spades(&[Ace; 4]), None);
        let five = resolve(&spades(&[Two; 5]), None);
        let flush_declaration = declaration(
            CombinationKind::StraightFlush,
            None,
            Some(OrdinaryRank::Seven),
        );
        let straight_flush = resolve(
            &[Three, Four, Five, Six, Seven].map(|rank| card(rank, OrdinarySuit::Club)),
            Some(&flush_declaration),
        );
        let six = resolve(&spades(&[Three; 6]), None);
        let jokers = resolve(&spades(&[SmallJoker, SmallJoker, BigJoker, BigJoker]), None);

        assert!(can_beat(&five, &four, OrdinaryRank::Seven));
        assert!(can_beat(&straight_flush, &five, OrdinaryRank::Seven));
        assert!(can_beat(&six, &straight_flush, OrdinaryRank::Seven));
        assert!(can_beat(&jokers, &six, OrdinaryRank::Seven));
        assert!(!can_beat(&four, &four, OrdinaryRank::Seven));
    }

    #[test]
    fn straight_flush_is_ambiguous_without_a_declaration() {
        let cards = [Three, Four, Five, Six, Seven].map(|rank| card(rank, OrdinarySuit::Club));
        let error = resolve_combination(&cards, OrdinaryRank::Nine, None).unwrap_err();
        assert_eq!(error.code, "AMBIGUOUS_COMBINATION");
        assert_eq!(
            error.details.unwrap()["options"].as_array().unwrap().len(),
            2
        );
    }

    #[test]
    fn duplicate_ids_and_joker_triples_are_invalid() {
        let invalid = |cards: &[Card]| list_combinations(cards, OrdinaryRank::Seven).is_empty();
        let duplicated = spade(CardRank::Three);
        assert!(invalid(&[duplicated.clone(), duplicated]));
        assert!(invalid(&spades(&[CardRank::SmallJoker; 3])));
    }

    #[test]
    fn natural_level_strength_controls_non_sequence_comparison() {
        let level_single = resolve(&[spade(Seven)], None);
        let ace_single = resolve(&[spade(Ace)], None);
        assert!(can_beat(&level_single, &ace_single, OrdinaryRank::Seven));

        let low_pairs = declaration(
            CombinationKind::ConsecutivePairs,
            None,
            Some(OrdinaryRank::Three),
        );
        let combination = resolve(
            &spades(&[Ace, Ace, Two, Two, Three, Three]),
            Some(&low_pairs),
        );
        assert_eq!(combination.sequence_top, Some(OrdinaryRank::Three));
    }
}
