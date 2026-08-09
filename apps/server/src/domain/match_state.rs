use serde::{Deserialize, Serialize};

use super::errors::RuleError;
use super::round::{
    CreateRoundOptions, RoundResult, RoundState, TributeState, create_round,
    create_round_with_random, pass_turn, play_cards, submit_return, submit_tribute,
};
use super::types::{CombinationDeclaration, ORDINARY_RANKS, OrdinaryRank, Seat, Team};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MatchPhase {
    Playing,
    BetweenRounds,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchState {
    pub phase: MatchPhase,
    pub team_levels: [OrdinaryRank; 2],
    pub previous_round_result: Option<RoundResult>,
    pub current_round: Option<RoundState>,
    pub next_round_number: u32,
    pub next_level_rank: OrdinaryRank,
    pub next_level_owner_team: Option<Team>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchActionOutcome {
    pub round_result: Option<RoundResult>,
    pub match_winner: Option<Team>,
}

pub fn advance_level(rank: OrdinaryRank, steps: u8) -> OrdinaryRank {
    let next_index = (rank.index() + usize::from(steps)).min(ORDINARY_RANKS.len() - 1);
    ORDINARY_RANKS[next_index]
}

pub fn create_match() -> MatchState {
    let current_round = create_round(CreateRoundOptions {
        number: 1,
        level_rank: OrdinaryRank::Two,
        level_owner_team: None,
        previous_result: None,
    })
    .expect("a freshly-created round is internally valid");
    initial_match(current_round)
}

pub fn create_match_with_random<F>(random_index: &mut F) -> Result<MatchState, RuleError>
where
    F: FnMut(usize) -> usize + ?Sized,
{
    let current_round = create_round_with_random(
        CreateRoundOptions {
            number: 1,
            level_rank: OrdinaryRank::Two,
            level_owner_team: None,
            previous_result: None,
        },
        random_index,
    )?;
    Ok(initial_match(current_round))
}

fn initial_match(current_round: RoundState) -> MatchState {
    MatchState {
        phase: MatchPhase::Playing,
        team_levels: [OrdinaryRank::Two, OrdinaryRank::Two],
        previous_round_result: None,
        current_round: Some(current_round),
        next_round_number: 2,
        next_level_rank: OrdinaryRank::Two,
        next_level_owner_team: None,
    }
}

fn require_round(match_state: &mut MatchState) -> Result<&mut RoundState, RuleError> {
    if match_state.phase != MatchPhase::Playing {
        return Err(RuleError::new("NO_ACTIVE_ROUND", "当前没有进行中的轮牌"));
    }
    match_state
        .current_round
        .as_mut()
        .ok_or_else(|| RuleError::new("NO_ACTIVE_ROUND", "当前没有进行中的轮牌"))
}

fn settle_round(
    match_state: &mut MatchState,
    result: RoundResult,
) -> Result<MatchActionOutcome, RuleError> {
    let round = require_round(match_state)?;
    let passed_ace = round.level_rank == OrdinaryRank::Ace
        && round.level_owner_team == Some(result.winner_team)
        && result.partner_placement != 4;

    if passed_ace {
        return Ok(MatchActionOutcome {
            round_result: Some(result.clone()),
            match_winner: Some(result.winner_team),
        });
    }

    let steps = match result.partner_placement {
        2 => 3,
        3 => 2,
        _ => 1,
    };
    let winning_team = result.winner_team;
    let next_level = advance_level(match_state.team_levels[winning_team.index()], steps);
    match_state.team_levels[winning_team.index()] = next_level;
    match_state.previous_round_result = Some(result.clone());
    match_state.current_round = None;
    match_state.phase = MatchPhase::BetweenRounds;
    match_state.next_level_rank = next_level;
    match_state.next_level_owner_team = Some(winning_team);

    Ok(MatchActionOutcome {
        round_result: Some(result),
        match_winner: None,
    })
}

pub fn play_match_cards(
    match_state: &mut MatchState,
    seat: Seat,
    card_ids: &[String],
    declaration: Option<&CombinationDeclaration>,
) -> Result<MatchActionOutcome, RuleError> {
    let result = play_cards(require_round(match_state)?, seat, card_ids, declaration)?;
    match result {
        Some(result) => settle_round(match_state, result),
        None => Ok(MatchActionOutcome {
            round_result: None,
            match_winner: None,
        }),
    }
}

pub fn pass_match_turn(match_state: &mut MatchState, seat: Seat) -> Result<(), RuleError> {
    pass_turn(require_round(match_state)?, seat)
}

pub fn give_match_tribute(
    match_state: &mut MatchState,
    seat: Seat,
    card_id: &str,
) -> Result<(), RuleError> {
    submit_tribute(require_round(match_state)?, seat, card_id)
}

pub fn return_match_tribute(
    match_state: &mut MatchState,
    seat: Seat,
    card_id: &str,
) -> Result<Option<TributeState>, RuleError> {
    submit_return(require_round(match_state)?, seat, card_id)
}

pub fn start_next_round(match_state: &mut MatchState) -> Result<&RoundState, RuleError> {
    if match_state.phase != MatchPhase::BetweenRounds || match_state.current_round.is_some() {
        return Err(RuleError::new(
            "ROUND_ALREADY_ACTIVE",
            "当前并非轮牌间隔阶段",
        ));
    }
    let previous_result = match_state
        .previous_round_result
        .clone()
        .ok_or_else(|| RuleError::internal("Missing previous round result"))?;
    let round = create_round(CreateRoundOptions {
        number: match_state.next_round_number,
        level_rank: match_state.next_level_rank,
        level_owner_team: match_state.next_level_owner_team,
        previous_result: Some(previous_result),
    })?;
    install_next_round(match_state, round)
}

pub fn start_next_round_with_random<'a, F>(
    match_state: &'a mut MatchState,
    random_index: &mut F,
) -> Result<&'a RoundState, RuleError>
where
    F: FnMut(usize) -> usize + ?Sized,
{
    if match_state.phase != MatchPhase::BetweenRounds || match_state.current_round.is_some() {
        return Err(RuleError::new(
            "ROUND_ALREADY_ACTIVE",
            "当前并非轮牌间隔阶段",
        ));
    }
    let previous_result = match_state
        .previous_round_result
        .clone()
        .ok_or_else(|| RuleError::internal("Missing previous round result"))?;
    let round = create_round_with_random(
        CreateRoundOptions {
            number: match_state.next_round_number,
            level_rank: match_state.next_level_rank,
            level_owner_team: match_state.next_level_owner_team,
            previous_result: Some(previous_result),
        },
        random_index,
    )?;
    install_next_round(match_state, round)
}

fn install_next_round(
    match_state: &mut MatchState,
    round: RoundState,
) -> Result<&RoundState, RuleError> {
    match_state.current_round = Some(round);
    match_state.phase = MatchPhase::Playing;
    match_state.next_round_number += 1;
    match_state
        .current_round
        .as_ref()
        .ok_or_else(|| RuleError::internal("Failed to install next round"))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use indexmap::IndexSet;

    use super::*;
    use crate::domain::round::{RoundPhase, pass_turn};
    use crate::domain::types::{Card, CardRank, Suit};

    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    fn card(rank: CardRank) -> Card {
        Card {
            id: format!("match-card-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed)),
            deck_index: 0,
            rank,
            suit: if matches!(rank, CardRank::SmallJoker | CardRank::BigJoker) {
                Suit::Joker
            } else {
                Suit::Spade
            },
        }
    }

    fn simple_round(level_rank: OrdinaryRank) -> RoundState {
        RoundState {
            number: 1,
            level_rank,
            level_owner_team: None,
            phase: RoundPhase::Playing,
            hands: [
                vec![card(CardRank::Three)],
                vec![card(CardRank::Four), card(CardRank::Eight)],
                vec![card(CardRank::Five)],
                vec![card(CardRank::Six), card(CardRank::Nine)],
            ],
            active_seats: Seat::all().into_iter().collect::<IndexSet<_>>(),
            turn_seat: Seat::ZERO,
            current_play: None,
            consecutive_passes: 0,
            finish_order: Vec::new(),
            tribute: None,
        }
    }

    fn play_only_card(match_state: &mut MatchState, seat: Seat) -> MatchActionOutcome {
        let card_id = match_state.current_round.as_ref().unwrap().hands[seat.index()][0]
            .id
            .clone();
        play_match_cards(match_state, seat, &[card_id], None).unwrap()
    }

    #[test]
    fn level_advancement_caps_at_ace() {
        assert_eq!(advance_level(OrdinaryRank::Two, 3), OrdinaryRank::Five);
        assert_eq!(advance_level(OrdinaryRank::Queen, 3), OrdinaryRank::Ace);
        assert_eq!(advance_level(OrdinaryRank::Ace, 3), OrdinaryRank::Ace);
    }

    #[test]
    fn playing_ace_wins_when_partner_is_not_last() {
        let mut match_state = create_match_with_random(&mut |_| 0).unwrap();
        match_state.team_levels[0] = OrdinaryRank::Ace;
        let mut round = simple_round(OrdinaryRank::Ace);
        round.level_owner_team = Some(Team::ZERO);
        match_state.current_round = Some(round);

        play_only_card(&mut match_state, Seat::ZERO);
        pass_match_turn(&mut match_state, Seat::ONE).unwrap();
        pass_match_turn(&mut match_state, Seat::TWO).unwrap();
        pass_match_turn(&mut match_state, Seat::THREE).unwrap();
        let outcome = play_only_card(&mut match_state, Seat::TWO);
        assert_eq!(outcome.match_winner, Some(Team::ZERO));
    }

    #[test]
    fn failing_to_pass_ace_stays_at_ace_and_moves_between_rounds() {
        let mut match_state = create_match_with_random(&mut |_| 0).unwrap();
        match_state.team_levels[0] = OrdinaryRank::Ace;
        let mut round = simple_round(OrdinaryRank::Ace);
        round.level_owner_team = Some(Team::ZERO);
        match_state.current_round = Some(round);

        let result = RoundResult {
            winner_team: Team::ZERO,
            finish_order: vec![Seat::ZERO, Seat::ONE, Seat::THREE, Seat::TWO],
            double_last_seats: Vec::new(),
            partner_placement: 4,
        };
        let outcome = settle_round(&mut match_state, result).unwrap();
        assert_eq!(outcome.match_winner, None);
        assert_eq!(match_state.team_levels[0], OrdinaryRank::Ace);
        assert_eq!(match_state.phase, MatchPhase::BetweenRounds);
        assert!(match_state.current_round.is_none());
    }

    #[test]
    fn settlement_advances_by_partner_placement() {
        for (placement, expected) in [
            (2, OrdinaryRank::Five),
            (3, OrdinaryRank::Four),
            (4, OrdinaryRank::Three),
        ] {
            let mut match_state = create_match_with_random(&mut |_| 0).unwrap();
            let result = RoundResult {
                winner_team: Team::ZERO,
                finish_order: if placement == 2 {
                    vec![Seat::ZERO, Seat::TWO]
                } else {
                    vec![Seat::ZERO, Seat::ONE, Seat::TWO, Seat::THREE]
                },
                double_last_seats: if placement == 2 {
                    vec![Seat::ONE, Seat::THREE]
                } else {
                    Vec::new()
                },
                partner_placement: placement,
            };
            settle_round(&mut match_state, result).unwrap();
            assert_eq!(match_state.team_levels[0], expected);
        }
    }

    #[test]
    fn next_round_uses_pending_level_and_increments_number() {
        let mut match_state = create_match_with_random(&mut |_| 0).unwrap();
        let result = RoundResult {
            winner_team: Team::ZERO,
            finish_order: vec![Seat::ZERO, Seat::ONE, Seat::THREE, Seat::TWO],
            double_last_seats: Vec::new(),
            partner_placement: 4,
        };
        settle_round(&mut match_state, result).unwrap();
        let round = start_next_round_with_random(&mut match_state, &mut |_| 0).unwrap();
        assert_eq!(round.number, 2);
        assert_eq!(round.level_rank, OrdinaryRank::Three);
        assert_eq!(round.level_owner_team, Some(Team::ZERO));
        assert_eq!(match_state.next_round_number, 3);
        assert_eq!(match_state.phase, MatchPhase::Playing);
    }

    #[test]
    fn wrappers_reject_actions_between_rounds() {
        let mut match_state = create_match_with_random(&mut |_| 0).unwrap();
        match_state.phase = MatchPhase::BetweenRounds;
        match_state.current_round = None;
        assert_eq!(
            pass_match_turn(&mut match_state, Seat::ZERO)
                .unwrap_err()
                .code,
            "NO_ACTIVE_ROUND"
        );
    }

    #[test]
    fn match_wire_phase_is_between_rounds() {
        assert_eq!(
            serde_json::to_string(&MatchPhase::BetweenRounds).unwrap(),
            "\"between-rounds\""
        );
    }

    #[test]
    fn imported_round_pass_helper_is_consistent() {
        let mut round = simple_round(OrdinaryRank::Seven);
        assert_eq!(
            pass_turn(&mut round, Seat::ZERO).unwrap_err().code,
            "CANNOT_PASS_WHEN_LEADING"
        );
    }
}
