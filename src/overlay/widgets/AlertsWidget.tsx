import { ipc } from "../../shared/ipc";
import { ALERT_ICON, useAlerts } from "../../shared/useAlerts";

/** Banner yang ditampilkan sekaligus; sisanya diringkas agar tidak menutupi game. */
const MAX_VISIBLE = 3;

export function AlertsWidget({ editMode }: { editMode: boolean }) {
  const alerts = useAlerts();
  const shown = alerts.slice(0, MAX_VISIBLE);
  const hidden = alerts.length - shown.length;

  return (
    <div className="alerts">
      {shown.map((a) => (
        // `seq` di key: alert yang muncul lagi diberi animasi masuk lagi.
        <div key={`${a.id}-${a.seq}`} className={`alert ${a.level}`} role="alert">
          <span className="alert-icon" aria-hidden>
            {ALERT_ICON[a.level]}
          </span>
          <div className="alert-body">
            <strong>{a.title}</strong>
            {a.detail && <small>{a.detail}</small>}
          </div>
          {editMode && (
            <button type="button" className="no-drag" onClick={() => void ipc.alertsDismiss(a.id)}>
              Tutup
            </button>
          )}
        </div>
      ))}
      {hidden > 0 && <div className="alert-more">+{hidden} alert lain</div>}
      {editMode && alerts.length === 0 && (
        <div className="alert-placeholder">
          Area alert. Tekan <strong>Tes alert</strong> di dashboard → Alert untuk melihat contoh.
        </div>
      )}
    </div>
  );
}
