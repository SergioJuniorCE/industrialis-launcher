import { useEffect, useRef, type ReactNode } from "react";

export default function Modal({ onClose, label, labelledBy, children }: { onClose: () => void; label?: string; labelledBy?: string; children: ReactNode }) {
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = ref.current;
    dialog?.showModal();
    return () => dialog?.close();
  }, []);

  return (
    <dialog
      ref={ref}
      aria-label={label}
      aria-labelledby={labelledBy}
      onCancel={onClose}
      className="fixed inset-0 m-auto max-h-[100dvh] w-full max-w-none border-0 bg-transparent p-3 text-copy backdrop:bg-black/70"
    >
      {children}
    </dialog>
  );
}
