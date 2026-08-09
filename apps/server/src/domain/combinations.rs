use std::collections::HashSet;

use indexmap::IndexMap;
use serde_json::{Map, Value};

use super::cards::{card_rank_strength, is_wildcard, ordinary_rank_value};
use super::errors::RuleError;
use super::types::{
    Card, CardRank, Combination, CombinationDeclaration, CombinationKind, ORDINARY_RANKS,
    OrdinaryRank, OrdinarySuit, Suit, WildcardAssignment,
};

const ORDINARY_SUITS: [OrdinarySuit; 4] = [
    OrdinarySuit::Heart,
    OrdinarySuit::Diamond,
    OrdinarySuit::Club,
    OrdinarySuit::Spade,
];

#[derive(Clone)]
struct VirtualCard {
    rank: CardRank,
    suit: Suit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SemanticKey {
    kind: CombinationKind,
    size: usize,
    primary_rank: Option<CardRank>,
    sequence_top: Option<OrdinaryRank>,
    suit: Option<OrdinarySuit>,
}

impl From<&Combination> for SemanticKey {
    fn from(combination: &Combination) -> Self {
        Self {
            kind: combination.kind,
            size: combination.size,
            primary_rank: combination.primary_rank,
            sequence_top: combination.sequence_top,
            suit: combination.suit,
        }
    }
}

fn sequence_patterns(length: usize) -> Vec<Vec<OrdinaryRank>> {
    let mut patterns = Vec::new();
    let mut low_ace = Vec::with_capacity(length);
    low_ace.push(OrdinaryRank::Ace);
    low_ace.extend(
        ORDINARY_RANKS
            .iter()
            .copied()
            .take(length.saturating_sub(1)),
    );
    patterns.push(low_ace);

    for start in 0..=ORDINARY_RANKS.len() - length {
        patterns.push(ORDINARY_RANKS[start..start + length].to_vec());
    }
    patterns
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
    for pattern in sequence_patterns(sequence_length) {
        if counts.len() == sequence_length
            && pattern
                .iter()
                .all(|rank| counts.get(&CardRank::from(*rank)).copied() == Some(copies_per_rank))
        {
            return pattern.last().copied();
        }
    }
    None
}

fn add_combination(
    combinations: &mut Vec<Combination>,
    wildcard_assignments: &IndexMap<String, WildcardAssignment>,
    kind: CombinationKind,
    size: usize,
    primary_rank: Option<CardRank>,
    sequence_top: Option<OrdinaryRank>,
    suit: Option<OrdinarySuit>,
) {
    combinations.push(Combination {
        kind,
        size,
        primary_rank,
        sequence_top,
        suit,
        wildcard_assignments: wildcard_assignments.clone(),
    });
}

fn classify_resolved(
    cards: &[VirtualCard],
    wildcard_assignments: &IndexMap<String, WildcardAssignment>,
) -> Vec<Combination> {
    let mut combinations = Vec::new();
    let counts = count_ranks(cards);
    let size = cards.len();
    let ordinary_only = cards.iter().all(|card| card.suit != Suit::Joker);

    if size == 1
        && let Some(card) = cards.first()
    {
        add_combination(
            &mut combinations,
            wildcard_assignments,
            CombinationKind::Single,
            size,
            Some(card.rank),
            None,
            None,
        );
    }

    if size == 2 && counts.len() == 1 {
        add_combination(
            &mut combinations,
            wildcard_assignments,
            CombinationKind::Pair,
            size,
            cards.first().map(|card| card.rank),
            None,
            None,
        );
    }

    if size == 3 && ordinary_only && counts.len() == 1 {
        add_combination(
            &mut combinations,
            wildcard_assignments,
            CombinationKind::Triple,
            size,
            cards.first().map(|card| card.rank),
            None,
            None,
        );
    }

    if size == 5 {
        for (rank, count) in &counts {
            if *count == 3 && rank.as_ordinary().is_some() {
                let has_pair = counts
                    .iter()
                    .any(|(pair_rank, pair_count)| pair_rank != rank && *pair_count == 2);
                if has_pair {
                    add_combination(
                        &mut combinations,
                        wildcard_assignments,
                        CombinationKind::FullHouse,
                        size,
                        Some(*rank),
                        None,
                        None,
                    );
                }
            }
        }

        if ordinary_only && let Some(sequence_top) = find_sequence_top(&counts, 5, 1) {
            add_combination(
                &mut combinations,
                wildcard_assignments,
                CombinationKind::Straight,
                size,
                None,
                Some(sequence_top),
                None,
            );

            if let Some(first_suit) = cards.first().and_then(|card| card.suit.as_ordinary())
                && cards
                    .iter()
                    .all(|card| card.suit.as_ordinary() == Some(first_suit))
            {
                add_combination(
                    &mut combinations,
                    wildcard_assignments,
                    CombinationKind::StraightFlush,
                    size,
                    None,
                    Some(sequence_top),
                    Some(first_suit),
                );
            }
        }
    }

    if size == 6 && ordinary_only {
        if let Some(pair_top) = find_sequence_top(&counts, 3, 2) {
            add_combination(
                &mut combinations,
                wildcard_assignments,
                CombinationKind::ConsecutivePairs,
                size,
                None,
                Some(pair_top),
                None,
            );
        }
        if let Some(triple_top) = find_sequence_top(&counts, 2, 3) {
            add_combination(
                &mut combinations,
                wildcard_assignments,
                CombinationKind::ConsecutiveTriples,
                size,
                None,
                Some(triple_top),
                None,
            );
        }
    }

    if size >= 4 && ordinary_only && counts.len() == 1 {
        add_combination(
            &mut combinations,
            wildcard_assignments,
            CombinationKind::Bomb,
            size,
            cards.first().map(|card| card.rank),
            None,
            None,
        );
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

    let Some(card) = wildcard_indexes
        .get(wildcard_position)
        .and_then(|index| cards.get(*index))
    else {
        return;
    };

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

    if wildcard_indexes.is_empty() {
        return vec![(
            cards
                .iter()
                .map(|card| VirtualCard {
                    rank: card.rank,
                    suit: card.suit,
                })
                .collect(),
            IndexMap::new(),
        )];
    }

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
        unique.insert(SemanticKey::from(&combination), combination);
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
        unique.insert(SemanticKey::from(&combination), combination);
        return unique.into_values().collect();
    }

    for (resolved_cards, assignments) in enumerate_resolved_cards(cards, level_rank) {
        for combination in classify_resolved(&resolved_cards, &assignments) {
            unique
                .entry(SemanticKey::from(&combination))
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
    let matches = candidates
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
            .map(|candidate| {
                let mut option = Map::new();
                option.insert(
                    "kind".into(),
                    serde_json::to_value(candidate.kind).expect("combination kind is serializable"),
                );
                if let Some(primary_rank) = candidate.primary_rank {
                    option.insert(
                        "primaryRank".into(),
                        serde_json::to_value(primary_rank).expect("card rank is serializable"),
                    );
                }
                if let Some(sequence_top) = candidate.sequence_top {
                    option.insert(
                        "sequenceTop".into(),
                        serde_json::to_value(sequence_top).expect("ordinary rank is serializable"),
                    );
                }
                Value::Object(option)
            })
            .collect();
        return Err(RuleError::new(
            "AMBIGUOUS_COMBINATION",
            "所选牌张可以解释为多种牌型，请明确声明",
        )
        .with_details(Value::Object(Map::from_iter([(
            "options".into(),
            Value::Array(options),
        )]))));
    }

    Ok(matches
        .into_iter()
        .next()
        .expect("a single combination match exists"))
}

pub const fn is_bomb_combination(kind: CombinationKind) -> bool {
    matches!(
        kind,
        CombinationKind::Bomb | CombinationKind::StraightFlush | CombinationKind::JokerBomb
    )
}

fn sequence_strength(rank: OrdinaryRank) -> u8 {
    ordinary_rank_value(rank)
}

fn bomb_strength(combination: &Combination, level_rank: OrdinaryRank) -> Option<(u8, usize, u8)> {
    match combination.kind {
        CombinationKind::JokerBomb => Some((4, 0, 0)),
        CombinationKind::Bomb if combination.size >= 6 => combination
            .primary_rank
            .map(|rank| (3, combination.size, card_rank_strength(rank, level_rank))),
        CombinationKind::StraightFlush => combination
            .sequence_top
            .map(|top| (2, 0, sequence_strength(top))),
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

    if challenger_is_bomb && !incumbent_is_bomb {
        return true;
    }
    if !challenger_is_bomb && incumbent_is_bomb {
        return false;
    }
    if challenger_is_bomb && incumbent_is_bomb {
        return bomb_strength(challenger, level_rank) > bomb_strength(incumbent, level_rank);
    }
    if challenger.kind != incumbent.kind || challenger.size != incumbent.size {
        return false;
    }

    if let (Some(challenger_top), Some(incumbent_top)) =
        (challenger.sequence_top, incumbent.sequence_top)
    {
        return sequence_strength(challenger_top) > sequence_strength(incumbent_top);
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    static NEXT_CARD_ID: AtomicUsize = AtomicUsize::new(0);

    fn card(rank: CardRank, suit: OrdinarySuit) -> Card {
        Card {
            id: format!("test-card-{}", NEXT_CARD_ID.fetch_add(1, Ordering::Relaxed)),
            deck_index: 0,
            suit: if matches!(rank, CardRank::SmallJoker | CardRank::BigJoker) {
                Suit::Joker
            } else {
                Suit::from(suit)
            },
            rank,
        }
    }

    fn spade(rank: CardRank) -> Card {
        card(rank, OrdinarySuit::Spade)
    }

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
        assert_eq!(
            resolve(&[spade(CardRank::Three)], None).kind,
            CombinationKind::Single
        );
        assert_eq!(
            resolve(&[spade(CardRank::Three), spade(CardRank::Three)], None).kind,
            CombinationKind::Pair
        );
        assert_eq!(
            resolve(
                &[
                    spade(CardRank::Four),
                    spade(CardRank::Four),
                    spade(CardRank::Four),
                ],
                None,
            )
            .kind,
            CombinationKind::Triple
        );
        assert_eq!(
            resolve(
                &[
                    spade(CardRank::Five),
                    spade(CardRank::Five),
                    spade(CardRank::Five),
                    spade(CardRank::Nine),
                    spade(CardRank::Nine),
                ],
                None,
            )
            .kind,
            CombinationKind::FullHouse
        );
        assert_eq!(
            resolve(
                &[
                    spade(CardRank::Three),
                    spade(CardRank::Four),
                    spade(CardRank::Five),
                    spade(CardRank::Six),
                    card(CardRank::Seven, OrdinarySuit::Club),
                ],
                None,
            )
            .kind,
            CombinationKind::Straight
        );
        assert_eq!(
            resolve(
                &[
                    spade(CardRank::Three),
                    spade(CardRank::Three),
                    spade(CardRank::Four),
                    spade(CardRank::Four),
                    spade(CardRank::Five),
                    spade(CardRank::Five),
                ],
                None,
            )
            .kind,
            CombinationKind::ConsecutivePairs
        );
        assert_eq!(
            resolve(
                &[
                    spade(CardRank::Eight),
                    spade(CardRank::Eight),
                    spade(CardRank::Eight),
                    spade(CardRank::Nine),
                    spade(CardRank::Nine),
                    spade(CardRank::Nine),
                ],
                None,
            )
            .kind,
            CombinationKind::ConsecutiveTriples
        );
        assert_eq!(
            resolve(
                &[
                    spade(CardRank::Queen),
                    spade(CardRank::Queen),
                    spade(CardRank::Queen),
                    spade(CardRank::Queen),
                ],
                None,
            )
            .kind,
            CombinationKind::Bomb
        );

        let straight_flush = [
            card(CardRank::Six, OrdinarySuit::Heart),
            card(CardRank::Seven, OrdinarySuit::Heart),
            card(CardRank::Eight, OrdinarySuit::Heart),
            card(CardRank::Nine, OrdinarySuit::Heart),
            card(CardRank::Ten, OrdinarySuit::Heart),
        ];
        let straight_flush_declaration = declaration(
            CombinationKind::StraightFlush,
            None,
            Some(OrdinaryRank::Ten),
        );
        assert_eq!(
            resolve_combination(
                &straight_flush,
                OrdinaryRank::Queen,
                Some(&straight_flush_declaration),
            )
            .unwrap()
            .kind,
            CombinationKind::StraightFlush
        );

        assert_eq!(
            resolve(
                &[
                    spade(CardRank::SmallJoker),
                    spade(CardRank::SmallJoker),
                    spade(CardRank::BigJoker),
                    spade(CardRank::BigJoker),
                ],
                None,
            )
            .kind,
            CombinationKind::JokerBomb
        );
    }

    #[test]
    fn ace_is_low_or_high_but_does_not_wrap() {
        let low = declaration(CombinationKind::Straight, None, Some(OrdinaryRank::Five));
        assert_eq!(
            resolve(
                &[
                    spade(CardRank::Ace),
                    spade(CardRank::Two),
                    spade(CardRank::Three),
                    spade(CardRank::Four),
                    spade(CardRank::Five),
                ],
                Some(&low),
            )
            .sequence_top,
            Some(OrdinaryRank::Five)
        );
        let high = declaration(CombinationKind::Straight, None, Some(OrdinaryRank::Ace));
        assert_eq!(
            resolve(
                &[
                    spade(CardRank::Ten),
                    spade(CardRank::Jack),
                    spade(CardRank::Queen),
                    spade(CardRank::King),
                    spade(CardRank::Ace),
                ],
                Some(&high),
            )
            .sequence_top,
            Some(OrdinaryRank::Ace)
        );
        assert!(
            list_combinations(
                &[
                    spade(CardRank::Jack),
                    spade(CardRank::Queen),
                    spade(CardRank::King),
                    spade(CardRank::Ace),
                    spade(CardRank::Two),
                ],
                OrdinaryRank::Seven,
            )
            .is_empty()
        );
    }

    #[test]
    fn wildcard_builds_bomb_but_is_natural_when_played_alone() {
        let wildcard = card(CardRank::Seven, OrdinarySuit::Heart);
        let bomb_declaration = declaration(CombinationKind::Bomb, Some(CardRank::Eight), None);
        let bomb = resolve(
            &[
                spade(CardRank::Eight),
                spade(CardRank::Eight),
                spade(CardRank::Eight),
                wildcard.clone(),
            ],
            Some(&bomb_declaration),
        );
        assert_eq!(bomb.kind, CombinationKind::Bomb);
        assert_eq!(
            bomb.wildcard_assignments
                .get(&wildcard.id)
                .map(|item| item.rank),
            Some(OrdinaryRank::Eight)
        );

        let single = resolve(&[wildcard], None);
        assert_eq!(single.primary_rank, Some(CardRank::Seven));
        assert!(single.wildcard_assignments.is_empty());
    }

    #[test]
    fn bomb_hierarchy_matches_rules() {
        let four = resolve(
            &[
                spade(CardRank::Ace),
                spade(CardRank::Ace),
                spade(CardRank::Ace),
                spade(CardRank::Ace),
            ],
            None,
        );
        let five = resolve(
            &[
                spade(CardRank::Two),
                spade(CardRank::Two),
                spade(CardRank::Two),
                spade(CardRank::Two),
                spade(CardRank::Two),
            ],
            None,
        );
        let flush_declaration = declaration(
            CombinationKind::StraightFlush,
            None,
            Some(OrdinaryRank::Seven),
        );
        let straight_flush = resolve(
            &[
                card(CardRank::Three, OrdinarySuit::Club),
                card(CardRank::Four, OrdinarySuit::Club),
                card(CardRank::Five, OrdinarySuit::Club),
                card(CardRank::Six, OrdinarySuit::Club),
                card(CardRank::Seven, OrdinarySuit::Club),
            ],
            Some(&flush_declaration),
        );
        let six = resolve(
            &[
                spade(CardRank::Three),
                spade(CardRank::Three),
                spade(CardRank::Three),
                spade(CardRank::Three),
                spade(CardRank::Three),
                spade(CardRank::Three),
            ],
            None,
        );
        let jokers = resolve(
            &[
                spade(CardRank::SmallJoker),
                spade(CardRank::SmallJoker),
                spade(CardRank::BigJoker),
                spade(CardRank::BigJoker),
            ],
            None,
        );

        assert!(can_beat(&five, &four, OrdinaryRank::Seven));
        assert!(can_beat(&straight_flush, &five, OrdinaryRank::Seven));
        assert!(can_beat(&six, &straight_flush, OrdinaryRank::Seven));
        assert!(can_beat(&jokers, &six, OrdinaryRank::Seven));
        assert!(!can_beat(&four, &four, OrdinaryRank::Seven));
    }

    #[test]
    fn straight_flush_is_ambiguous_without_a_declaration() {
        let cards = [
            card(CardRank::Three, OrdinarySuit::Club),
            card(CardRank::Four, OrdinarySuit::Club),
            card(CardRank::Five, OrdinarySuit::Club),
            card(CardRank::Six, OrdinarySuit::Club),
            card(CardRank::Seven, OrdinarySuit::Club),
        ];
        let error = resolve_combination(&cards, OrdinaryRank::Nine, None).unwrap_err();
        assert_eq!(error.code, "AMBIGUOUS_COMBINATION");
        assert_eq!(
            error.details.unwrap()["options"].as_array().unwrap().len(),
            2
        );
    }

    #[test]
    fn duplicate_ids_and_joker_triples_are_invalid() {
        let duplicated = spade(CardRank::Three);
        assert!(
            list_combinations(&[duplicated.clone(), duplicated], OrdinaryRank::Seven).is_empty()
        );
        assert!(
            list_combinations(
                &[
                    spade(CardRank::SmallJoker),
                    spade(CardRank::SmallJoker),
                    spade(CardRank::SmallJoker),
                ],
                OrdinaryRank::Seven,
            )
            .is_empty()
        );
    }

    #[test]
    fn natural_level_strength_controls_non_sequence_comparison() {
        let level_single = resolve(&[spade(CardRank::Seven)], None);
        let ace_single = resolve(&[spade(CardRank::Ace)], None);
        assert!(can_beat(&level_single, &ace_single, OrdinaryRank::Seven));

        let low_pairs = declaration(
            CombinationKind::ConsecutivePairs,
            None,
            Some(OrdinaryRank::Three),
        );
        let combination = resolve(
            &[
                spade(CardRank::Ace),
                spade(CardRank::Ace),
                spade(CardRank::Two),
                spade(CardRank::Two),
                spade(CardRank::Three),
                spade(CardRank::Three),
            ],
            Some(&low_pairs),
        );
        assert_eq!(combination.sequence_top, Some(OrdinaryRank::Three));
    }
}
