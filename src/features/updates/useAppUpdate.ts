import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { UpdateOffer } from "../../types";
import {
  checkForAppUpdate,
  discardPreparedUpdate,
  prepareAutoInstall,
} from "../../services/updater";

// Owns the update badge/modal lifecycle: background check, serialized
// prepare/discard bookkeeping, and stale-request suppression. `release`
// non-null is the availability badge; `visible` gates the modal.
export function useAppUpdate() {
  const [release, setRelease] = useState<UpdateOffer | null>(null);
  const [visible, setVisible] = useState(false);
  const [ready, setReady] = useState(false);
  const [preparationError, setPreparationError] = useState<string | null>(null);
  const preparedVersionRef = useRef<string | null>(null);
  const pendingVersionRef = useRef<string | null>(null);
  // Prepare and discard serialize on one chain so an in-flight preparation
  // always settles before its discard, and a discard precedes a reopened
  // preparation. The request id makes every stale completion a no-op,
  // including a stale completion for the same version.
  const opsRef = useRef<Promise<void>>(Promise.resolve());
  const requestIdRef = useRef(0);

  const enqueue = (work: () => Promise<void>) => {
    opsRef.current = opsRef.current.then(work).catch(() => {});
  };

  // Serialized preparation shared by manual open and the background check;
  // only a successful background prepare reveals the modal automatically.
  const prepare = (offer: UpdateOffer) => {
    setRelease(offer);

    if (preparedVersionRef.current === offer.version) {
      setReady(true);
      return;
    }
    // A prepare for this version is already queued or in flight; the
    // serialized chain completes it rather than issuing a duplicate request.
    if (pendingVersionRef.current === offer.version) {
      return;
    }
    const requestId = ++requestIdRef.current;
    // The native prepared slot is replaced by this request; readiness for the
    // previously cached version must not carry over.
    preparedVersionRef.current = null;
    pendingVersionRef.current = offer.version;
    setReady(false);
    setPreparationError(null);
    enqueue(async () => {
      try {
        await prepareAutoInstall(offer.version);
        if (requestIdRef.current !== requestId) return;
        pendingVersionRef.current = null;
        preparedVersionRef.current = offer.version;
        setReady(true);
        setVisible(true);
      } catch (error) {
        if (requestIdRef.current !== requestId) return;
        pendingVersionRef.current = null;
        const message = error instanceof Error ? error.message : String(error);
        setPreparationError(message || "Failed to prepare the update in the background.");
      }
    });
  };

  const open = (offer: UpdateOffer) => {
    setVisible(true);
    prepare(offer);
  };

  // "Later"/X frees the verified download while the app stays open; the
  // update-available badge is kept so the user can reopen and re-prepare.
  const dismiss = () => {
    requestIdRef.current += 1;
    const version = release?.version ?? null;
    preparedVersionRef.current = null;
    setReady(false);
    setPreparationError(null);
    setVisible(false);
    if (version) {
      pendingVersionRef.current = null;
      enqueue(async () => {
        await discardPreparedUpdate(version).catch(() => {});
      });
    }
  };

  useEffect(() => {
    let cancelled = false;
    // If this process was launched by the updater, the parent is waiting on
    // the handshake socket; confirm once React has committed.
    void invoke("confirm_update_startup").catch(() => {});
    checkForAppUpdate()
      .then((offer) => {
        if (!cancelled && offer) prepare(offer);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return { release, visible, ready, preparationError, open, dismiss };
}
