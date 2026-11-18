// Plugin Logo: logo & nama toko dari Profil bisnis, bebas diletakkan. Satu sumber logo (profil) sehingga tidak
// pernah berbeda antarhalaman; bila plugin ini ada, logo di navigasi disembunyikan (tidak dobel).

import { LogoMark } from "./LogoMark";
import type { LayerBlock } from "./layers";
import { safeHref } from "./safe";

export function LogoLayer({ b, name, logoUrl, editing }: { b: LayerBlock; name: string; logoUrl?: string; editing?: boolean }) {
  const body = (
    <>
      <LogoMark url={logoUrl} name={name} className="sv2-logo-mark" size={96} />
      {b.data.show_name !== false && <span className="sv2-logo-name">{name}</span>}
    </>
  );
  const href = editing ? undefined : safeHref(b.data.href) ?? "/";
  return href
    ? <a className="sv2-logo" href={href} aria-label={name}>{body}</a>
    : <div className="sv2-logo">{body}</div>;
}
