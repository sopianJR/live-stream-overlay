import type { ReactNode } from "react";
import { Rnd } from "react-rnd";
import type { WidgetId, WidgetRect } from "../shared/types";

interface Props {
  id: WidgetId;
  title: string;
  rect: WidgetRect;
  viewport: { w: number; h: number };
  editMode: boolean;
  minWidth: number;
  minHeight: number;
  onChange: (id: WidgetId, rect: WidgetRect) => void;
  children: ReactNode;
}

/** Bingkai widget: diam di mode pasif, bisa diseret & diubah ukurannya di mode edit. */
export function WidgetFrame({
  id,
  title,
  rect,
  viewport,
  editMode,
  minWidth,
  minHeight,
  onChange,
  children,
}: Props) {
  const { w: vw, h: vh } = viewport;

  return (
    <Rnd
      bounds="parent"
      size={{ width: rect.w * vw, height: rect.h * vh }}
      position={{ x: rect.x * vw, y: rect.y * vh }}
      minWidth={minWidth}
      minHeight={minHeight}
      disableDragging={!editMode}
      enableResizing={editMode}
      className={editMode ? "widget-frame editing" : "widget-frame"}
      onDragStop={(_e, d) => onChange(id, { ...rect, x: d.x / vw, y: d.y / vh })}
      onResizeStop={(_e, _dir, ref, _delta, pos) =>
        onChange(id, {
          x: pos.x / vw,
          y: pos.y / vh,
          w: ref.offsetWidth / vw,
          h: ref.offsetHeight / vh,
        })
      }
    >
      {editMode && <div className="widget-label">{title}</div>}
      <div className="widget-body">{children}</div>
    </Rnd>
  );
}
