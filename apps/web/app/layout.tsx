import type { Metadata } from "next";
import { headers } from "next/headers";
import "./globals.css";

export async function generateMetadata(): Promise<Metadata> {
  const requestHeaders = await headers();
  const host = requestHeaders.get("x-forwarded-host") ?? requestHeaders.get("host");
  const forwardedProtocol = requestHeaders.get("x-forwarded-proto");
  let metadataBase = new URL("http://localhost:5174");
  if (host) {
    try { metadataBase = new URL(`${forwardedProtocol === "http" ? "http" : "https"}://${host}`); }
    catch { /* Retain the safe local fallback for malformed Host headers. */ }
  }
  return {
    metadataBase,
    title: { default: "openGuandan｜和搭档打好每一手牌", template: "%s｜openGuandan" },
    description: "无需注册，创建房间，与三位朋友在线打掼蛋。",
    openGraph: {
      title: "openGuandan",
      description: "和搭档一起，把这一手打漂亮。",
      type: "website",
      images: [{ url: "/og.png", width: 1536, height: 1024, alt: "openGuandan 墨绿牌桌" }],
    },
    twitter: { card: "summary_large_image", images: ["/og.png"] },
  };
}

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="zh-CN">
      <body>{children}</body>
    </html>
  );
}
