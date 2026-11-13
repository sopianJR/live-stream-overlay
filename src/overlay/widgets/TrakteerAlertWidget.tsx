import { ipc } from "../../shared/ipc";
import type { TrakteerSettings } from "../../shared/types";
import { useDonationQueue } from "../../shared/useTrakteer";

const DEFAULT_AVATAR = "https://edge-cdn.trakteer.id/images/mix/default-avatar.png";

const KIND_BADGE = { live: null, test: "TEST", sample: "CONTOH" } as const;

/** Kartu alert donasi Trakteer (hanya terlihat streamer; overlay tersembunyi dari capture). */
export function TrakteerAlertWidget({
  editMode,
  settings,
}: {
  editMode: boolean;
  settings: TrakteerSettings | undefined;
}) {
  const { current, phase, pending } = useDonationQueue(settings?.alertSeconds ?? 8);
  const showMessage = settings?.showMessage ?? true;

  if (!current) {
    return editMode ? (
      <div className="donation-placeholder">
        <span>Kartu donasi Trakteer muncul di sini.</span>
        <button type="button" className="no-drag" onClick={() => void ipc.trakteerTestDonation()}>
          Tes donasi
        </button>
      </div>
    ) : null;
  }

  const d = current.donation;
  const badge = KIND_BADGE[current.kind];

  return (
    <div className="donation-wrap">
      <div className={`donation-card ${phase} kind-${current.kind}`} role="status">
        <img
          className="donation-avatar"
          src={d.avatarUrl || DEFAULT_AVATAR}
          alt=""
          draggable={false}
          onError={(e) => {
            const img = e.currentTarget;
            if (img.src !== DEFAULT_AVATAR) img.src = DEFAULT_AVATAR;
          }}
        />
        <div className="donation-body">
          <div className="donation-head">
            <span className="donation-name">{d.supporterName}</span>
            {badge && <span className="donation-badge">{badge}</span>}
          </div>
          <div className="donation-amount">
            Mentraktir {d.quantity}× {d.unit} · <strong>{d.priceText}</strong>
          </div>
          {showMessage && d.message && <p className="donation-message">{d.message}</p>}
        </div>
        {d.unitIconUrl && <img className="donation-unit" src={d.unitIconUrl} alt="" draggable={false} />}
      </div>
      {pending > 0 && <div className="donation-pending">+{pending} donasi antre</div>}
    </div>
  );
}
