import Link from "next/link";

export function Brand({ compact = false }: { compact?: boolean }) {
  return (
    <Link prefetch={false} className={`brand ${compact ? "brand--compact" : ""}`} href="/" aria-label="openGuandan 首页">
      <span className="brand__mark" aria-hidden="true"><i>♠</i><i>♥</i></span>
      <span><b>open</b>Guandan</span>
    </Link>
  );
}
