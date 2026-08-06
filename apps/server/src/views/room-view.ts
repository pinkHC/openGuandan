import { sortCards } from "../domain/cards.js";
import type { Card, PlayerId, Seat } from "../domain/types.js";
import type { RoundState, TributeState } from "../domain/round.js";
import type { Participant, RoomState } from "../rooms/types.js";

function isConnected(participant: Participant): boolean {
  return participant.socketIds.size > 0;
}

function cardsBySeat(round: RoundState): Record<string, number> {
  return Object.fromEntries(
    ([0, 1, 2, 3] as const).map((seat) => [seat, round.hands.get(seat)?.length ?? 0]),
  );
}

function visibleSubmittedCards(
  values: ReadonlyMap<Seat, Card>,
  reveal: boolean,
): Array<{ seat: Seat; card: Card }> {
  if (!reveal) return [];
  return [...values.entries()].map(([seat, card]) => ({ seat, card }));
}

function tributeView(tribute: TributeState | null): unknown {
  if (tribute === null) return null;
  const contributionsComplete = tribute.contributions.size === tribute.givers.length;
  const returnsComplete =
    tribute.stage === "returning" && tribute.returns.size === tribute.receiverForGiver.size;
  return {
    kind: tribute.kind,
    stage: tribute.stage,
    previousFirst: tribute.previousFirst,
    previousSecond: tribute.previousSecond,
    givers: tribute.givers,
    receiverForGiver: Object.fromEntries(tribute.receiverForGiver),
    contributedSeats: [...tribute.contributions.keys()],
    returnedSeats: [...tribute.returns.keys()],
    contributions: visibleSubmittedCards(
      tribute.contributions,
      tribute.kind === "single" || contributionsComplete,
    ),
    returns: visibleSubmittedCards(tribute.returns, returnsComplete),
  };
}

function publicRoundView(round: RoundState): unknown {
  return {
    number: round.number,
    phase: round.phase,
    levelRank: round.levelRank,
    levelOwnerTeam: round.levelOwnerTeam,
    turnSeat: round.turnSeat,
    currentPlay: round.currentPlay,
    consecutivePasses: round.consecutivePasses,
    finishOrder: round.finishOrder,
    activeSeats: [...round.activeSeats],
    handCounts: cardsBySeat(round),
    tribute: tributeView(round.tribute),
  };
}

export function createRoomView(room: RoomState, viewerId?: PlayerId): unknown {
  const viewer = viewerId === undefined ? undefined : room.participants.get(viewerId);
  const currentRound = room.match?.currentRound ?? null;

  const self =
    viewer === undefined
      ? null
      : {
          participantId: viewer.id,
          role: viewer.role,
          seat: viewer.seat,
          ready: viewer.ready,
          hand:
            viewer.seat !== null && currentRound !== null
              ? sortCards(currentRound.hands.get(viewer.seat) ?? [], currentRound.levelRank)
              : [],
        };

  return {
    roomCode: room.code,
    phase: room.phase,
    version: room.version,
    hostId: room.hostId,
    seats: room.seats,
    participants: [...room.participants.values()].map((participant) => ({
      id: participant.id,
      displayName: participant.displayName,
      role: participant.role,
      seat: participant.seat,
      ready: participant.ready,
      connected: isConnected(participant),
    })),
    match:
      room.match === null
        ? null
        : {
            phase: room.match.phase,
            teamLevels: room.match.teamLevels,
            nextRoundNumber: room.match.nextRoundNumber,
            previousRoundResult: room.match.previousRoundResult,
            currentRound: currentRound === null ? null : publicRoundView(currentRound),
          },
    self,
  };
}
