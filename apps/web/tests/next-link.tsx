import type { AnchorHTMLAttributes, ReactNode } from "react";

export default function Link({ href, children, prefetch: _prefetch, ...props }: AnchorHTMLAttributes<HTMLAnchorElement> & { href: string; children: ReactNode; prefetch?: boolean }) {
  return <a href={href} {...props}>{children}</a>;
}
