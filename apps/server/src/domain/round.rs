use std::collections::HashSet;

use indexmap::{IndexMap, IndexSet};
use rand::RngExt;
use serde::{Deserialize, Serialize};

use super::cards::{
    card_rank_strength, create_deck, deal_cards, is_wildcard, ordinary_rank_value,
    shuffle_cards_with,
};
use super::combinations::{can_beat, resolve_combination};
use super::errors::RuleError;
use super::types::{Card, CardRank, Combination, CombinationDeclaration, OrdinaryRank, Seat, Team};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoundPhase {
    Tribute,
    Playing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TributeKind {
    Single,
    Double,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TributeStage {
    Giving,
    Returning,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayedCombination {
    pub seat: Seat,
    pub cards: Vec<Card>,
    pub combination: Combination,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundResult {
    pub winner_team: Team,
    pub finish_order: Vec<Seat>,
    pub double_last_seats: Vec<Seat>,
    pub partner_placement: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TributeState {
    pub kind: TributeKind,
    pub stage: TributeStage,
    pub previous_first: Seat,
    pub previous_second: Option<Seat>,
    pub givers: Vec<Seat>,
    pub contributions: IndexMap<Seat, Card>,
    pub receiver_for_giver: IndexMap<Seat, Seat>,
    pub returns: IndexMap<Seat, Card>,
    pub leader_seat: Option<Seat>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundState {
    pub number: u32,
    pub level_rank: OrdinaryRank,
    pub level_owner_team: Option<Team>,
    pub phase: RoundPhase,
    pub hands: [Vec<Card>; 4],
    pub active_seats: IndexSet<Seat>,
    pub turn_seat: Seat,
    pub current_play: Option<PlayedCombination>,
    pub consecutive_passes: usize,
    pub finish_order: Vec<Seat>,
    pub tribute: Option<TributeState>,
}

#[derive(Clone, Debug)]
pub struct CreateRoundOptions {
    pub number: u32,
    pub level_rank: OrdinaryRank,
    pub level_owner_team: Option<Team>,
    pub previous_result: Option<RoundResult>,
}

fn count_big_jokers(hand: &[Card]) -> usize {
    hand.iter()
        .filter(|card| card.rank == CardRank::BigJoker)
        .count()
}

fn next_active_seat(from: Seat, active_seats: &IndexSet<Seat>) -> Result<Seat, RuleError> {
    let mut candidate = from;
    for _ in 0..4 {
        candidate = candidate.next();
        if active_seats.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(RuleError::new("NO_ACTIVE_PLAYER", "没有可继续行动的玩家"))
}

fn card_from_hand(round: &RoundState, seat: Seat, card_id: &str) -> Result<Card, RuleError> {
    round.hands[seat.index()]
        .iter()
        .find(|card| card.id == card_id)
        .cloned()
        .ok_or_else(|| RuleError::new("CARD_NOT_OWNED", "所选牌张不在玩家手中"))
}

fn remove_card_ids(hand: &mut Vec<Card>, card_ids: &HashSet<&str>) {
    hand.retain(|card| !card_ids.contains(card.id.as_str()));
}

fn remove_one_card(hand: &mut Vec<Card>, card_id: &str) {
    hand.retain(|card| card.id != card_id);
}

fn start_playing(round: &mut RoundState, leader_seat: Seat) {
    round.phase = RoundPhase::Playing;
    round.turn_seat = leader_seat;
    round.tribute = None;
}

fn create_tribute_state(
    hands: &[Vec<Card>; 4],
    previous_result: &RoundResult,
) -> Result<Option<TributeState>, RuleError> {
    let previous_first = previous_result
        .finish_order
        .first()
        .copied()
        .ok_or_else(|| RuleError::internal("Previous round has no first finisher"))?;

    if previous_result.double_last_seats.len() == 2 {
        let givers = previous_result.double_last_seats.clone();
        let big_jokers = givers
            .iter()
            .map(|seat| count_big_jokers(&hands[seat.index()]))
            .sum::<usize>();
        if big_jokers == 2 {
            return Ok(None);
        }

        return Ok(Some(TributeState {
            kind: TributeKind::Double,
            stage: TributeStage::Giving,
            previous_first,
            previous_second: Some(previous_first.partner()),
            givers,
            contributions: IndexMap::new(),
            receiver_for_giver: IndexMap::new(),
            returns: IndexMap::new(),
            leader_seat: None,
        }));
    }

    let previous_last = previous_result
        .finish_order
        .get(3)
        .copied()
        .ok_or_else(|| RuleError::internal("Previous round has no last finisher"))?;
    if count_big_jokers(&hands[previous_last.index()]) == 2 {
        return Ok(None);
    }

    Ok(Some(TributeState {
        kind: TributeKind::Single,
        stage: TributeStage::Giving,
        previous_first,
        previous_second: None,
        givers: vec![previous_last],
        contributions: IndexMap::new(),
        receiver_for_giver: IndexMap::from_iter([(previous_last, previous_first)]),
        returns: IndexMap::new(),
        leader_seat: Some(previous_last),
    }))
}

pub fn create_round(options: CreateRoundOptions) -> Result<RoundState, RuleError> {
    let mut rng = rand::rng();
    create_round_with_random(options, &mut |upper_exclusive| {
        rng.random_range(0..upper_exclusive)
    })
}

pub fn create_round_with_random<F>(
    options: CreateRoundOptions,
    random_index: &mut F,
) -> Result<RoundState, RuleError>
where
    F: FnMut(usize) -> usize + ?Sized,
{
    let shuffled = shuffle_cards_with(&create_deck(), random_index)?;
    let hands = deal_cards(&shuffled)?;
    let random_leader_index = random_index(4);
    let random_leader = Seat::new(random_leader_index as u8)
        .filter(|_| random_leader_index < 4)
        .ok_or_else(|| RuleError::internal("Invalid random leader index"))?;
    let tribute = options
        .previous_result
        .as_ref()
        .map(|result| create_tribute_state(&hands, result))
        .transpose()?
        .flatten();

    let initial_turn = options
        .previous_result
        .as_ref()
        .and_then(|result| result.finish_order.first().copied())
        .unwrap_or(random_leader);
    let phase = if tribute.is_some() {
        RoundPhase::Tribute
    } else {
        RoundPhase::Playing
    };

    Ok(RoundState {
        number: options.number,
        level_rank: options.level_rank,
        level_owner_team: options.level_owner_team,
        phase,
        hands,
        active_seats: Seat::all().into_iter().collect(),
        turn_seat: initial_turn,
        current_play: None,
        consecutive_passes: 0,
        finish_order: Vec::new(),
        tribute,
    })
}

fn validate_turn(
    round: &RoundState,
    seat: Seat,
    expected_phase: RoundPhase,
) -> Result<(), RuleError> {
    if round.phase != expected_phase {
        let phase = match expected_phase {
            RoundPhase::Tribute => "tribute",
            RoundPhase::Playing => "playing",
        };
        return Err(RuleError::new(
            "INVALID_PHASE",
            format!("当前轮牌不处于 {phase} 阶段"),
        ));
    }
    if expected_phase == RoundPhase::Playing && round.turn_seat != seat {
        return Err(RuleError::new("NOT_YOUR_TURN", "尚未轮到该玩家行动"));
    }
    Ok(())
}

fn is_maximum_tribute_card(hand: &[Card], selected: &Card, level_rank: OrdinaryRank) -> bool {
    hand.iter()
        .filter(|card| !is_wildcard(card, level_rank))
        .map(|card| card_rank_strength(card.rank, level_rank))
        .max()
        .is_some_and(|maximum| card_rank_strength(selected.rank, level_rank) == maximum)
}

fn clockwise_distance(from: Seat, to: Seat) -> u8 {
    (from.value() + 4 - to.value()) % 4
}

fn finalize_contributions(round: &mut RoundState) -> Result<(), RuleError> {
    let tribute = round
        .tribute
        .as_mut()
        .ok_or_else(|| RuleError::internal("Missing tribute state"))?;
    if tribute.kind == TributeKind::Double {
        let incomplete = || RuleError::internal("Incomplete double tribute");
        let &[first_giver, second_giver, ..] = tribute.givers.as_slice() else {
            return Err(incomplete());
        };
        let first_card = tribute
            .contributions
            .get(&first_giver)
            .ok_or_else(&incomplete)?;
        let second_card = tribute
            .contributions
            .get(&second_giver)
            .ok_or_else(&incomplete)?;
        let previous_second = tribute.previous_second.ok_or_else(&incomplete)?;

        let first_strength = card_rank_strength(first_card.rank, round.level_rank);
        let second_strength = card_rank_strength(second_card.rank, round.level_rank);
        let first_wins = first_strength > second_strength
            || first_strength == second_strength
                && clockwise_distance(tribute.previous_first, first_giver)
                    <= clockwise_distance(tribute.previous_first, second_giver);
        let (higher_giver, lower_giver) = if first_wins {
            (first_giver, second_giver)
        } else {
            (second_giver, first_giver)
        };
        tribute
            .receiver_for_giver
            .insert(higher_giver, tribute.previous_first);
        tribute
            .receiver_for_giver
            .insert(lower_giver, previous_second);
        tribute.leader_seat = Some(higher_giver);
    }

    let transfers = tribute
        .contributions
        .iter()
        .map(|(giver, card)| {
            tribute
                .receiver_for_giver
                .get(giver)
                .copied()
                .map(|receiver| (*giver, receiver, card.clone()))
                .ok_or_else(|| RuleError::internal("Tribute receiver was not assigned"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    for (giver, receiver, card) in transfers {
        remove_one_card(&mut round.hands[giver.index()], &card.id);
        round.hands[receiver.index()].push(card);
    }
    tribute.stage = TributeStage::Returning;
    Ok(())
}

pub fn submit_tribute(round: &mut RoundState, seat: Seat, card_id: &str) -> Result<(), RuleError> {
    validate_turn(round, seat, RoundPhase::Tribute)?;
    let tribute = round
        .tribute
        .as_ref()
        .filter(|tribute| tribute.stage == TributeStage::Giving)
        .ok_or_else(|| RuleError::new("INVALID_TRIBUTE_STAGE", "当前不接受贡牌"))?;
    if !tribute.givers.contains(&seat) {
        return Err(RuleError::new("NOT_TRIBUTE_GIVER", "该玩家无需贡牌"));
    }
    if tribute.contributions.contains_key(&seat) {
        return Err(RuleError::new("TRIBUTE_ALREADY_GIVEN", "该玩家已经贡牌"));
    }

    let card = card_from_hand(round, seat, card_id)?;
    let hand = &round.hands[seat.index()];
    if is_wildcard(&card, round.level_rank) {
        return Err(RuleError::new(
            "WILDCARD_CANNOT_BE_TRIBUTE",
            "红桃级牌不能用于进贡",
        ));
    }
    if !is_maximum_tribute_card(hand, &card, round.level_rank) {
        return Err(RuleError::new(
            "TRIBUTE_NOT_MAXIMUM",
            "必须进贡手中最大的合资格牌",
        ));
    }

    let tribute = round.tribute.as_mut().expect("tribute was validated");
    tribute.contributions.insert(seat, card);
    if tribute.contributions.len() == tribute.givers.len() {
        finalize_contributions(round)?;
    }
    Ok(())
}

fn can_return_card(hand: &[Card], selected: &Card, level_rank: OrdinaryRank) -> bool {
    let is_low_card = |card: &Card| {
        card.suit != super::types::Suit::Joker
            && card
                .rank
                .as_ordinary()
                .is_some_and(|rank| ordinary_rank_value(rank) <= 10)
    };
    if hand.iter().any(&is_low_card) {
        return is_low_card(selected);
    }

    hand.iter()
        .map(|card| card_rank_strength(card.rank, level_rank))
        .min()
        .is_some_and(|minimum| card_rank_strength(selected.rank, level_rank) == minimum)
}

pub fn submit_return(
    round: &mut RoundState,
    seat: Seat,
    card_id: &str,
) -> Result<Option<TributeState>, RuleError> {
    validate_turn(round, seat, RoundPhase::Tribute)?;
    let tribute = round
        .tribute
        .as_ref()
        .filter(|tribute| tribute.stage == TributeStage::Returning)
        .ok_or_else(|| RuleError::new("INVALID_TRIBUTE_STAGE", "当前不接受还牌"))?;
    if !tribute
        .receiver_for_giver
        .values()
        .any(|receiver| *receiver == seat)
    {
        return Err(RuleError::new("NOT_TRIBUTE_RECEIVER", "该玩家无需还牌"));
    }
    if tribute.returns.contains_key(&seat) {
        return Err(RuleError::new("RETURN_ALREADY_GIVEN", "该玩家已经还牌"));
    }

    let card = card_from_hand(round, seat, card_id)?;
    if !can_return_card(&round.hands[seat.index()], &card, round.level_rank) {
        return Err(RuleError::new(
            "INVALID_RETURN_CARD",
            "还牌必须不大于 10；若无此类牌则必须还最小牌",
        ));
    }

    let tribute = round.tribute.as_mut().expect("tribute was validated");
    tribute.returns.insert(seat, card);
    if tribute.returns.len() != tribute.receiver_for_giver.len() {
        return Ok(None);
    }

    let transfers = tribute
        .receiver_for_giver
        .iter()
        .map(|(giver, receiver)| {
            tribute
                .returns
                .get(receiver)
                .cloned()
                .map(|card| (*giver, *receiver, card))
                .ok_or_else(|| RuleError::internal("Missing return card"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let leader_seat = tribute
        .leader_seat
        .ok_or_else(|| RuleError::internal("Tribute leader was not assigned"))?;
    let completed_tribute = tribute.clone();

    for (giver, receiver, returned) in transfers {
        remove_one_card(&mut round.hands[receiver.index()], &returned.id);
        round.hands[giver.index()].push(returned);
    }
    start_playing(round, leader_seat);
    Ok(Some(completed_tribute))
}

fn create_round_result(round: &RoundState) -> Result<Option<RoundResult>, RuleError> {
    let Some(first) = round.finish_order.first().copied() else {
        return Ok(None);
    };

    if let Some(second) = round.finish_order.get(1).copied()
        && first.team() == second.team()
    {
        return Ok(Some(RoundResult {
            winner_team: first.team(),
            finish_order: vec![first, second],
            double_last_seats: round.active_seats.iter().copied().collect(),
            partner_placement: 2,
        }));
    }

    if round.finish_order.len() == 3 {
        let last = round
            .active_seats
            .first()
            .copied()
            .ok_or_else(|| RuleError::internal("Missing last active player"))?;
        let mut finish_order = round.finish_order.clone();
        finish_order.push(last);
        let partner = first.partner();
        let partner_index = finish_order
            .iter()
            .position(|seat| *seat == partner)
            .ok_or_else(|| RuleError::internal("Invalid partner placement"))?;
        let partner_placement = (partner_index + 1) as u8;
        if !matches!(partner_placement, 3 | 4) {
            return Err(RuleError::internal("Invalid partner placement"));
        }
        return Ok(Some(RoundResult {
            winner_team: first.team(),
            finish_order,
            double_last_seats: Vec::new(),
            partner_placement,
        }));
    }
    Ok(None)
}

pub fn play_cards(
    round: &mut RoundState,
    seat: Seat,
    card_ids: &[String],
    declaration: Option<&CombinationDeclaration>,
) -> Result<Option<RoundResult>, RuleError> {
    validate_turn(round, seat, RoundPhase::Playing)?;
    if !round.active_seats.contains(&seat) {
        return Err(RuleError::new("PLAYER_FINISHED", "该玩家已经出完手牌"));
    }
    let unique_ids = card_ids.iter().map(String::as_str).collect::<HashSet<_>>();
    if card_ids.is_empty() || unique_ids.len() != card_ids.len() {
        return Err(RuleError::new(
            "INVALID_CARD_SELECTION",
            "必须选择至少一张且不得重复选择牌张",
        ));
    }

    let cards = card_ids
        .iter()
        .map(|card_id| card_from_hand(round, seat, card_id))
        .collect::<Result<Vec<_>, _>>()?;
    let combination = resolve_combination(&cards, round.level_rank, declaration)?;
    if round.current_play.as_ref().is_some_and(|current_play| {
        !can_beat(&combination, &current_play.combination, round.level_rank)
    }) {
        return Err(RuleError::new(
            "COMBINATION_TOO_SMALL",
            "所出牌型不能压制当前牌型",
        ));
    }

    remove_card_ids(&mut round.hands[seat.index()], &unique_ids);
    round.current_play = Some(PlayedCombination {
        seat,
        cards,
        combination,
    });
    round.consecutive_passes = 0;

    if round.hands[seat.index()].is_empty() {
        round.finish_order.push(seat);
        round.active_seats.shift_remove(&seat);
        if let Some(result) = create_round_result(round)? {
            return Ok(Some(result));
        }
    }

    round.turn_seat = next_active_seat(seat, &round.active_seats)?;
    Ok(None)
}

pub fn pass_turn(round: &mut RoundState, seat: Seat) -> Result<(), RuleError> {
    validate_turn(round, seat, RoundPhase::Playing)?;
    let Some(current_play) = round.current_play.as_ref() else {
        return Err(RuleError::new(
            "CANNOT_PASS_WHEN_LEADING",
            "领出玩家不能过牌",
        ));
    };

    round.consecutive_passes += 1;
    let last_played_seat = current_play.seat;
    let last_player_still_active = round.active_seats.contains(&last_played_seat);
    let passes_needed = round.active_seats.len() - usize::from(last_player_still_active);

    if round.consecutive_passes >= passes_needed {
        let leader = if round.active_seats.contains(&last_played_seat) {
            last_played_seat
        } else if round.active_seats.contains(&last_played_seat.partner()) {
            last_played_seat.partner()
        } else {
            next_active_seat(last_played_seat, &round.active_seats)?
        };
        round.current_play = None;
        round.consecutive_passes = 0;
        round.turn_seat = leader;
        return Ok(());
    }

    round.turn_seat = next_active_seat(seat, &round.active_seats)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        test_support::{card as suited_card, simple_round, spade as card},
        types::Suit,
    };

    fn play_only_card(round: &mut RoundState, seat: Seat) -> Option<RoundResult> {
        let card_id = round.hands[seat.index()][0].id.clone();
        play_cards(round, seat, &[card_id], None).unwrap()
    }

    fn single_tribute_state() -> TributeState {
        TributeState {
            kind: TributeKind::Single,
            stage: TributeStage::Giving,
            previous_first: Seat::ZERO,
            previous_second: None,
            givers: vec![Seat::THREE],
            contributions: IndexMap::new(),
            receiver_for_giver: IndexMap::from_iter([(Seat::THREE, Seat::ZERO)]),
            returns: IndexMap::new(),
            leader_seat: Some(Seat::THREE),
        }
    }

    #[test]
    fn partner_borrows_lead_after_unbeaten_final_play() {
        let mut round = simple_round(OrdinaryRank::Seven);
        assert!(play_only_card(&mut round, Seat::ZERO).is_none());
        pass_turn(&mut round, Seat::ONE).unwrap();
        pass_turn(&mut round, Seat::TWO).unwrap();
        pass_turn(&mut round, Seat::THREE).unwrap();
        assert_eq!(round.turn_seat, Seat::TWO);
        assert!(round.current_play.is_none());
    }

    #[test]
    fn partners_finishing_first_and_second_settle_double_last_immediately() {
        let mut round = simple_round(OrdinaryRank::Seven);
        play_only_card(&mut round, Seat::ZERO);
        pass_turn(&mut round, Seat::ONE).unwrap();
        pass_turn(&mut round, Seat::TWO).unwrap();
        pass_turn(&mut round, Seat::THREE).unwrap();
        let result = play_only_card(&mut round, Seat::TWO).unwrap();
        assert_eq!(result.finish_order, vec![Seat::ZERO, Seat::TWO]);
        assert_eq!(result.double_last_seats, vec![Seat::ONE, Seat::THREE]);
        assert_eq!(result.partner_placement, 2);
    }

    #[test]
    fn single_tribute_exchange_is_completed_and_giver_leads() {
        let mut round = simple_round(OrdinaryRank::Seven);
        let tribute_card = card(CardRank::BigJoker);
        let return_card = card(CardRank::Two);
        round.hands = [
            vec![return_card.clone(), card(CardRank::Jack)],
            vec![card(CardRank::Three)],
            vec![card(CardRank::Four)],
            vec![tribute_card.clone(), card(CardRank::Ten)],
        ];
        round.phase = RoundPhase::Tribute;
        round.tribute = Some(single_tribute_state());

        submit_tribute(&mut round, Seat::THREE, &tribute_card.id).unwrap();
        assert_eq!(
            round.tribute.as_ref().unwrap().stage,
            TributeStage::Returning
        );
        assert!(round.hands[0].iter().any(|card| card.id == tribute_card.id));

        let completed = submit_return(&mut round, Seat::ZERO, &return_card.id)
            .unwrap()
            .expect("the only return completes a single tribute");
        assert_eq!(completed.returns[&Seat::ZERO].id, return_card.id);
        assert_eq!(round.phase, RoundPhase::Playing);
        assert_eq!(round.turn_seat, Seat::THREE);
        assert!(round.tribute.is_none());
        assert!(round.hands[3].iter().any(|card| card.id == return_card.id));
    }

    #[test]
    fn equal_double_tributes_use_the_typescript_seat_tiebreak() {
        let first_card = card(CardRank::Ace);
        let second_card = card(CardRank::Ace);
        let mut round = simple_round(OrdinaryRank::Seven);
        round.hands = [
            vec![card(CardRank::Two)],
            vec![first_card.clone(), card(CardRank::King)],
            vec![card(CardRank::Three)],
            vec![second_card.clone(), card(CardRank::Queen)],
        ];
        round.phase = RoundPhase::Tribute;
        round.tribute = Some(TributeState {
            kind: TributeKind::Double,
            stage: TributeStage::Giving,
            previous_first: Seat::ZERO,
            previous_second: Some(Seat::TWO),
            givers: vec![Seat::ONE, Seat::THREE],
            contributions: IndexMap::new(),
            receiver_for_giver: IndexMap::new(),
            returns: IndexMap::new(),
            leader_seat: None,
        });

        submit_tribute(&mut round, Seat::ONE, &first_card.id).unwrap();
        submit_tribute(&mut round, Seat::THREE, &second_card.id).unwrap();
        let tribute = round.tribute.as_ref().unwrap();
        assert_eq!(tribute.receiver_for_giver[&Seat::THREE], Seat::ZERO);
        assert_eq!(tribute.receiver_for_giver[&Seat::ONE], Seat::TWO);
        assert_eq!(tribute.leader_seat, Some(Seat::THREE));
    }

    #[test]
    fn tribute_rejects_wildcard_and_nonmaximum_card() {
        let wildcard = suited_card(CardRank::Seven, Suit::Heart);
        let king = card(CardRank::King);
        let ace = card(CardRank::Ace);
        let mut round = simple_round(OrdinaryRank::Seven);
        round.phase = RoundPhase::Tribute;
        round.hands[3] = vec![wildcard.clone(), king.clone(), ace.clone()];
        round.tribute = Some(single_tribute_state());

        for (card, expected) in [
            (&wildcard, "WILDCARD_CANNOT_BE_TRIBUTE"),
            (&king, "TRIBUTE_NOT_MAXIMUM"),
        ] {
            let error = submit_tribute(&mut round, Seat::THREE, &card.id).unwrap_err();
            assert_eq!(error.code, expected);
        }
    }

    #[test]
    fn resistance_requires_both_big_jokers() {
        let mut hands: [Vec<Card>; 4] = std::array::from_fn(|_| Vec::new());
        hands[3] = vec![card(CardRank::BigJoker), card(CardRank::BigJoker)];
        let previous = RoundResult {
            winner_team: Team::ZERO,
            finish_order: vec![Seat::ZERO, Seat::ONE, Seat::TWO, Seat::THREE],
            double_last_seats: Vec::new(),
            partner_placement: 4,
        };
        assert!(create_tribute_state(&hands, &previous).unwrap().is_none());
    }

    #[test]
    fn third_and_fourth_partner_placements_are_computed() {
        let mut third = simple_round(OrdinaryRank::Seven);
        third.finish_order = vec![Seat::ZERO, Seat::ONE];
        third.active_seats = [Seat::TWO, Seat::THREE].into_iter().collect();
        third.turn_seat = Seat::TWO;
        third.current_play = None;
        let third_result = play_only_card(&mut third, Seat::TWO).unwrap();
        assert_eq!(third_result.partner_placement, 3);
        assert_eq!(third_result.finish_order[3], Seat::THREE);

        let mut fourth = simple_round(OrdinaryRank::Seven);
        fourth.finish_order = vec![Seat::ZERO, Seat::ONE];
        fourth.active_seats = [Seat::TWO, Seat::THREE].into_iter().collect();
        fourth.turn_seat = Seat::THREE;
        fourth.current_play = None;
        fourth.hands[Seat::THREE.index()] = vec![card(CardRank::Six)];
        let fourth_result = play_only_card(&mut fourth, Seat::THREE).unwrap();
        assert_eq!(fourth_result.partner_placement, 4);
        assert_eq!(fourth_result.finish_order[3], Seat::TWO);
    }

    #[test]
    fn cannot_pass_when_leading_or_play_out_of_turn() {
        let mut round = simple_round(OrdinaryRank::Seven);
        let error = pass_turn(&mut round, Seat::ZERO).unwrap_err();
        assert_eq!(error.code, "CANNOT_PASS_WHEN_LEADING");
        let card_id = round.hands[1][0].id.clone();
        let error = play_cards(&mut round, Seat::ONE, &[card_id], None).unwrap_err();
        assert_eq!(error.code, "NOT_YOUR_TURN");
    }

    #[test]
    fn low_return_cards_are_natural_ranks_not_effective_strength() {
        let low_level = suited_card(CardRank::Seven, Suit::Heart);
        let ace = card(CardRank::Ace);
        let can_return =
            |hand: &[Card], candidate: &Card| can_return_card(hand, candidate, OrdinaryRank::Seven);
        assert!(can_return(&[low_level.clone(), ace.clone()], &low_level));
        assert!(!can_return(&[low_level, ace.clone()], &ace));

        let king = card(CardRank::King);
        assert!(can_return(&[king.clone(), ace], &king));
    }

    #[test]
    fn first_round_consumes_shuffle_then_leader_randomness() {
        let mut calls = Vec::new();
        let round = create_round_with_random(
            CreateRoundOptions {
                number: 1,
                level_rank: OrdinaryRank::Two,
                level_owner_team: None,
                previous_result: None,
            },
            &mut |upper| {
                calls.push(upper);
                0
            },
        )
        .unwrap();
        assert_eq!(calls.len(), 108);
        assert_eq!(calls.last(), Some(&4));
        assert_eq!(round.turn_seat, Seat::ZERO);
        assert!(round.hands.iter().all(|hand| hand.len() == 27));
    }
}
