import type { Metadata } from "next";
import { RoomClient } from "../../components/RoomClient";

export const metadata: Metadata = { title: "牌桌" };

export default async function RoomPage({ params }: { params: Promise<{ code: string }> }) {
  const { code } = await params;
  return <RoomClient roomCode={decodeURIComponent(code).toUpperCase()} />;
}
