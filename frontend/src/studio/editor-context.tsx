"use client";

// Jembatan antara kanvas Studio (LiveCanvas) dan renderer situs (`StudioSite` mode `studio-edit`).
// Renderer tidak tahu apa pun tentang drag/resize/kuas: ia hanya membaca konteks ini lewat
// `EditableBlockWrapper`, sehingga situs terbit (mode `customer`) tidak menyentuh kode editor.

import { createContext, useContext, type ReactNode } from "react";

export type GestureMode = "move" | "resize-x" | "resize-y";

export interface BlockEditor {
  /** Blok yang sedang diseret/diubah ukurannya. */
  activeId: string | null;
  /** Blok terkunci (tidak bisa diseret). */
  isLocked: (id: string) => boolean;
  /** Mulai gerakan (seret/ubah ukuran) dari pointerdown pada blok atau pegangannya. */
  startGesture: (id: string, mode: GestureMode, e: React.PointerEvent<HTMLElement>) => void;
  /** Tombol-tombol aksi di bilah atas blok terpilih. */
  toolbar?: (id: string) => ReactNode;
  /** Foto dari komputer dijatuhkan ke blok. */
  acceptsFile?: (id: string) => boolean;
  onDropFile?: (id: string, file: File) => void;
  /** Chatbot: buka/tutup pratinjau (disimpan sebagai `chat_open`). */
  onChatOpen?: (id: string, open: boolean) => void;
  /** Status chatbot nyata dari Asisten AI (null = belum diketahui). */
  chatReady?: boolean | null;
  /** Lapisan tambahan di atas grid (kuas, garis sisip saat menyeret). */
  gridOverlay?: ReactNode;
  /** Mode kuas/penghapus aktif: blok tidak menangkap klik. */
  drawing?: boolean;
}

export const BlockEditorContext = createContext<BlockEditor | null>(null);
export const useBlockEditor = () => useContext(BlockEditorContext);
