import { z } from "zod";
import { COMBINATION_KINDS, ORDINARY_RANKS } from "../domain/types.js";

export const displayNameSchema = z.string().trim().min(1).max(20);
export const roomCodeSchema = z.string().trim().min(4).max(12).transform((value) => value.toUpperCase());

export const createRoomSchema = z.object({ displayName: displayNameSchema }).strict();
export const joinRoomSchema = z.object({ displayName: displayNameSchema }).strict();

export const socketAuthSchema = z
  .object({
    roomCode: roomCodeSchema,
    participantId: z.string().uuid(),
    reconnectToken: z.string().min(32).max(128),
  })
  .strict();

const actionEnvelopeSchema = z.object({
  actionId: z.string().min(8).max(128),
  version: z.number().int().nonnegative(),
});

export const readySchema = actionEnvelopeSchema.extend({ ready: z.boolean() }).strict();
export const simpleActionSchema = actionEnvelopeSchema.strict();

const declarationSchema = z
  .object({
    kind: z.enum(COMBINATION_KINDS),
    primaryRank: z
      .enum([...ORDINARY_RANKS, "small-joker", "big-joker"] as const)
      .optional(),
    sequenceTop: z.enum(ORDINARY_RANKS).optional(),
  })
  .strict();

export const playCardsSchema = actionEnvelopeSchema
  .extend({
    cardIds: z.array(z.string().min(1).max(80)).min(1).max(10),
    declaration: declarationSchema.optional(),
  })
  .strict();

export const cardActionSchema = actionEnvelopeSchema
  .extend({ cardId: z.string().min(1).max(80) })
  .strict();
