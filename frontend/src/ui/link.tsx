"use client";

// Link dengan prefetch berbasis niat (UAT 15 Nov perf #1/#6): Next.js default mem-prefetch
// setiap tautan yang terlihat — sidebar saja 17 rute → ~95 payload HTML/RSC per muat, dan di
// jaringan lambat halaman tak pernah idle. Di sini prefetch baru aktif saat pointer/fokus/
// sentuhan menyentuh tautan (pola resmi "HoverPrefetchLink" di panduan prefetching Next.js).

import NextLink from "next/link";
import { forwardRef, useState, type ComponentProps } from "react";

type Props = ComponentProps<typeof NextLink>;

const IntentLink = forwardRef<HTMLAnchorElement, Props>(function IntentLink(
  { prefetch, onMouseEnter, onFocus, onTouchStart, ...rest },
  ref,
) {
  const [active, setActive] = useState(false);
  return (
    <NextLink
      ref={ref}
      {...rest}
      prefetch={prefetch !== undefined ? prefetch : active ? null : false}
      onMouseEnter={(e) => { setActive(true); onMouseEnter?.(e); }}
      onFocus={(e) => { setActive(true); onFocus?.(e); }}
      onTouchStart={(e) => { setActive(true); onTouchStart?.(e); }}
    />
  );
});

export default IntentLink;
