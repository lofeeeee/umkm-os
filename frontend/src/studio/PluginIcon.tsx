// Ikon khas per plugin (katalog, inspektur, toolbar) — supaya plugin dikenali sekilas, bukan ikon puzzle seragam.

import {
  ChatCircleDots, Clock, Images, Info, MapPin, Megaphone, Phone, Quotes, Storefront, TextAa, WhatsappLogo, Image as ImageIcon,
  PuzzlePiece,
} from "@phosphor-icons/react/ssr";

import type { PluginKind } from "./plugins";

const ICONS: Partial<Record<PluginKind, typeof PuzzlePiece>> = {
  hero: ImageIcon,
  product_grid: Storefront,
  chatbot: ChatCircleDots,
  promotion: Megaphone,
  about: Info,
  gallery: Images,
  testimonials: Quotes,
  business_hours: Clock,
  location: MapPin,
  text: TextAa,
  whatsapp: WhatsappLogo,
  contact: Phone,
};

export function PluginIcon({ kind, size = 18 }: { kind: PluginKind; size?: number }) {
  const Icon = ICONS[kind] ?? PuzzlePiece;
  return <Icon weight="duotone" size={size} aria-hidden="true" />;
}
